use super::*;
use crate::construction::heuristics::ActivityContext;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{test_driver, test_vehicle_with_id, FleetBuilder, TestSingleBuilder};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};
use crate::models::common::{Demand, Distance, Location, Profile, Schedule};
use crate::models::problem::{Multi, TransportCost, TravelTime};
use crate::models::solution::{Activity, Place, Route};
use std::sync::Arc;

const MAX_RIDE_DURATION_CODE: ViolationCode = ViolationCode(1200);

/// Test transport cost that returns scaled distance as duration.
/// Duration = |to - from| * scale_factor
struct ScaledTransportCost {
    scale: f64,
}

impl ScaledTransportCost {
    fn new(scale: f64) -> Self {
        Self { scale }
    }

    fn new_shared(scale: f64) -> Arc<dyn TransportCost + Send + Sync> {
        Arc::new(Self::new(scale))
    }
}

impl TransportCost for ScaledTransportCost {
    fn duration_approx(&self, _: &Profile, from: Location, to: Location) -> Duration {
        (to.abs_diff(from) as f64) * self.scale
    }

    fn distance_approx(&self, _: &Profile, from: Location, to: Location) -> Distance {
        to.abs_diff(from) as f64
    }

    fn duration(&self, _: &Route, from: Location, to: Location, _: TravelTime) -> Duration {
        (to.abs_diff(from) as f64) * self.scale
    }

    fn distance(&self, _: &Route, from: Location, to: Location, _: TravelTime) -> Distance {
        to.abs_diff(from) as f64
    }

    fn size(&self) -> usize {
        1
    }
}

#[test]
fn can_create_max_ride_duration_feature() {
    // Basic test to ensure the feature compiles and can be created
    let transport = ScaledTransportCost::new_shared(1.0);
    let result = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport);
    assert!(result.is_ok());
}

#[test]
fn test_max_ride_duration_dimension_on_multi() {
    // Create a pickup single
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build_shared();

    // Create a delivery single
    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build_shared();

    // Create Multi job with max ride duration
    let mut dimens: Dimensions = Default::default();
    dimens.set_job_max_ride_duration(600.0); // 10 minutes

    // Note: Multi::new_shared will bind these singles, so we can't clone them beforehand
    let multi = Multi::new_shared(vec![pickup, delivery], dimens);

    // Verify max ride duration is accessible from child singles via the Multi
    assert_eq!(multi.dimens.get_job_max_ride_duration(), Some(&600.0));

    // Verify we can get the Multi from each child single
    let pickup_single = &multi.jobs[0];
    let delivery_single = &multi.jobs[1];
    assert_eq!(Multi::roots(pickup_single).unwrap().dimens.get_job_max_ride_duration(), Some(&600.0));
    assert_eq!(Multi::roots(delivery_single).unwrap().dimens.get_job_max_ride_duration(), Some(&600.0));
}

#[test]
fn test_is_pickup_detection() {
    let transport = ScaledTransportCost::new_shared(1.0);
    let constraint = MaxRideDurationConstraint { code: MAX_RIDE_DURATION_CODE, transport };

    // Create a pickup single job
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build();

    assert!(constraint.is_pickup(&pickup));
    assert!(!constraint.is_delivery(&pickup));
}

#[test]
fn test_is_delivery_detection() {
    let transport = ScaledTransportCost::new_shared(1.0);
    let constraint = MaxRideDurationConstraint { code: MAX_RIDE_DURATION_CODE, transport };

    // Create a delivery single job
    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build();

    assert!(!constraint.is_pickup(&delivery));
    assert!(constraint.is_delivery(&delivery));
}

#[test]
fn test_is_same_job_detection() {
    let transport = ScaledTransportCost::new_shared(1.0);
    let constraint = MaxRideDurationConstraint { code: MAX_RIDE_DURATION_CODE, transport };

    // Create a pickup single
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build_shared();

    // Create a delivery single
    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build_shared();

    // Create Multi job - note: do not clone the Arc before passing to Multi
    let dimens: Dimensions = Default::default();
    let multi = Multi::new_shared(vec![pickup, delivery], dimens);

    // Get references to the bound singles
    let pickup_single = &multi.jobs[0];
    let delivery_single = &multi.jobs[1];

    // Verify same job detection
    assert!(constraint.is_same_job(pickup_single, delivery_single));
    assert!(constraint.is_same_job(delivery_single, pickup_single));

    // Create a different single
    let different = TestSingleBuilder::default().build();
    assert!(!constraint.is_same_job(pickup_single, &different));
}

// Helper to create a pickup activity with specific location and schedule
fn create_pickup_activity(location: usize, departure: f64, single: Arc<Single>) -> Activity {
    Activity {
        place: Place { idx: 0, location, duration: 60.0, time: TimeWindow::new(0.0, 1000.0) },
        schedule: Schedule { arrival: departure - 60.0, departure },
        job: Some(single),
        commute: None,
    }
}

// Helper to create a delivery activity with specific location
fn create_delivery_activity(location: usize, single: Arc<Single>) -> Activity {
    Activity {
        place: Place { idx: 0, location, duration: 60.0, time: TimeWindow::new(0.0, 1000.0) },
        schedule: Schedule { arrival: 0.0, departure: 0.0 },
        job: Some(single),
        commute: None,
    }
}

// Helper to create a Multi job with pickup and delivery singles
fn create_pudo_multi_job(max_ride_duration: Option<Duration>) -> Arc<Multi> {
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    pickup_builder.location(Some(10)); // pickup location
    let pickup = pickup_builder.build_shared();

    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    delivery_builder.location(Some(20)); // delivery location
    let delivery = delivery_builder.build_shared();

    let mut dimens: Dimensions = Default::default();
    if let Some(duration) = max_ride_duration {
        dimens.set_job_max_ride_duration(duration);
    }

    Multi::new_shared(vec![pickup, delivery], dimens)
}

