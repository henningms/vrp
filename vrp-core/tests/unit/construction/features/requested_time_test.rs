use super::*;
use crate::construction::heuristics::ActivityContext;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{
    FleetBuilder, TestSingleBuilder, TestTransportCost, test_driver, test_vehicle_with_id,
};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};
use crate::models::common::{ConfigurableLoad, Demand, Schedule, TimeWindow};
use crate::models::solution::{Activity, Place};
use std::sync::Arc;

fn requested_activity(location: usize, arrival: Timestamp, window_start: Timestamp, requested: Timestamp) -> Activity {
    let mut single = TestSingleBuilder::default().build();
    single.dimens.set_job_requested_times(HashMap::from([(0, requested)]));
    Activity {
        place: Place { idx: 0, location, duration: 0.0, time: TimeWindow::new(window_start, 10_000.0) },
        schedule: Schedule::new(arrival, arrival.max(window_start)),
        job: Some(Arc::new(single)),
        commute: None,
    }
}

fn objective() -> RequestedTimeObjective {
    RequestedTimeObjective {
        penalty: Arc::new(RequestedTimePenalty::default()),
        transport: TestTransportCost::new_shared(),
    }
}

#[test]
fn can_calculate_early_penalty() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    // Arriving 30 minutes (1800 seconds) early
    let arrival = 1000.0;
    let requested = 2800.0; // 1800 seconds later

    // Expected: 1800 seconds * (1.0 / 60) = 30 penalty
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 30.0).abs() < 0.001, "Expected 30.0, got {}", result);
}

#[test]
fn can_calculate_late_penalty() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    // Arriving 30 minutes (1800 seconds) late
    let arrival = 2800.0;
    let requested = 1000.0; // 1800 seconds earlier

    // Expected: 1800 seconds * (2.0 / 60) = 60 penalty
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 60.0).abs() < 0.001, "Expected 60.0, got {}", result);
}

#[test]
fn can_calculate_zero_penalty_for_on_time() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    let arrival = 1000.0;
    let requested = 1000.0;

    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 0.0).abs() < 0.001, "Expected 0.0, got {}", result);
}

#[test]
fn can_use_default_penalty() {
    let penalty = RequestedTimePenalty::default();

    // Arriving 60 minutes (3600 seconds) late
    let arrival = 4600.0;
    let requested = 1000.0;

    // Expected: 3600 seconds * (1.0 / 60) = 60 penalty (default 1.0 per minute)
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 60.0).abs() < 0.001, "Expected 60.0, got {}", result);
}

#[test]
fn waiting_before_service_is_not_penalized_as_early() {
    let activity = requested_activity(1, 100.0, 200.0, 200.0);

    assert_eq!(objective().calculate_activity_penalty(&activity), Some(0.0));
}

#[test]
fn requested_time_penalty_survives_configurable_capacity_dimensions() {
    let demand = Demand {
        pickup: (ConfigurableLoad::default(), ConfigurableLoad::from_load(vec![1, 0])),
        delivery: (ConfigurableLoad::default(), ConfigurableLoad::default()),
    };
    let mut single = TestSingleBuilder::default().demand(demand).build();
    single.dimens.set_job_requested_times(HashMap::from([(0, 100.0)]));
    let activity = Activity {
        place: Place { idx: 0, location: 1, duration: 0.0, time: TimeWindow::new(0.0, 10_000.0) },
        schedule: Schedule::new(160.0, 160.0),
        job: Some(Arc::new(single)),
        commute: None,
    };

    assert_eq!(objective().calculate_activity_penalty(&activity), Some(1.0));
}

#[test]
fn insertion_estimate_includes_requested_time_delay_to_downstream_activity() {
    let existing = requested_activity(10, 10.0, 0.0, 10.0);
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v1")).build();
    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").add_activity(existing).build())
        .build();
    let target = Activity {
        place: Place { idx: 0, location: 20, duration: 0.0, time: TimeWindow::new(0.0, 10_000.0) },
        schedule: Schedule::new(0.0, 0.0),
        job: Some(TestSingleBuilder::default().build_shared()),
        commute: None,
    };
    let activity_ctx = ActivityContext {
        index: 0,
        prev: route_ctx.route().tour.get(0).unwrap(),
        target: &target,
        next: route_ctx.route().tour.get(1),
    };
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let move_ctx = MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx);

    // Existing service moves from t=10 to t=30, a 20-second late deviation.
    let result = objective().estimate(&move_ctx);
    assert!((result - 20.0 / 60.0).abs() < 0.001, "unexpected insertion delta: {result}");
}
