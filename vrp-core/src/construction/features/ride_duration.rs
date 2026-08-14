//! Provides a constraint to limit the ride duration for pickup-delivery jobs.
//!
//! This feature ensures that the time between a pickup departure and delivery arrival
//! does not exceed a specified maximum duration. This is commonly used for:
//! - Passenger transport services with service level agreements
//! - Perishable goods delivery
//! - Time-sensitive medical transport
//!
//! # How It Works
//! - Jobs with `maxRideDuration` set will have the max duration stored in the Multi job dimensions
//! - When evaluating insertions, the constraint checks if the delivery would occur within
//!   the allowed time from when the corresponding pickup departs
//! - This is a hard constraint - violations result in the insertion being rejected

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/ride_duration_test.rs"]
mod ride_duration_test;

use super::*;
use crate::models::common::{ConfigurableLoad, Duration, MultiDimLoad, SingleDimLoad, Timestamp};
use crate::models::problem::{Multi, Single, TransportCost, TravelTime};
use crate::models::solution::Activity;
use std::collections::HashMap;
use std::sync::Arc;

custom_dimension!(pub JobMaxRideDuration typeof Duration);

/// Creates a max ride duration feature as a hard constraint.
///
/// This feature enforces that the time between pickup departure and delivery arrival
/// does not exceed the job's `maxRideDuration` value.
pub fn create_max_ride_duration_feature(
    name: &str,
    code: ViolationCode,
    transport: Arc<dyn TransportCost>,
) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name(name)
        .with_constraint(MaxRideDurationConstraint { code, transport })
        .with_state(MaxRideDurationState {})
        .build()
}

struct MaxRideDurationConstraint {
    code: ViolationCode,
    transport: Arc<dyn TransportCost>,
}

struct MaxRideDurationState {}

impl FeatureState for MaxRideDurationState {
    fn accept_insertion(&self, _: &mut SolutionContext, _: usize, _: &Job) {}

    fn accept_route_state(&self, _: &mut RouteContext) {}

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        // Removing another job can make a pickup happen earlier while its delivery
        // remains pinned by a later time window. Such a removal is not evaluated as
        // an insertion move, so remove any newly invalid ride and let the normal
        // unassigned/recreate flow try it again in a subsequent search iteration.
        let mut invalid_jobs = Vec::new();
        for route_ctx in &mut solution_ctx.routes {
            let jobs = get_violating_jobs(route_ctx);
            for job in jobs {
                if route_ctx.route_mut().tour.remove(&job) {
                    route_ctx.mark_stale(true);
                    invalid_jobs.push(job);
                }
            }
        }

        solution_ctx.unassigned.extend(invalid_jobs.into_iter().map(|job| (job, UnassignmentInfo::Unknown)));
    }
}

impl FeatureConstraint for MaxRideDurationConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Activity { route_ctx, activity_ctx, .. } => self.check_ride_duration(route_ctx, activity_ctx),
            MoveContext::Route { .. } => None,
        }
    }

    fn merge(&self, source: Job, _candidate: Job) -> Result<Job, ViolationCode> {
        // Don't allow merging jobs with max ride duration
        if source.dimens().get_job_max_ride_duration().is_some() { Err(self.code) } else { Ok(source) }
    }
}

