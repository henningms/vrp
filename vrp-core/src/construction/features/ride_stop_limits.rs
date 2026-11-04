//! Provides a constraint which limits how many stops a vehicle makes during a job's ride.
//!
//! For example, a passenger who must board last is a job whose ride allows zero pickup stops.
//!
//! # Semantics
//! - A job's ride runs from its first pickup activity to its last delivery activity in the tour.
//!   A job with only its pickups or only its deliveries in the tour is not judged yet.
//! - Consecutive pickup and delivery activities at the same location form one stop. Activities
//!   without demand (breaks, reloads) and the depot start and end never form or split a stop.
//! - The stop holding the job's first pickup and the stop holding its last delivery are excluded,
//!   even when other jobs act there too. Only the stops strictly between them are counted:
//!   - `pickups`: stops where at least one other job is picked up;
//!   - `deliveries`: stops where at least one other job is delivered;
//!   - `stops`: stops where any other job is picked up or delivered.
//! - An insertion is refused when any limit that is set is exceeded.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/ride_stop_limits_test.rs"]
mod ride_stop_limits_test;

use super::*;
use crate::models::common::{ConfigurableLoad, Location, MultiDimLoad, SingleDimLoad};
use crate::models::problem::Single;
use crate::models::solution::Activity;
use std::iter::once;

custom_dimension!(pub JobRideStopLimits typeof RideStopLimits);

/// Limits on the stops a vehicle makes while a job is on board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RideStopLimits {
    /// Max number of stops where other jobs are picked up.
    pub pickups: Option<usize>,
    /// Max number of stops where other jobs are delivered.
    pub deliveries: Option<usize>,
    /// Max number of stops where other jobs are picked up or delivered.
    pub stops: Option<usize>,
}

/// Creates a ride stop limits feature as a hard constraint.
pub fn create_ride_stop_limits_feature(name: &str, code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default().with_name(name).with_constraint(RideStopLimitsConstraint { code }).build()
}

struct RideStopLimitsConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for RideStopLimitsConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else {
            return None;
        };

        let tour = &route_ctx.route().tour;
        let target_has_limits = activity_ctx.target.retrieve_job().is_some_and(|job| has_limits(&job));
        if !target_has_limits && !tour.jobs().any(has_limits) {
            return None;
        }

        // An activity which is neither a pickup nor a delivery cannot change any stop.
        let target = StopActivity::new(activity_ctx.target)?;

        // `activity_ctx.index` is the index of `prev`: the target goes right after it.
        let before = (0..=activity_ctx.index).filter_map(|idx| tour.get(idx)).filter_map(StopActivity::new);
        let after =
            (activity_ctx.index + 1..tour.total()).filter_map(|idx| tour.get(idx)).filter_map(StopActivity::new);
        let mut sequence = before.collect::<Vec<_>>();
        let target_idx = sequence.len();
        sequence.extend(once(target).chain(after));

        get_rides(&sequence)
            .into_iter()
            .filter(|ride| ride.first_pickup <= target_idx && target_idx <= ride.last_delivery)
            .any(|ride| ride.exceeds_limits(&sequence))
            .then_some(ConstraintViolation { code: self.code, stopped: false })
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, ViolationCode> {
        if has_limits(&source) || has_limits(&candidate) { Err(self.code) } else { Ok(source) }
    }
}

fn has_limits(job: &Job) -> bool {
    job.dimens().get_job_ride_stop_limits().is_some()
}

/// A pickup or delivery activity of a job.
struct StopActivity {
    job: Job,
    location: Location,
    is_pickup: bool,
    is_delivery: bool,
}

impl StopActivity {
    fn new(activity: &Activity) -> Option<Self> {
        let single = activity.job.as_ref()?;
        let is_pickup = is_pickup(single);
        let is_delivery = is_delivery(single);
        if !is_pickup && !is_delivery {
            return None;
        }

        Some(Self { job: activity.retrieve_job()?, location: activity.place.location, is_pickup, is_delivery })
    }
}

/// A limited job's ride as indices into a sequence of stop activities.
struct Ride<'a> {
    job: &'a Job,
    limits: &'a RideStopLimits,
    first_pickup: usize,
    last_delivery: usize,
}

