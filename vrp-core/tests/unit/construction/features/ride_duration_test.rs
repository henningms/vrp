use super::*;
use crate::construction::heuristics::ActivityContext;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, test_driver, test_vehicle_with_id};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};
use crate::models::common::{ConfigurableLoad, Demand, Distance, Location, MultiDimLoad, Profile, Schedule};
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
    // Create a pickup single job
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build();

    assert!(is_pickup(&pickup));
    assert!(!is_delivery(&pickup));
}

#[test]
fn test_is_delivery_detection() {
    // Create a delivery single job
    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build();

    assert!(!is_pickup(&delivery));
    assert!(is_delivery(&delivery));
}

fn configurable_pudo_pickup_demand(value: i32) -> Demand<ConfigurableLoad> {
    Demand {
        pickup: (ConfigurableLoad::default(), ConfigurableLoad::from_load(vec![value])),
        delivery: (ConfigurableLoad::default(), ConfigurableLoad::default()),
    }
}

fn configurable_pudo_delivery_demand(value: i32) -> Demand<ConfigurableLoad> {
    Demand {
        pickup: (ConfigurableLoad::default(), ConfigurableLoad::default()),
        delivery: (ConfigurableLoad::default(), ConfigurableLoad::from_load(vec![value])),
    }
}

fn multi_dim_pudo_pickup_demand(value: i32) -> Demand<MultiDimLoad> {
    Demand {
        pickup: (MultiDimLoad::default(), MultiDimLoad::new(vec![value])),
        delivery: (MultiDimLoad::default(), MultiDimLoad::default()),
    }
}

fn multi_dim_pudo_delivery_demand(value: i32) -> Demand<MultiDimLoad> {
    Demand {
        pickup: (MultiDimLoad::default(), MultiDimLoad::default()),
        delivery: (MultiDimLoad::default(), MultiDimLoad::new(vec![value])),
    }
}

#[test]
fn test_pickup_and_delivery_detection_with_configurable_and_multi_dim_demand() {
    for (pickup, delivery) in [
        {
            let mut pickup = TestSingleBuilder::default();
            pickup.demand(configurable_pudo_pickup_demand(1));
            let mut delivery = TestSingleBuilder::default();
            delivery.demand(configurable_pudo_delivery_demand(1));
            (pickup.build(), delivery.build())
        },
        {
            let mut pickup = TestSingleBuilder::default();
            pickup.demand(multi_dim_pudo_pickup_demand(1));
            let mut delivery = TestSingleBuilder::default();
            delivery.demand(multi_dim_pudo_delivery_demand(1));
            (pickup.build(), delivery.build())
        },
    ] {
        assert!(is_pickup(&pickup));
        assert!(!is_delivery(&pickup));
        assert!(!is_pickup(&delivery));
        assert!(is_delivery(&delivery));
    }
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

fn create_configurable_pudo_multi_job(max_ride_duration: Duration) -> Arc<Multi> {
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.demand(configurable_pudo_pickup_demand(1));
    pickup_builder.location(Some(10));
    let pickup = pickup_builder.build_shared();

    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.demand(configurable_pudo_delivery_demand(1));
    delivery_builder.location(Some(20));
    let delivery = delivery_builder.build_shared();

    let mut dimens: Dimensions = Default::default();
    dimens.set_job_max_ride_duration(max_ride_duration);
    Multi::new_shared(vec![pickup, delivery], dimens)
}

#[test]
fn test_delivery_insertion_violates_max_ride_duration_with_configurable_demand() {
    let transport = ScaledTransportCost::new_shared(100.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();
    let multi = create_configurable_pudo_multi_job(500.0);
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

    let result = feature.constraint.unwrap().evaluate(&move_ctx);
    assert!(result.is_some(), "configurable demand must enforce max ride duration");
    assert_eq!(result.unwrap().code, MAX_RIDE_DURATION_CODE);
}

#[test]
fn test_unrelated_insertion_between_pickup_and_delivery_violates_max_ride_duration() {
    let transport = ScaledTransportCost::new_shared(100.0);
    let feature = create_max_ride_duration_feature("test", MAX_RIDE_DURATION_CODE, transport).unwrap();
    let multi = create_configurable_pudo_multi_job(1500.0);
    let pickup = create_pickup_activity(10, 100.0, multi.jobs[0].clone());
    let delivery = create_delivery_activity(20, multi.jobs[1].clone());
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default().with_vehicle(&fleet, "v1").add_activity(pickup).add_activity(delivery).build(),
        )
        .build();

    // The existing direct ride is 1000 seconds. Inserting an unrelated stop at
    // location 30 makes the projected ride 3000 seconds.
    let unrelated = Activity {
        place: Place { idx: 0, location: 30, duration: 0.0, time: TimeWindow::new(0.0, 10_000.0) },
        schedule: Schedule::new(0.0, 0.0),
        job: Some(TestSingleBuilder::default().build_shared()),
        commute: None,
    };
    let activity_ctx = ActivityContext {
        index: 1,
        prev: route_ctx.route().tour.get(1).unwrap(),
        target: &unrelated,
        next: route_ctx.route().tour.get(2),
    };
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    let result = feature.constraint.unwrap().evaluate(&move_ctx);
    assert_eq!(result.map(|violation| violation.code), Some(MAX_RIDE_DURATION_CODE));
}

#[test]
fn invalid_ride_after_route_change_is_returned_to_unassigned() {
    let multi = create_configurable_pudo_multi_job(500.0);
    let job = Job::Multi(multi.clone());
    let pickup = create_pickup_activity(10, 100.0, multi.jobs[0].clone());
    let mut delivery = create_delivery_activity(20, multi.jobs[1].clone());
    delivery.schedule = Schedule::new(1000.0, 1060.0);
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default().with_vehicle(&fleet, "v1").add_activity(pickup).add_activity(delivery).build(),
        )
        .build();
    let mut insertion_ctx = TestInsertionContextBuilder::default().with_routes(vec![route_ctx]).build();

    MaxRideDurationState {}.accept_solution_state(&mut insertion_ctx.solution);

    assert!(!insertion_ctx.solution.routes[0].route().tour.contains(&job));
    assert!(insertion_ctx.solution.unassigned.contains_key(&job));
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