impl MaxRideDurationConstraint {
    /// Checks if inserting the target activity would violate max ride duration constraint.
    fn check_ride_duration(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ConstraintViolation> {
        let route = route_ctx.route();
        let tour = &route.tour;
        let mut intervals = HashMap::<usize, RideInterval>::new();

        // Activities through `index` precede the insertion and keep their current schedule.
        for idx in 0..=activity_ctx.index {
            if let Some(activity) = tour.get(idx) {
                self.record_interval(activity, activity.schedule.arrival, activity.schedule.departure, &mut intervals);
            }
        }

        // Project the inserted activity and every downstream activity using the same
        // earliest-arrival scheduling rule as the route schedule updater. This is
        // necessary even when the inserted activity belongs to another job: a stop
        // inserted between an existing pickup and delivery can lengthen that ride.
        let mut location = activity_ctx.prev.place.location;
        let mut departure = activity_ctx.prev.schedule.departure;

        let mut project = |activity: &Activity| {
            let arrival = departure
                + self.transport.duration(route, location, activity.place.location, TravelTime::Departure(departure));
            departure = arrival.max(activity.place.time.start) + activity.place.duration;
            location = activity.place.location;
            self.record_interval(activity, arrival, departure, &mut intervals);
        };

        project(activity_ctx.target);
        for idx in activity_ctx.index + 1..tour.total() {
            if let Some(activity) = tour.get(idx) {
                project(activity);
            }
        }

        intervals.values().find_map(|interval| {
            interval.pickup_departure.zip(interval.delivery_service_start).and_then(|(pickup, delivery)| {
                (delivery - pickup > interval.limit).then_some(ConstraintViolation { code: self.code, stopped: false })
            })
        })
    }

    fn record_interval(
        &self,
        activity: &Activity,
        arrival: Timestamp,
        departure: Timestamp,
        intervals: &mut HashMap<usize, RideInterval>,
    ) {
        record_interval(activity, arrival, departure, intervals);
    }
}

struct RideInterval {
    job: Arc<Multi>,
    limit: Duration,
    pickup_departure: Option<Timestamp>,
    delivery_service_start: Option<Timestamp>,
}

fn get_violating_jobs(route_ctx: &RouteContext) -> Vec<Job> {
    let mut intervals = HashMap::<usize, RideInterval>::new();
    route_ctx.route().tour.all_activities().for_each(|activity| {
        record_interval(activity, activity.schedule.arrival, activity.schedule.departure, &mut intervals)
    });

    intervals
        .into_values()
        .filter(|interval| {
            interval
                .pickup_departure
                .zip(interval.delivery_service_start)
                .is_some_and(|(pickup, delivery)| delivery - pickup > interval.limit)
        })
        .map(|interval| Job::Multi(interval.job))
        .collect()
}

fn record_interval(
    activity: &Activity,
    arrival: Timestamp,
    departure: Timestamp,
    intervals: &mut HashMap<usize, RideInterval>,
) {
    let Some(single) = activity.job.as_ref() else { return };
    let Some(multi) = Multi::roots(single) else { return };
    let Some(limit) = multi.dimens.get_job_max_ride_duration().copied() else { return };
    let key = Arc::as_ptr(&multi) as usize;
    let interval = intervals.entry(key).or_insert_with(|| RideInterval {
        job: multi,
        limit,
        pickup_departure: None,
        delivery_service_start: None,
    });

    if is_pickup(single) {
        interval.pickup_departure = Some(interval.pickup_departure.map_or(departure, |value| value.min(departure)));
    } else if is_delivery(single) {
        let service_start = arrival.max(activity.place.time.start);
        interval.delivery_service_start =
            Some(interval.delivery_service_start.map_or(service_start, |value| value.max(service_start)));
    }
}

fn is_pickup(single: &Single) -> bool {
    if let Some(demand) = single.dimens.get_job_demand::<ConfigurableLoad>() {
        return configurable_load_has_load(&demand.pickup.1);
    }
    if let Some(demand) = single.dimens.get_job_demand::<MultiDimLoad>() {
        return multi_dim_has_load(&demand.pickup.1);
    }
    single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.pickup.1.is_not_empty())
}

fn is_delivery(single: &Single) -> bool {
    if let Some(demand) = single.dimens.get_job_demand::<ConfigurableLoad>() {
        return configurable_load_has_load(&demand.delivery.1);
    }
    if let Some(demand) = single.dimens.get_job_demand::<MultiDimLoad>() {
        return multi_dim_has_load(&demand.delivery.1);
    }
    single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.delivery.1.is_not_empty())
}

fn multi_dim_has_load(load: &MultiDimLoad) -> bool {
    load.size > 0 && load.load[..load.size].iter().any(|value| *value != 0)
}

fn configurable_load_has_load(load: &ConfigurableLoad) -> bool {
    load.size > 0 && load.load[..load.size].iter().any(|value| *value != 0)
}
