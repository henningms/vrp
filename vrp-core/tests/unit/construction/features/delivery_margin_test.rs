use super::*;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{
    FleetBuilder, TestActivityCost, TestSingleBuilder, TestTransportCost, test_driver, test_vehicle_with_id,
};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};
use crate::models::solution::Place;

fn objective() -> DeliveryMargin {
    DeliveryMargin {
        target_margin: 300.,
        transport: TestTransportCost::new_shared(),
        activity: Arc::new(TestActivityCost::default()),
    }
}

fn trip() -> Arc<Multi> {
    let pickup = TestSingleBuilder::default().demand(Demand::pudo_pickup(1)).build_shared();
    let own = TestSingleBuilder::default().demand(Demand::pudo_delivery(1)).build_shared();
    let mut companion = TestSingleBuilder::default().demand(Demand::pudo_delivery(1)).build();
    companion.dimens.set_job_outside_ride_limit(true);
    Multi::new_shared(vec![pickup, own, Arc::new(companion)], Default::default())
}

fn activity(
    single: Arc<Single>,
    location: usize,
    window: (Timestamp, Timestamp),
    arrival: Timestamp,
    duration: Duration,
) -> Activity {
    Activity {
        place: Place { idx: 0, location, duration, time: TimeWindow::new(window.0, window.1) },
        schedule: Schedule::new(arrival, arrival.max(window.0) + duration),
        job: Some(single),
        commute: None,
    }
}

#[test]
fn delivery_margin_uses_own_service_start_and_stops_rewarding_earliness_at_target() {
    let rider = trip();
    let score = objective();
    let delivery = activity(rider.jobs[1].clone(), 1, (100., 500.), 0., 60.);
    assert_eq!(score.shortfall(&delivery, 0.), 0.);
    assert_eq!(score.shortfall(&delivery, 200.), 0.);
    assert_eq!(score.shortfall(&delivery, 201.), 1.);
    assert_eq!(score.shortfall(&delivery, 500.), 300.);
    let waiting = activity(rider.jobs[1].clone(), 1, (460., 500.), 0., 60.);
    assert_eq!(score.shortfall(&waiting, 0.), 260.);
    for i in [0, 2] {
        assert_eq!(score.shortfall(&activity(rider.jobs[i].clone(), 1, (0., 500.), 470., 0.), 470.), 0.);
    }
}

#[test]
fn delivery_margin_insertion_delta_counts_other_rider_and_charges_target_service_once() {
    let rider = trip();
    let existing = activity(rider.jobs[1].clone(), 10, (0., 330.), 10., 4.);
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(test_vehicle_with_id("v")).build();
    let route = RouteContextBuilder::default()
        .with_route(RouteBuilder::default().with_vehicle(&fleet, "v").add_activity(existing).build())
        .build();
    let second = trip();
    let insert = activity(second.jobs[1].clone(), 20, (25., 315.), 0., 10.);
    let context = ActivityContext {
        index: 0,
        prev: route.route().tour.get(0).unwrap(),
        target: &insert,
        next: route.route().tour.get(1),
    };
    let solution = TestInsertionContextBuilder::default().build().solution;
    // The new service starts at 25 (10 seconds short), leaves at 35, and delays
    // the existing set-down to 45 (15 seconds short). Its prior margin cost was zero.
    assert_eq!(objective().estimate(&MoveContext::activity(&solution, &route, &context)), 25.);
}

#[test]
fn delivery_margin_rejects_invalid_target() {
    let score = objective();
    assert!(create_delivery_margin_feature("margin", -1., score.transport, score.activity).is_err());
}
