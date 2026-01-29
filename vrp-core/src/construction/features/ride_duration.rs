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
use crate::models::common::{Duration, SingleDimLoad, Timestamp};
use crate::models::problem::{Multi, Single, TransportCost, TravelTime};
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
        .build()
}

struct MaxRideDurationConstraint {
    code: ViolationCode,
    transport: Arc<dyn TransportCost>,
}

impl FeatureConstraint for MaxRideDurationConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                self.check_ride_duration(route_ctx, activity_ctx)
            }
            MoveContext::Route { .. } => None,
        }
    }

    fn merge(&self, source: Job, _candidate: Job) -> Result<Job, ViolationCode> {
        // Don't allow merging jobs with max ride duration
        if source.dimens().get_job_max_ride_duration().is_some() {
            Err(self.code)
        } else {
            Ok(source)
        }
    }
}

impl MaxRideDurationConstraint {
    /// Checks if inserting the target activity would violate max ride duration constraint.
    fn check_ride_duration(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ConstraintViolation> {
        let target = &activity_ctx.target;

        // Get the job associated with this activity
        let single = target.job.as_ref()?;

        // Try to get max ride duration from the Multi parent job
        let max_ride_duration = self.get_max_ride_duration_for_single(single)?;

        // Check if this is a pickup or delivery
        if self.is_pickup(single) {
            // For pickup insertion, check if existing deliveries for this job would violate the constraint
            self.check_pickup_insertion(route_ctx, activity_ctx, single, max_ride_duration)
        } else if self.is_delivery(single) {
            // For delivery insertion, check if the ride duration from pickup would be exceeded
            self.check_delivery_insertion(route_ctx, activity_ctx, single, max_ride_duration)
        } else {
            None
        }
    }

    /// Gets the max ride duration for a Single that belongs to a Multi job.
    fn get_max_ride_duration_for_single(&self, single: &Single) -> Option<Duration> {
        // First check if the Single itself has the max ride duration
        if let Some(duration) = single.dimens.get_job_max_ride_duration() {
            return Some(*duration);
        }

        // Then check the Multi parent via the root
        if let Some(multi) = Multi::roots(single) {
            return multi.dimens.get_job_max_ride_duration().copied();
        }

        None
    }

    /// Checks if inserting a pickup would cause downstream deliveries to violate the constraint.
    fn check_pickup_insertion(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
        pickup_single: &Single,
        max_ride_duration: Duration,
    ) -> Option<ConstraintViolation> {
        let route = route_ctx.route();
        let tour = &route.tour;

        // Calculate when we would depart from this pickup
        let pickup_departure = self.estimate_departure_time(route_ctx, activity_ctx);

        // Look for the corresponding delivery in the tour (after insertion point)
        for idx in activity_ctx.index..tour.total() {
            if let Some(activity) = tour.get(idx)
                && let Some(delivery_single) = activity.job.as_ref()
                && self.is_same_job(pickup_single, delivery_single)
                && self.is_delivery(delivery_single)
            {
                // Found the delivery - recalculate its arrival time considering the insertion
                let delivery_arrival =
                    self.estimate_arrival_at_activity_after_insertion(route_ctx, activity_ctx, idx);

                let ride_duration = delivery_arrival - pickup_departure;
                if ride_duration > max_ride_duration {
                    return Some(ConstraintViolation { code: self.code, stopped: false });
                }
            }
        }

        None
    }

    /// Checks if inserting a delivery would exceed the max ride duration from its pickup.
    fn check_delivery_insertion(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
        delivery_single: &Single,
        max_ride_duration: Duration,
    ) -> Option<ConstraintViolation> {
        let route = route_ctx.route();
        let tour = &route.tour;

        // Look for the corresponding pickup earlier in the tour
        // Note: activity_ctx.index is the leg index, which corresponds to the index of activity_ctx.prev.
        // The delivery will be inserted AFTER prev, so we need to check indices 0..=activity_ctx.index
        // to include prev (which might be the pickup).
        for idx in 0..=activity_ctx.index {
            if let Some(activity) = tour.get(idx)
                && let Some(pickup_single) = activity.job.as_ref()
                && self.is_same_job(delivery_single, pickup_single)
                && self.is_pickup(pickup_single)
            {
                // Found the pickup - get its departure time
                let pickup_departure = activity.schedule.departure;

                // Calculate when we would arrive at the delivery
                let delivery_arrival = self.estimate_arrival_time(route_ctx, activity_ctx);

                let ride_duration = delivery_arrival - pickup_departure;
                if ride_duration > max_ride_duration {
                    return Some(ConstraintViolation { code: self.code, stopped: false });
                }

                // Found and checked the pickup, no need to continue
                return None;
            }
        }

        None
    }

    /// Estimates the arrival time at the target activity.
    fn estimate_arrival_time(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Timestamp {
        let prev = activity_ctx.prev;
        let target = &activity_ctx.target;

        let travel_duration = self.transport.duration(
            route_ctx.route(),
            prev.place.location,
            target.place.location,
            TravelTime::Departure(prev.schedule.departure),
        );

        prev.schedule.departure + travel_duration
    }

    /// Estimates the departure time from the target activity after insertion.
    fn estimate_departure_time(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Timestamp {
        let arrival = self.estimate_arrival_time(route_ctx, activity_ctx);
        let target = &activity_ctx.target;

        // Departure = max(arrival, time_window_start) + service_duration
        arrival.max(target.place.time.start) + target.place.duration
    }

    /// Estimates the arrival time at an activity that comes after the insertion point.
    fn estimate_arrival_at_activity_after_insertion(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
        target_idx: usize,
    ) -> Timestamp {
        let route = route_ctx.route();
        let tour = &route.tour;

        // Start from the inserted activity's departure
        let mut current_departure = self.estimate_departure_time(route_ctx, activity_ctx);
        let mut current_location = activity_ctx.target.place.location;

        // Walk through activities from insertion point to target
        for idx in activity_ctx.index..=target_idx {
            if let Some(activity) = tour.get(idx) {
                let travel_duration = self.transport.duration(
                    route,
                    current_location,
                    activity.place.location,
                    TravelTime::Departure(current_departure),
                );

                let arrival = current_departure + travel_duration;

                if idx == target_idx {
                    return arrival;
                }

                // Update for next iteration
                current_departure = arrival.max(activity.place.time.start) + activity.place.duration;
                current_location = activity.place.location;
            }
        }

        // Should not reach here
        current_departure
    }

    /// Checks if a job activity is a pickup.
    fn is_pickup(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.pickup.1.is_not_empty())
    }

    /// Checks if a job activity is a delivery.
    fn is_delivery(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.delivery.1.is_not_empty())
    }

    /// Checks if two Singles belong to the same Multi job.
    fn is_same_job(&self, single1: &Single, single2: &Single) -> bool {
        match (Multi::roots(single1), Multi::roots(single2)) {
            (Some(multi1), Some(multi2)) => Arc::ptr_eq(&multi1, &multi2),
            _ => false,
        }
    }
}
