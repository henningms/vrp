use super::*;

use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::solution::Activity;
use rustc_hash::FxHashSet;

const LIFO_VIOLATION_CODE: ViolationCode = ViolationCode(1100);

fn make_lifo_tags(tags: &[&str]) -> FxHashSet<String> {
    tags.iter().map(|s| s.to_string()).collect()
}

/// Creates an activity representing a pickup with LIFO tag and group
fn create_lifo_pickup(location: usize, tag: &str, group_id: usize) -> Activity {
    let mut single_builder = TestSingleBuilder::default();
    single_builder.location(Some(location));
    single_builder.demand(Demand::pudo_pickup(1));
    single_builder.dimens_mut().set_lifo_tag(tag.to_string());
    single_builder.dimens_mut().set_lifo_group(LifoGroupId(group_id));
    let single = single_builder.build_shared();

    ActivityBuilder::with_location(location).job(Some(single)).build()
}

/// Creates an activity representing a delivery with LIFO tag and group
fn create_lifo_delivery(location: usize, tag: &str, group_id: usize) -> Activity {
    let mut single_builder = TestSingleBuilder::default();
    single_builder.location(Some(location));
    single_builder.demand(Demand::pudo_delivery(1));
    single_builder.dimens_mut().set_lifo_tag(tag.to_string());
    single_builder.dimens_mut().set_lifo_group(LifoGroupId(group_id));
    let single = single_builder.build_shared();

    ActivityBuilder::with_location(location).job(Some(single)).build()
}

/// Creates a regular activity without LIFO constraints
fn create_regular_activity(location: usize) -> Activity {
    ActivityBuilder::with_location(location)
        .job(Some(TestSingleBuilder::default().location(Some(location)).build_shared()))
        .build()
}

/// Creates a vehicle with the given LIFO tags
fn create_lifo_vehicle(id: &str, tags: &[&str]) -> crate::models::problem::Vehicle {
    let mut builder = TestVehicleBuilder::default();
    builder.id(id);
    builder.dimens_mut().set_vehicle_lifo_tags(make_lifo_tags(tags));
    builder.build()
}

// =============================================================================
// Unit Tests
// =============================================================================

#[test]
fn test_lifo_group_id_equality() {
    let id1 = LifoGroupId(42);
    let id2 = LifoGroupId(42);
    let id3 = LifoGroupId(43);

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_lifo_group_dimension() {
    let mut builder = TestSingleBuilder::default();
    builder.dimens_mut().set_lifo_group(LifoGroupId(1));
    let single = builder.build();

    assert_eq!(single.dimens.get_lifo_group(), Some(&LifoGroupId(1)));
}

#[test]
fn test_lifo_tag_dimension() {
    let mut builder = TestSingleBuilder::default();
    builder.dimens_mut().set_lifo_tag("wheelchair".to_string());
    let single = builder.build();

    assert_eq!(single.dimens.get_lifo_tag(), Some(&"wheelchair".to_string()));
}

#[test]
fn test_vehicle_lifo_tags_dimension() {
    let mut builder = TestVehicleBuilder::default();
    builder.dimens_mut().set_vehicle_lifo_tags(make_lifo_tags(&["wheelchair", "stroller"]));
    let vehicle = builder.build();

    let tags = vehicle.dimens.get_vehicle_lifo_tags().unwrap();
    assert!(tags.contains("wheelchair"));
    assert!(tags.contains("stroller"));
    assert!(!tags.contains("other"));
}

#[test]
fn test_vehicle_empty_lifo_tags() {
    let mut builder = TestVehicleBuilder::default();
    builder.dimens_mut().set_vehicle_lifo_tags(FxHashSet::default());
    let vehicle = builder.build();

    let tags = vehicle.dimens.get_vehicle_lifo_tags().unwrap();
    assert!(tags.is_empty());
}

#[test]
fn test_feature_creation() {
    let feature = create_lifo_ordering_feature(LIFO_VIOLATION_CODE).unwrap();
    assert_eq!(feature.name, "lifo_ordering");
}

#[test]
fn test_is_pickup_detection() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build();

    assert!(constraint.is_pickup(&pickup));
    assert!(!constraint.is_delivery(&pickup));
}

#[test]
fn test_is_delivery_detection() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build();

    assert!(!constraint.is_pickup(&delivery));
    assert!(constraint.is_delivery(&delivery));
}

#[test]
fn test_regular_job_not_pickup_or_delivery() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    let regular = TestSingleBuilder::default().build();

    assert!(!constraint.is_pickup(&regular));
    assert!(!constraint.is_delivery(&regular));
}

