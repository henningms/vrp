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
//! - Jobs are marked with a `LifoTag` (category like "wheelchair", "stroller") and a `LifoGroup` (unique ID per pickup-delivery pair)
//! - Vehicles specify which tags require LIFO ordering via `VehicleLifoTags`
//! - Different tags maintain separate stacks (wheelchairs and strollers don't interfere)
//! - Jobs without LIFO tags can be interleaved without constraint
//!
//! # Example
//! With tags ["wheelchair"]:
//! - Valid: [Pickup W1, Pickup S1, Deliver W1, Deliver S1] (stroller S1 is unconstrained)
//! - Valid: [Pickup W1, Pickup W2, Deliver W2, Deliver W1] (LIFO within wheelchair stack)
//! - Invalid: [Pickup W1, Pickup W2, Deliver W1, Deliver W2] (W2 picked up last but delivered last)
//!
//! # Algorithm
//! The constraint validates tours by simulating separate stacks per tag:
//! - When encountering a pickup with tag T and group G, push G onto the stack for tag T
//! - When encountering a delivery with tag T and group G, verify it matches the top of stack T, then pop
//! - If delivery doesn't match stack top for its tag, the tour violates LIFO ordering

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/lifo_ordering_test.rs"]
mod lifo_ordering_test;

use super::*;
use crate::models::common::SingleDimLoad;
use crate::models::problem::Single;
use crate::models::solution::Activity;
use rustc_hash::{FxHashMap, FxHashSet};

custom_dimension!(pub LifoGroup typeof LifoGroupId);
custom_dimension!(pub LifoTag typeof String);
custom_dimension!(pub VehicleLifoTags typeof FxHashSet<String>);

/// Represents a unique identifier for a pickup-delivery pair that requires LIFO ordering.
/// Each pickup-delivery pair that must follow LIFO semantics gets a unique ID.
///
/// # Example
/// ```ignore
/// // Wheelchair passenger 1
/// pickup.dimens.set_lifo_tag("wheelchair".to_string());
/// pickup.dimens.set_lifo_group(LifoGroupId(1));
/// delivery.dimens.set_lifo_tag("wheelchair".to_string());
/// delivery.dimens.set_lifo_group(LifoGroupId(1));
///
/// // Stroller 1
/// pickup.dimens.set_lifo_tag("stroller".to_string());
/// pickup.dimens.set_lifo_group(LifoGroupId(2));
/// delivery.dimens.set_lifo_tag("stroller".to_string());
/// delivery.dimens.set_lifo_group(LifoGroupId(2));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LifoGroupId(pub usize);

/// Creates a LIFO ordering feature as a hard constraint.
///
/// This feature enforces LIFO ordering for jobs marked with LIFO tags on vehicles
/// that have those tags in their `VehicleLifoTags` set.
///
/// # Example
/// ```ignore
/// let lifo_feature = create_lifo_ordering_feature(ViolationCode(16))?;
///
/// // Vehicle setup
/// let mut tags = FxHashSet::default();
/// tags.insert("wheelchair".to_string());
/// vehicle.dimens.set_vehicle_lifo_tags(tags);
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
                // Get the vehicle's LIFO tags (if any)
                let vehicle_lifo_tags = route_ctx.route().actor.vehicle.dimens.get_vehicle_lifo_tags();

                // If vehicle has no LIFO tags, no constraint applies
                let vehicle_lifo_tags = vehicle_lifo_tags?;
                if vehicle_lifo_tags.is_empty() {
                    return None;
                }

                // Simulate the tour with the new activity inserted
                let would_violate = self.check_lifo_violation(route_ctx, activity_ctx, vehicle_lifo_tags);

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
        // Don't allow merging jobs with LIFO tags
        // This is conservative but safe - we'd need to verify LIFO compatibility
        if source.dimens().get_lifo_tag().is_some() {
            Err(self.code)
        } else {
            Ok(source)
        }
    }
}

impl LifoOrderingConstraint {
    /// Checks if inserting the target activity would violate LIFO ordering.
    ///
    /// Simulates traversing the tour with the new activity inserted, maintaining separate
    /// stacks per LIFO tag. Verifies that deliveries match the stack top for their tag.
    fn check_lifo_violation(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
        vehicle_lifo_tags: &FxHashSet<String>,
    ) -> bool {
        let tour = &route_ctx.route().tour;
        // Separate stack per tag
        let mut stacks: FxHashMap<String, Vec<LifoGroupId>> = FxHashMap::default();

        // Process activities up to insertion point
        for idx in 0..activity_ctx.index {
            if let Some(activity) = tour.get(idx)
                && self.process_activity(activity, &mut stacks, vehicle_lifo_tags).is_err()
            {
                return true; // Violation in existing tour (shouldn't happen)
            }
        }

        // Process the new activity being inserted
        if self.process_activity(activity_ctx.target, &mut stacks, vehicle_lifo_tags).is_err() {
            return true; // Insertion would violate LIFO
        }

        // Process remaining activities
        for idx in activity_ctx.index..tour.total() {
            if let Some(activity) = tour.get(idx)
                && self.process_activity(activity, &mut stacks, vehicle_lifo_tags).is_err()
            {
                return true; // Insertion causes downstream violation
            }
        }

        false // No LIFO violation
    }

    /// Processes a single activity, updating the appropriate LIFO stack.
    ///
    /// Returns Err if the activity violates LIFO ordering (delivery doesn't match stack top for its tag).
    fn process_activity(
        &self,
        activity: &Activity,
        stacks: &mut FxHashMap<String, Vec<LifoGroupId>>,
        vehicle_lifo_tags: &FxHashSet<String>,
    ) -> Result<(), ()> {
        let Some(single) = activity.job.as_ref().map(|j| j.as_ref()) else {
            return Ok(());
        };

        let Some(lifo_tag) = single.dimens.get_lifo_tag() else {
            return Ok(()); // No LIFO tag, unconstrained
        };

        // Only enforce LIFO for tags the vehicle cares about
        if !vehicle_lifo_tags.contains(lifo_tag) {
            return Ok(());
        }

        let Some(lifo_group_id) = single.dimens.get_lifo_group().copied() else {
            return Ok(()); // Has tag but no group ID, skip
        };

        // Get or create the stack for this tag
        let stack = stacks.entry(lifo_tag.clone()).or_default();

        if self.is_pickup(single) {
            // Pickup: push group ID onto this tag's stack
            stack.push(lifo_group_id);
        } else if self.is_delivery(single) {
            // Delivery: must match top of this tag's stack (LIFO)
            if stack.last() == Some(&lifo_group_id) {
                stack.pop();
            } else {
                // Violation: delivery doesn't match stack top (not LIFO)
                return Err(());
            }
        }

        Ok(())
    }

    /// Checks if a job activity is a pickup.
    /// For PUDO (pickup-delivery) jobs, the dynamic pickup demand is stored in pickup.1
    fn is_pickup(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.pickup.1.is_not_empty())
    }

    /// Checks if a job activity is a delivery.
    /// For PUDO (pickup-delivery) jobs, the dynamic delivery demand is stored in delivery.1
    fn is_delivery(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.delivery.1.is_not_empty())
    }
}
