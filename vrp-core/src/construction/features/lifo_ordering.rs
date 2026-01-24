//! Provides LIFO (Last-In-First-Out) ordering constraint for pickup-delivery problems.
//!
//! This feature ensures that for vehicles requiring LIFO ordering (e.g., vehicles with limited
//! maneuvering space like wheelchair-accessible minibuses, narrow cargo holds, or loading docks),
//! items picked up last are delivered first, maintaining a stack-like ordering.
//!
//! # Use Cases
//! - Wheelchair-accessible minibuses with limited space
//! - Loading docks with depth constraints (pallets)
//! - Narrow warehouse aisles (forklifts can't maneuver around items)
//! - Aircraft cargo holds with single access point
//! - Moving trucks with narrow interiors
//!
//! # Semantics
//! - Vehicles are marked with `lifoRequired` flag
//! - Jobs requiring LIFO are marked with a `LifoGroup` dimension (unique ID per pickup-delivery pair)
//! - On LIFO-required vehicles, jobs with LIFO groups must follow LIFO ordering
//! - Jobs without LIFO groups can be interleaved without constraint
//!
//! # Example
//! Valid tour: [Pickup L1, Pickup L2, Pickup Regular, Deliver L2, Deliver L1]
//! Invalid tour: [Pickup L1, Pickup L2, Deliver L1, Deliver L2] ← L2 picked up last but delivered last
//!
//! # Algorithm
//! The constraint validates tours by simulating a stack:
//! - When encountering a pickup with LIFO group, push the group ID onto a stack
//! - When encountering a delivery with LIFO group, verify it matches the top of the stack (LIFO), then pop
//! - If delivery doesn't match stack top, the tour violates LIFO ordering

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/lifo_ordering_test.rs"]
mod lifo_ordering_test;

use super::*;
use crate::models::common::SingleDimLoad;
use crate::models::problem::Single;
use crate::models::solution::Activity;

custom_dimension!(pub LifoGroup typeof LifoGroupId);
custom_dimension!(pub VehicleLifoRequired typeof bool);

/// Represents a unique identifier for a pickup-delivery pair that requires LIFO ordering.
/// Each pickup-delivery pair that must follow LIFO semantics gets a unique ID.
///
/// # Example
/// ```ignore
/// // Wheelchair passenger 1
/// pickup.dimens.set_lifo_group(LifoGroupId(1));
/// delivery.dimens.set_lifo_group(LifoGroupId(1));
///
/// // Wheelchair passenger 2
/// pickup.dimens.set_lifo_group(LifoGroupId(2));
/// delivery.dimens.set_lifo_group(LifoGroupId(2));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LifoGroupId(pub usize);

/// Creates a LIFO ordering feature as a hard constraint.
///
/// This feature enforces LIFO ordering for jobs marked with LIFO groups on vehicles
/// marked with `lifoRequired`.
///
/// # Example
/// ```ignore
/// let lifo_feature = create_lifo_ordering_feature(ViolationCode(16))?;
/// ```
pub fn create_lifo_ordering_feature(code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name("lifo_ordering")
        .with_constraint(LifoOrderingConstraint { code })
        .build()
}

struct LifoOrderingConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for LifoOrderingConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                // Only check on vehicles that require LIFO
                let vehicle_requires_lifo =
                    route_ctx.route().actor.vehicle.dimens.get_vehicle_lifo_required().copied().unwrap_or(false);

                if !vehicle_requires_lifo {
                    return None;
                }

                // Simulate the tour with the new activity inserted
                let would_violate = self.check_lifo_violation(route_ctx, activity_ctx);

                if would_violate {
                    Some(ConstraintViolation { code: self.code, stopped: false })
                } else {
                    None
                }
            }
            MoveContext::Route { .. } => None,
        }
    }

    fn merge(&self, source: Job, _candidate: Job) -> Result<Job, ViolationCode> {
        // Don't allow merging jobs with LIFO groups
        // This is conservative but safe - we'd need to verify LIFO compatibility
        if source.dimens().get_lifo_group().is_some() {
            Err(self.code)
        } else {
            Ok(source)
        }
    }
}

impl LifoOrderingConstraint {
    /// Checks if inserting the target activity would violate LIFO ordering.
    ///
    /// Simulates traversing the tour with the new activity inserted, maintaining a stack
    /// of LIFO group IDs currently "on board". Verifies that deliveries match the stack top.
    fn check_lifo_violation(&self, route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> bool {
        let tour = &route_ctx.route().tour;
        let mut stack: Vec<LifoGroupId> = Vec::new();

        // Process activities up to insertion point
        for idx in 0..activity_ctx.index {
            if let Some(activity) = tour.get(idx) {
                if self.process_activity(activity, &mut stack).is_err() {
                    return true; // Violation in existing tour (shouldn't happen)
                }
            }
        }

        // Process the new activity being inserted
        if self.process_activity(&activity_ctx.target, &mut stack).is_err() {
            return true; // Insertion would violate LIFO
        }

        // Process remaining activities
        for idx in activity_ctx.index..tour.total() {
            if let Some(activity) = tour.get(idx) {
                if self.process_activity(activity, &mut stack).is_err() {
                    return true; // Insertion causes downstream violation
                }
            }
        }

        false // No LIFO violation
    }

    /// Processes a single activity, updating the LIFO stack.
    ///
    /// Returns Err if the activity violates LIFO ordering (delivery doesn't match stack top).
    fn process_activity(&self, activity: &Activity, stack: &mut Vec<LifoGroupId>) -> Result<(), ()> {
        if let Some(single) = activity.job.as_ref().map(|j| j.as_ref()) {
            if let Some(lifo_group_id) = single.dimens.get_lifo_group().copied() {
                // This activity is part of a LIFO group
                if self.is_pickup(single) {
                    // Pickup: push group ID onto stack
                    stack.push(lifo_group_id);
                } else if self.is_delivery(single) {
                    // Delivery: must match top of stack (LIFO)
                    if stack.last() == Some(&lifo_group_id) {
                        stack.pop();
                    } else {
                        // Violation: delivery doesn't match stack top (not LIFO)
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks if a job activity is a pickup.
    /// For PUDO (pickup-delivery) jobs, the dynamic pickup demand is stored in pickup.1
    fn is_pickup(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().map_or(false, |d| d.pickup.1.is_not_empty())
    }

    /// Checks if a job activity is a delivery.
    /// For PUDO (pickup-delivery) jobs, the dynamic delivery demand is stored in delivery.1
    fn is_delivery(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().map_or(false, |d| d.delivery.1.is_not_empty())
    }
}