// =============================================================================
// Integration Tests - LIFO Ordering Constraint Evaluation
// =============================================================================

/// Helper to evaluate constraint for activity insertion
/// insertion_idx is the position where the new activity will be inserted
/// prev_idx is the index of the activity before the insertion point
fn evaluate_insertion(
    route_ctx: &RouteContext,
    target: &Activity,
    insertion_idx: usize,
    prev_idx: usize,
    next_idx: Option<usize>,
) -> Option<ConstraintViolation> {
    let feature = create_lifo_ordering_feature(LIFO_VIOLATION_CODE).unwrap();
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;

    let prev = route_ctx.route().tour.get(prev_idx).unwrap();
    let next = next_idx.and_then(|idx| route_ctx.route().tour.get(idx));

    let activity_ctx = ActivityContext { index: insertion_idx, prev, target, next };

    feature.constraint.unwrap().evaluate(&MoveContext::Activity {
        solution_ctx: &solution_ctx,
        route_ctx,
        activity_ctx: &activity_ctx,
    })
}

#[test]
fn test_valid_lifo_tour_accepts_correct_delivery_order() {
    // Tour: [Start(0), Pickup W1(1), Pickup W2(2), Delivery W2(3)]
    // This is valid LIFO: W2 picked up last, delivered first
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1))   // idx 1: W1 pickup
                .add_activity(create_lifo_pickup(20, "wheelchair", 2))   // idx 2: W2 pickup
                .add_activity(create_lifo_delivery(30, "wheelchair", 2)) // idx 3: W2 delivery
                .build(),
        )
        .build();

    // Try inserting W1 delivery at position 4 (after W2 delivery at idx 3)
    // insertion_idx=4, prev_idx=3, next=None
    let w1_delivery = create_lifo_delivery(40, "wheelchair", 1);
    let result = evaluate_insertion(&route_ctx, &w1_delivery, 4, 3, None);

    assert!(result.is_none(), "Valid LIFO delivery should be accepted");
}

#[test]
fn test_invalid_lifo_tour_rejects_wrong_delivery_order() {
    // Tour: [Start(0), Pickup W1(1), Pickup W2(2)]
    // Trying to insert W1 delivery after W2 pickup violates LIFO (W2 should be delivered first)
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1)) // idx 1: W1 pickup
                .add_activity(create_lifo_pickup(20, "wheelchair", 2)) // idx 2: W2 pickup
                .build(),
        )
        .build();

    // Try inserting W1 delivery at position 3 (after W2 pickup at idx 2)
    // insertion_idx=3, prev_idx=2, next=None
    // Should fail because W2 was picked up last but we're delivering W1 first
    let w1_delivery = create_lifo_delivery(30, "wheelchair", 1);
    let result = evaluate_insertion(&route_ctx, &w1_delivery, 3, 2, None);

    assert!(result.is_some(), "Invalid LIFO delivery order should be rejected");
    assert_eq!(result.unwrap().code, LIFO_VIOLATION_CODE);
}

#[test]
fn test_separate_stacks_for_different_tags() {
    // Tour: [Start(0), Pickup W1(1), Pickup S1(2)]
    // W1=wheelchair, S1=stroller - separate stacks
    // Delivering W1 before S1 should be valid because they're independent stacks
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair", "stroller"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1)) // idx 1: W1 pickup
                .add_activity(create_lifo_pickup(20, "stroller", 2))   // idx 2: S1 pickup
                .build(),
        )
        .build();

    // Insert W1 delivery at position 3 (after S1 pickup)
    // insertion_idx=3, prev_idx=2, next=None
    // Should be valid because wheelchair stack only has W1
    let w1_delivery = create_lifo_delivery(30, "wheelchair", 1);
    let result = evaluate_insertion(&route_ctx, &w1_delivery, 3, 2, None);

    assert!(result.is_none(), "Delivery from independent stack should be accepted");
}

#[test]
fn test_vehicle_ignores_tags_not_in_its_lifo_tags() {
    // Vehicle only has wheelchair in lifoTags, stroller LIFO should be ignored
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"])) // Only wheelchair
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "stroller", 1)) // idx 1: S1 pickup
                .add_activity(create_lifo_pickup(20, "stroller", 2)) // idx 2: S2 pickup
                .build(),
        )
        .build();

    // Insert S1 delivery at position 3 (wrong LIFO order for strollers)
    // insertion_idx=3, prev_idx=2, next=None
    // Should be accepted because vehicle doesn't enforce LIFO for strollers
    let s1_delivery = create_lifo_delivery(30, "stroller", 1);
    let result = evaluate_insertion(&route_ctx, &s1_delivery, 3, 2, None);

    assert!(result.is_none(), "Stroller LIFO should be ignored when not in vehicle's lifoTags");
}