#[test]
fn test_delivery_insertion_violates_max_ride_duration() {
    // Create transport that takes 100 seconds per unit distance
    // Distance from location 10 to 20 = 10 units = 1000 seconds travel
    let transport = ScaledTransportCost::new_shared(100.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();

    // Create multi job with 500 second max ride duration (travel will take 1000s, so it should violate)
    let multi = create_pudo_multi_job(Some(500.0));
    let pickup_single = multi.jobs[0].clone();
    let delivery_single = multi.jobs[1].clone();

    // Build a route with the pickup already inserted
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_pickup_activity(10, 100.0, pickup_single))
                .build(),
        )
        .build();

    // Create delivery activity to insert
    let delivery_activity = create_delivery_activity(20, delivery_single);

    // Create activity context for inserting delivery after pickup
    // Tour is: [start(0), pickup(1), end(2)]
    // We want to insert at leg 1, which is between pickup (index 1) and end (index 2)
    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(), // pickup
        target: &delivery_activity,
        next: route_ctx.route().tour.get(2), // end
    };

    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Evaluate constraint - should return violation
    let result = feature.constraint.unwrap().evaluate(&move_ctx);

    assert!(result.is_some(), "Expected constraint violation for ride duration exceeding limit");
    assert_eq!(result.unwrap().code, MAX_RIDE_DURATION_CODE);
}

#[test]
fn test_delivery_insertion_within_max_ride_duration() {
    // Create transport that takes 10 seconds per unit distance
    // Distance from location 10 to 20 = 10 units = 100 seconds travel
    let transport = ScaledTransportCost::new_shared(10.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();

    // Create multi job with 500 second max ride duration (travel will take 100s, so it should be OK)
    let multi = create_pudo_multi_job(Some(500.0));
    let pickup_single = multi.jobs[0].clone();
    let delivery_single = multi.jobs[1].clone();

    // Build a route with the pickup already inserted
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_pickup_activity(10, 100.0, pickup_single))
                .build(),
        )
        .build();

    // Create delivery activity to insert
    let delivery_activity = create_delivery_activity(20, delivery_single);

    // Create activity context for inserting delivery after pickup
    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(), // pickup
        target: &delivery_activity,
        next: route_ctx.route().tour.get(2), // end
    };

    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Evaluate constraint - should return None (no violation)
    let result = feature.constraint.unwrap().evaluate(&move_ctx);

    assert!(result.is_none(), "Expected no constraint violation when ride duration is within limit");
}

#[test]
fn test_no_constraint_check_without_max_ride_duration() {
    // Create transport
    let transport = ScaledTransportCost::new_shared(100.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();

    // Create multi job WITHOUT max ride duration
    let multi = create_pudo_multi_job(None);
    let pickup_single = multi.jobs[0].clone();
    let delivery_single = multi.jobs[1].clone();

    // Build a route with the pickup already inserted
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_pickup_activity(10, 100.0, pickup_single))
                .build(),
        )
        .build();

    // Create delivery activity to insert
    let delivery_activity = create_delivery_activity(20, delivery_single);

    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(),
        target: &delivery_activity,
        next: route_ctx.route().tour.get(2),
    };

    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Evaluate constraint - should return None (no max ride duration set)
    let result = feature.constraint.unwrap().evaluate(&move_ctx);

    assert!(result.is_none(), "Expected no constraint check when max ride duration is not set");
}

#[test]
fn test_delivery_insertion_at_exact_limit() {
    // Create transport that takes 44 seconds per unit distance
    // Distance from location 10 to 20 = 10 units = 440 seconds travel
    // With pickup departure at 100, delivery arrival = 100 + 440 = 540
    // Ride duration = 540 - 100 = 440 seconds, exactly at the limit
    let transport = ScaledTransportCost::new_shared(44.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();

    // Create multi job with 440 second max ride duration (exactly at limit)
    let multi = create_pudo_multi_job(Some(440.0));
    let pickup_single = multi.jobs[0].clone();
    let delivery_single = multi.jobs[1].clone();

    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_pickup_activity(10, 100.0, pickup_single))
                .build(),
        )
        .build();

    let delivery_activity = create_delivery_activity(20, delivery_single);

    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(),
        target: &delivery_activity,
        next: route_ctx.route().tour.get(2),
    };

    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Evaluate constraint - should return None (exactly at limit, not exceeding)
    let result = feature.constraint.unwrap().evaluate(&move_ctx);

    assert!(result.is_none(), "Expected no violation when ride duration is exactly at limit");
}

#[test]
fn test_delivery_insertion_just_over_limit() {
    // Create transport that takes 45 seconds per unit distance
    // Distance = 10 units = 450 seconds travel
    // Ride duration = 450 > 440 limit
    let transport = ScaledTransportCost::new_shared(45.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();

    // Create multi job with 440 second max ride duration
    let multi = create_pudo_multi_job(Some(440.0));
    let pickup_single = multi.jobs[0].clone();
    let delivery_single = multi.jobs[1].clone();

    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(create_pickup_activity(10, 100.0, pickup_single))
                .build(),
        )
        .build();

    let delivery_activity = create_delivery_activity(20, delivery_single);

    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(),
        target: &delivery_activity,
        next: route_ctx.route().tour.get(2),
    };

    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Evaluate constraint - should return violation
    let result = feature.constraint.unwrap().evaluate(&move_ctx);

    assert!(result.is_some(), "Expected violation when ride duration exceeds limit");
}
