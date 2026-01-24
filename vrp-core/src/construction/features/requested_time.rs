//! Provides a feature to minimize deviation from requested arrival times.

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
/// Key is the place index, value is the requested arrival timestamp.
pub type RequestedTimes = HashMap<usize, Timestamp>;

custom_dimension!(pub JobRequestedTimes typeof RequestedTimes);

/// Penalty configuration for requested time deviations.
#[derive(Clone, Debug)]
pub struct RequestedTimePenalty {
    /// Penalty per second for arriving early (before requested time).
    pub early_penalty_per_second: Cost,
    /// Penalty per second for arriving late (after requested time).
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
    fn calculate_penalty(&self, arrival: Timestamp, requested: Timestamp) -> Cost {
        if arrival < requested {
            // Early arrival
            (requested - arrival) * self.early_penalty_per_second
        } else {
            // Late arrival (or on time = 0 penalty)
            (arrival - requested) * self.late_penalty_per_second
        }
    }
}

/// Creates a feature that minimizes deviation from requested arrival times.
///
/// Jobs with requested times specified (via `JobRequestedTimes` dimension) will be
/// penalized based on how far the actual arrival deviates from the requested time.
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
                route_ctx.route().tour.all_activities().filter_map(|activity| {
                    self.calculate_activity_penalty(activity)
                })
            })
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        match move_ctx {
            MoveContext::Route { .. } => Cost::default(),
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                // Calculate actual arrival time based on travel from previous activity
                let (_, (prev_to_tar_dur, _)) = calculate_travel(route_ctx, activity_ctx, self.transport.as_ref());
                let arrival = activity_ctx.prev.schedule.departure + prev_to_tar_dur;

                self.calculate_activity_penalty_with_arrival(activity_ctx.target, arrival)
                    .unwrap_or_default()
            }
        }
    }
}

impl RequestedTimeObjective {
    /// Calculates penalty for an activity using its scheduled arrival time.
    fn calculate_activity_penalty(&self, activity: &Activity) -> Option<Cost> {
        self.calculate_activity_penalty_with_arrival(activity, activity.schedule.arrival)
    }

    /// Calculates penalty for an activity with a given arrival time.
    fn calculate_activity_penalty_with_arrival(
        &self,
        activity: &Activity,
        arrival: Timestamp,
    ) -> Option<Cost> {
        let single = activity.job.as_ref()?;
        let requested_times = single.dimens.get_job_requested_times()?;
        let requested_time = requested_times.get(&activity.place.idx)?;

        Some(self.penalty.calculate_penalty(arrival, *requested_time))
    }
}
