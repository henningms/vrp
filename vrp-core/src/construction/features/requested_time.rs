//! Provides a feature to minimize deviation from requested service times.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/requested_time_test.rs"]
mod requested_time_test;

use super::*;
use crate::construction::enablers::calculate_travel;
use crate::models::problem::TransportCost;
use crate::models::solution::Activity;
use std::collections::HashMap;
use std::sync::Arc;

/// Stores requested times for each place index in a job.
/// Key is the place index, value is the requested service-start timestamp.
pub type RequestedTimes = HashMap<usize, Timestamp>;

custom_dimension!(pub JobRequestedTimes typeof RequestedTimes);

/// Penalty configuration for requested time deviations.
#[derive(Clone, Debug)]
pub struct RequestedTimePenalty {
    /// Penalty per second for starting service early (before requested time).
    pub early_penalty_per_second: Cost,
    /// Penalty per second for starting service late (after requested time).
    pub late_penalty_per_second: Cost,
}

impl Default for RequestedTimePenalty {
    fn default() -> Self {
        Self {
            // Default: 1.0 penalty per minute = 1/60 per second
            early_penalty_per_second: 1.0 / 60.0,
            late_penalty_per_second: 1.0 / 60.0,
        }
    }
}

impl RequestedTimePenalty {
    /// Creates a new penalty configuration with penalties specified per minute.
    pub fn new(early_penalty_per_minute: Cost, late_penalty_per_minute: Cost) -> Self {
        Self {
            early_penalty_per_second: early_penalty_per_minute / 60.0,
            late_penalty_per_second: late_penalty_per_minute / 60.0,
        }
    }

    /// Calculates the penalty for a given deviation from requested time.
    fn calculate_penalty(&self, service_start: Timestamp, requested: Timestamp) -> Cost {
        if service_start < requested {
            // Early service
            (requested - service_start) * self.early_penalty_per_second
        } else {
            // Late service (or on time = 0 penalty)
            (service_start - requested) * self.late_penalty_per_second
        }
    }
}

/// Creates a feature that minimizes deviation from requested service-start times.
///
/// Jobs with requested times specified (via `JobRequestedTimes` dimension) will be
/// penalized based on how far the actual service start deviates from the requested time.
pub fn create_requested_time_feature(
    name: &str,
    penalty: RequestedTimePenalty,
    transport: Arc<dyn TransportCost>,
) -> GenericResult<Feature> {
    FeatureBuilder::default()
        .with_name(name)
        .with_objective(RequestedTimeObjective { penalty: Arc::new(penalty), transport })
        .build()
}

struct RequestedTimeObjective {
    penalty: Arc<RequestedTimePenalty>,
    transport: Arc<dyn TransportCost>,
}

impl FeatureObjective for RequestedTimeObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        solution
            .solution
            .routes
            .iter()
            .flat_map(|route_ctx| {
                route_ctx.route().tour.all_activities().filter_map(|activity| self.calculate_activity_penalty(activity))
            })
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        match move_ctx {
            MoveContext::Route { .. } => Cost::default(),
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                let (_, (prev_to_tar_dur, _)) = calculate_travel(route_ctx, activity_ctx, self.transport.as_ref());
                let target = activity_ctx.target;
                let target_arrival = activity_ctx.prev.schedule.departure + prev_to_tar_dur;
                let target_service_start = target_arrival.max(target.place.time.start);
                let mut delta = self
                    .calculate_activity_penalty_with_service_start(target, target_service_start)
                    .unwrap_or_default();

                // An insertion can delay every downstream requested-time activity.
                // Include that full delta instead of scoring only the new target.
                let route = route_ctx.route();
                let mut location = target.place.location;
                let mut departure = target_service_start + target.place.duration;
                for idx in activity_ctx.index + 1..route.tour.total() {
                    let Some(activity) = route.tour.get(idx) else { continue };
                    delta -= self.calculate_activity_penalty(activity).unwrap_or_default();
                    let arrival = departure
                        + self.transport.duration(
                            route,
                            location,
                            activity.place.location,
                            crate::models::problem::TravelTime::Departure(departure),
                        );
                    let service_start = arrival.max(activity.place.time.start);
                    delta +=
                        self.calculate_activity_penalty_with_service_start(activity, service_start).unwrap_or_default();
                    departure = service_start + activity.place.duration;
                    location = activity.place.location;
                }

                delta
            }
        }
    }
}

impl RequestedTimeObjective {
    /// Calculates penalty for an activity using its scheduled service-start time.
    fn calculate_activity_penalty(&self, activity: &Activity) -> Option<Cost> {
        let service_start = activity.schedule.arrival.max(activity.place.time.start);
        self.calculate_activity_penalty_with_service_start(activity, service_start)
    }

    /// Calculates penalty for an activity with a given service-start time.
    fn calculate_activity_penalty_with_service_start(
        &self,
        activity: &Activity,
        service_start: Timestamp,
    ) -> Option<Cost> {
        let single = activity.job.as_ref()?;
        let requested_times = single.dimens.get_job_requested_times()?;
        let requested_time = requested_times.get(&activity.place.idx)?;

        Some(self.penalty.calculate_penalty(service_start, *requested_time))
    }
}
