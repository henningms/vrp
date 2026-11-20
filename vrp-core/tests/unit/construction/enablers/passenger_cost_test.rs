use super::*;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::{Demand, Dimensions, Location};
use crate::models::problem::Multi;
use crate::models::solution::Activity;
use std::sync::Arc;

const PER_PASSENGER: Cost = 100.;

/// Creates pickup and delivery activities of one pickup-delivery job with the given passengers. The job
/// is returned too: activities reach it through a weak link, so callers keep it alive.
fn pudo(pickup_at: Location, delivery_at: Location, passengers: usize) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup = TestSingleBuilder::default();
    pickup.location(Some(pickup_at)).demand(Demand::pudo_pickup(1));
    let mut delivery = TestSingleBuilder::default();
    delivery.location(Some(delivery_at)).demand(Demand::pudo_delivery(1));
    let mut dimens = Dimensions::default();
    dimens.set_job_passengers(passengers);
    let multi = Multi::new_shared(vec![pickup.build_shared(), delivery.build_shared()], dimens);

    let pickup = ActivityBuilder::with_location(pickup_at).job(Some(multi.jobs[0].clone())).build();
    let delivery = ActivityBuilder::with_location(delivery_at).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup, delivery)
}

fn service(location: Location, passengers: Option<usize>) -> Activity {
    let mut single = TestSingleBuilder::default();
    single.location(Some(location));
    if let Some(passengers) = passengers {
        single.dimens_mut().set_job_passengers(passengers);
    }

    ActivityBuilder::with_location(location).job(Some(single.build_shared())).build()
}

fn route_with(per_passenger: Option<Cost>, activities: &[&Activity]) -> RouteContext {
    let mut vehicle = TestVehicleBuilder::default();
    vehicle.id("v1");
    if let Some(per_passenger) = per_passenger {
        vehicle.dimens_mut().set_vehicle_per_passenger_cost(per_passenger);
    }
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle.build()).build();

    let mut builder = RouteBuilder::default();
    builder.with_vehicle(&fleet, "v1");
    activities.iter().for_each(|activity| {
        builder.add_activity(activity.deep_copy());
    });

    RouteContextBuilder::default().with_route(builder.build()).build()
}

#[test]
fn counts_a_multi_job_once() {
    let (_job, pickup, delivery) = pudo(5, 10, 3);

    assert_eq!(count_tour_passengers(&route_with(None, &[&pickup, &delivery]).route().tour), 3);
}

#[test]
fn counts_passengers_of_every_job_and_none_for_a_job_without_them() {
    let (_first, first_pickup, first_delivery) = pudo(5, 10, 1);
    let (_second, second_pickup, second_delivery) = pudo(6, 11, 2);
    let with_passengers = service(7, Some(4));
    let without_passengers = service(8, None);
    let activities =
        [&first_pickup, &second_pickup, &with_passengers, &without_passengers, &first_delivery, &second_delivery];

    assert_eq!(count_tour_passengers(&route_with(None, &activities).route().tour), 7);
}

#[test]
fn route_passenger_cost_is_none_without_per_passenger_cost() {
    let (_job, pickup, delivery) = pudo(5, 10, 2);

    assert_eq!(get_route_passenger_cost(&route_with(None, &[&pickup, &delivery])), None);
}

#[test]
fn route_passenger_cost_charges_every_passenger() {
    let (_first, first_pickup, first_delivery) = pudo(5, 10, 1);
    let (_second, second_pickup, second_delivery) = pudo(6, 11, 1);
    let route_ctx =
        route_with(Some(PER_PASSENGER), &[&first_pickup, &second_pickup, &first_delivery, &second_delivery]);

    assert_eq!(get_route_passenger_cost(&route_ctx), Some(2. * PER_PASSENGER));
    assert_eq!(get_route_passenger_cost(&route_with(Some(PER_PASSENGER), &[])), Some(0.));
}

#[test]
fn job_passenger_cost_needs_the_vehicle_cost_and_the_job_passengers() {
    let (job, ..) = pudo(5, 10, 3);
    let job = Job::Multi(job);
    let without_passengers = Job::Single(TestSingleBuilder::default().build_shared());
    let with_cost = route_with(Some(PER_PASSENGER), &[]);
    let without_cost = route_with(None, &[]);

    assert_eq!(get_job_passenger_cost(&with_cost.route().actor.vehicle, &job), Some(3. * PER_PASSENGER));
    assert_eq!(get_job_passenger_cost(&without_cost.route().actor.vehicle, &job), None);
    assert_eq!(get_job_passenger_cost(&with_cost.route().actor.vehicle, &without_passengers), None);
}