impl Ride<'_> {
    fn exceeds_limits(&self, sequence: &[StopActivity]) -> bool {
        let (first, last) = (self.first_pickup, self.last_delivery);

        let pickup_location = sequence[first].location;
        let pickup_stop_end = (first..last).find(|&idx| sequence[idx + 1].location != pickup_location).unwrap_or(last);

        let delivery_location = sequence[last].location;
        let delivery_stop_start = (pickup_stop_end + 1..=last)
            .rev()
            .find(|&idx| sequence[idx - 1].location != delivery_location)
            .unwrap_or(last);

        let between = if pickup_stop_end < delivery_stop_start {
            &sequence[pickup_stop_end + 1..delivery_stop_start]
        } else {
            &[]
        };

        let (mut pickups, mut deliveries, mut stops) = (0, 0, 0);
        between.chunk_by(|left, right| left.location == right.location).for_each(|stop| {
            let has_pickup = stop.iter().any(|activity| activity.is_pickup && activity.job != *self.job);
            let has_delivery = stop.iter().any(|activity| activity.is_delivery && activity.job != *self.job);

            pickups += usize::from(has_pickup);
            deliveries += usize::from(has_delivery);
            stops += usize::from(has_pickup || has_delivery);
        });

        let exceeds = |count: usize, limit: Option<usize>| limit.is_some_and(|limit| count > limit);

        exceeds(pickups, self.limits.pickups)
            || exceeds(deliveries, self.limits.deliveries)
            || exceeds(stops, self.limits.stops)
    }
}

fn get_rides(sequence: &[StopActivity]) -> Vec<Ride<'_>> {
    let mut rides: Vec<(&Job, &RideStopLimits, Option<usize>, Option<usize>)> = Vec::new();

    sequence.iter().enumerate().for_each(|(idx, activity)| {
        let Some(limits) = activity.job.dimens().get_job_ride_stop_limits() else {
            return;
        };

        let ride_idx = rides.iter().position(|(job, ..)| *job == &activity.job).unwrap_or_else(|| {
            rides.push((&activity.job, limits, None, None));
            rides.len() - 1
        });
        let (_, _, first_pickup, last_delivery) = &mut rides[ride_idx];

        if activity.is_pickup && first_pickup.is_none() {
            *first_pickup = Some(idx);
        }
        if activity.is_delivery {
            *last_delivery = Some(idx);
        }
    });

    rides
        .into_iter()
        .filter_map(|(job, limits, first_pickup, last_delivery)| {
            let (first_pickup, last_delivery) = first_pickup.zip(last_delivery)?;
            (first_pickup < last_delivery).then_some(Ride { job, limits, first_pickup, last_delivery })
        })
        .collect()
}

fn is_pickup(single: &Single) -> bool {
    if let Some(demand) = single.dimens.get_job_demand::<ConfigurableLoad>() {
        return has_load(&demand.pickup.0.load, demand.pickup.0.size)
            || has_load(&demand.pickup.1.load, demand.pickup.1.size);
    }
    if let Some(demand) = single.dimens.get_job_demand::<MultiDimLoad>() {
        return has_load(&demand.pickup.0.load, demand.pickup.0.size)
            || has_load(&demand.pickup.1.load, demand.pickup.1.size);
    }
    single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.pickup.0.value != 0 || d.pickup.1.value != 0)
}

fn is_delivery(single: &Single) -> bool {
    if let Some(demand) = single.dimens.get_job_demand::<ConfigurableLoad>() {
        return has_load(&demand.delivery.0.load, demand.delivery.0.size)
            || has_load(&demand.delivery.1.load, demand.delivery.1.size);
    }
    if let Some(demand) = single.dimens.get_job_demand::<MultiDimLoad>() {
        return has_load(&demand.delivery.0.load, demand.delivery.0.size)
            || has_load(&demand.delivery.1.load, demand.delivery.1.size);
    }
    single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.delivery.0.value != 0 || d.delivery.1.value != 0)
}

fn has_load(load: &[i32], size: usize) -> bool {
    size > 0 && load.iter().take(size).any(|value| *value != 0)
}
