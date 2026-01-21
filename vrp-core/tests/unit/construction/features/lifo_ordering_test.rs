use super::*;

use crate::helpers::models::problem::{TestSingleBuilder, TestVehicleBuilder};

const LIFO_VIOLATION_CODE: ViolationCode = ViolationCode(1100);

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
fn test_vehicle_lifo_required_dimension() {
    let mut builder = TestVehicleBuilder::default();
    builder.dimens_mut().set_vehicle_lifo_required(true);
    let vehicle = builder.build();

    assert_eq!(vehicle.dimens.get_vehicle_lifo_required(), Some(&true));
}

#[test]
fn test_vehicle_lifo_not_required_dimension() {
    let mut builder = TestVehicleBuilder::default();
    builder.dimens_mut().set_vehicle_lifo_required(false);
    let vehicle = builder.build();

    assert_eq!(vehicle.dimens.get_vehicle_lifo_required(), Some(&false));
}

#[test]
fn test_feature_creation() {
    let feature = create_lifo_ordering_feature().unwrap();
    assert_eq!(feature.name, "lifo_ordering");
}

#[test]
fn test_is_pickup_detection() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    // Create a pickup single job
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build();

    assert!(constraint.is_pickup(&pickup));
    assert!(!constraint.is_delivery(&pickup));
}

#[test]
fn test_is_delivery_detection() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    // Create a delivery single job
    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build();

    assert!(!constraint.is_pickup(&delivery));
    assert!(constraint.is_delivery(&delivery));
}

#[test]
fn test_regular_job_not_pickup_or_delivery() {
    let constraint = LifoOrderingConstraint { code: LIFO_VIOLATION_CODE };

    // Create a regular job (no pudo demand)
    let regular = TestSingleBuilder::default().build();

    assert!(!constraint.is_pickup(&regular));
    assert!(!constraint.is_delivery(&regular));
}

// TODO: Add integration tests that verify LIFO ordering:
// - Valid tour: [Pickup L1, Pickup L2, Delivery L2, Delivery L1]
// - Invalid tour: [Pickup L1, Pickup L2, Delivery L1, Delivery L2]
// - Interleaving with regular jobs
// - Multiple LIFO groups
// These require constructing full route contexts with tours, which is more complex