#[test]
fn test_vehicle_without_lifo_tags_ignores_all_lifo() {
    // Vehicle has no LIFO tags, all LIFO constraints should be ignored
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle({
            let mut builder = TestVehicleBuilder::default();
            builder.id("v1");
            // No LIFO tags set
            builder.build()
        })
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1)) // idx 1
                .add_activity(create_lifo_pickup(20, "wheelchair", 2)) // idx 2
                .build(),
        )
        .build();

    // Insert W1 delivery at position 3 (wrong LIFO order)
    // insertion_idx=3, prev_idx=2, next=None
    // Should be accepted because no LIFO enforcement
    let w1_delivery = create_lifo_delivery(30, "wheelchair", 1);
    let result = evaluate_insertion(&route_ctx, &w1_delivery, 3, 2, None);

    assert!(result.is_none(), "LIFO should be ignored when vehicle has no lifoTags");
}

#[test]
fn test_regular_jobs_interleave_without_affecting_lifo() {
    // Tour: [Start(0), Pickup W1(1), Regular(2), Pickup W2(3)]
    // Regular jobs should not affect LIFO stack
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1)) // idx 1: W1 pickup
                .add_activity(create_regular_activity(15))             // idx 2: Regular job
                .add_activity(create_lifo_pickup(20, "wheelchair", 2)) // idx 3: W2 pickup
                .build(),
        )
        .build();

    // Insert W2 delivery at position 4 (after W2 pickup)
    // insertion_idx=4, prev_idx=3, next=None
    // Should be valid because W2 is top of stack (LIFO)
    let w2_delivery = create_lifo_delivery(30, "wheelchair", 2);
    let result = evaluate_insertion(&route_ctx, &w2_delivery, 4, 3, None);

    assert!(result.is_none(), "LIFO delivery should work with interleaved regular jobs");
}

#[test]
fn test_inserting_pickup_that_would_cause_downstream_violation() {
    // Tour: [Start(0), Pickup W1(1), Delivery W1(2)]
    // Inserting W2 pickup between W1 pickup and W1 delivery would violate LIFO
    // because W2 would need to be delivered before W1, but W1 delivery is already placed
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1))   // idx 1: W1 pickup
                .add_activity(create_lifo_delivery(20, "wheelchair", 1)) // idx 2: W1 delivery
                .build(),
        )
        .build();

    // Try inserting W2 pickup at position 2 (between W1 pickup and W1 delivery)
    // insertion_idx=2, prev_idx=1, next_idx=2 (W1 delivery)
    // This would require W2 delivery to come before W1 delivery, but W1 delivery is already there
    let w2_pickup = create_lifo_pickup(15, "wheelchair", 2);
    let result = evaluate_insertion(&route_ctx, &w2_pickup, 2, 1, Some(2));

    assert!(result.is_some(), "Pickup insertion causing downstream LIFO violation should be rejected");
}

#[test]
fn test_multiple_valid_lifo_sequence() {
    // Tour: [Start(0), P1(1), P2(2), P3(3), D3(4), D2(5)]
    // Complete valid LIFO sequence when we add D1 at position 6
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_lifo_vehicle("v1", &["wheelchair"]))
        .build();

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_lifo_pickup(10, "wheelchair", 1))    // idx 1: P1
                .add_activity(create_lifo_pickup(20, "wheelchair", 2))    // idx 2: P2
                .add_activity(create_lifo_pickup(30, "wheelchair", 3))    // idx 3: P3
                .add_activity(create_lifo_delivery(40, "wheelchair", 3))  // idx 4: D3
                .add_activity(create_lifo_delivery(50, "wheelchair", 2))  // idx 5: D2
                .build(),
        )
        .build();

    // Insert D1 at position 6 (after D2) - completes valid LIFO sequence
    // insertion_idx=6, prev_idx=5, next=None
    let w1_delivery = create_lifo_delivery(60, "wheelchair", 1);
    let result = evaluate_insertion(&route_ctx, &w1_delivery, 6, 5, None);

    assert!(result.is_none(), "Final delivery completing LIFO sequence should be accepted");
}
