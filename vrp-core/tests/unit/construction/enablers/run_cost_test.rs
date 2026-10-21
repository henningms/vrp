use super::*;
use crate::construction::enablers::{TotalDistanceTourState, TotalDurationTourState};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::{Demand, Location};

const PER_RUN: Cost = 100.;

/// Creates pickup and delivery activities of one pickup-delivery job. The job is returned too:
/// activities reach it through a weak link, so callers keep it alive.
fn pudo(pickup_at: Location, delivery_at: Location) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup = TestSingleBuilder::default();
    pickup.location(Some(pickup_at)).demand(Demand::pudo_pickup(1));
    let mut delivery = TestSingleBuilder::default();
    delivery.location(Some(delivery_at)).demand(Demand::pudo_delivery(1));
    let multi = Multi::new_shared(vec![pickup.build_shared(), delivery.build_shared()], Default::default());

    let pickup = ActivityBuilder::with_location(pickup_at).job(Some(multi.jobs[0].clone())).build();
    let delivery = ActivityBuilder::with_location(delivery_at).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup, delivery)
}

fn service(location: Location) -> Activity {
    ActivityBuilder::with_location(location)
        .job(Some(TestSingleBuilder::default().location(Some(location)).build_shared()))
        .build()
}

/// Builds a route with copies of the given activities, which starts and ends at location 0.
fn route_with(per_run: Option<Cost>, activities: &[&Activity]) -> RouteContext {
    let mut vehicle = TestVehicleBuilder::default();
    vehicle.id("v1");
    if let Some(per_run) = per_run {
        vehicle.dimens_mut().set_vehicle_per_run_cost(per_run);
    }
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle.build()).build();

    let mut builder = RouteBuilder::default();
    builder.with_vehicle(&fleet, "v1");
    activities.iter().for_each(|activity| {
        builder.add_activity(activity.deep_copy());
    });

    RouteContextBuilder::default().with_route(builder.build()).build()
}

fn runs_of(activities: &[&Activity]) -> usize {
    count_tour_runs(&route_with(None, activities).route().tour)
}

/// Estimates the change in run fees of inserting `target` after the tour activity at `leg_index`.
fn estimate(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Option<Cost> {
    let tour = &route_ctx.route().tour;
    let activity_ctx =
        ActivityContext { index: leg_index, prev: tour.get(leg_index).unwrap(), target, next: tour.get(leg_index + 1) };

    estimate_route_run_cost(route_ctx, &activity_ctx)
}

/// Checks that the estimate of inserting `target` at every position of the route, with and without
/// cached runs, is the change of run fees after the insertion.
fn assert_estimates_match_fee_change(activities: &[&Activity], target: &Activity) {
    let mut cached = route_with(Some(PER_RUN), activities);
    update_route_runs(&mut cached);
    let uncached = route_with(Some(PER_RUN), activities);
    let before = get_route_run_cost(&cached).unwrap();

    (0..=activities.len()).for_each(|position| {
        let mut inserted = activities.to_vec();
        inserted.insert(position, target);
        let after = get_route_run_cost(&route_with(Some(PER_RUN), &inserted)).unwrap();

        let expected = after - before;
        assert_eq!(estimate(&cached, target, position).unwrap_or_default(), expected, "position {position}");
        assert_eq!(estimate(&uncached, target, position).unwrap_or_default(), expected, "position {position}");
    });
}

#[test]
fn empty_tour_has_no_runs() {
    assert_eq!(runs_of(&[]), 0);
}

#[test]
fn one_rider_makes_one_run() {
    let (_a, ap, ad) = pudo(1, 5);

    assert_eq!(runs_of(&[&ap, &ad]), 1);
}

#[test]
fn back_to_back_riders_make_two_runs() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(6, 9);

    assert_eq!(runs_of(&[&ap, &ad, &bp, &bd]), 2);
}

#[test]
fn riders_on_board_together_make_one_run() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(2, 9);

    assert_eq!(runs_of(&[&ap, &bp, &ad, &bd]), 1);
}

#[test]
fn handover_at_one_stop_makes_two_runs_in_either_order() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);

    assert_eq!(runs_of(&[&ap, &ad, &bp, &bd]), 2);
    assert_eq!(runs_of(&[&ap, &bp, &ad, &bd]), 2);
}

#[test]
fn rider_on_board_across_handover_joins_both_runs() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    let (_c, cp, cd) = pudo(2, 7);

    assert_eq!(runs_of(&[&ap, &cp, &ad, &bp, &cd, &bd]), 1);
    assert_eq!(runs_of(&[&ap, &cp, &bp, &ad, &cd, &bd]), 1);
}

#[test]
fn handover_split_by_another_stop_makes_one_run() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);

    // [start, ap, bp, s, ad, bd, end]: both riders are on board on the way to the service and back.
    assert_eq!(runs_of(&[&ap, &bp, &service(7), &ad, &bd]), 1);
    // [start, ap, ad, s, bp, bd, end]: the vehicle is empty on the way to the service and back.
    assert_eq!(runs_of(&[&ap, &ad, &service(7), &bp, &bd]), 2);
}

#[test]
fn job_picked_up_and_delivered_at_one_stop_opens_no_run() {
    let (_a, ap, ad) = pudo(3, 3);
    let (_b, bp, bd) = pudo(1, 9);

    assert_eq!(runs_of(&[&ap, &ad]), 0);
    assert_eq!(runs_of(&[&bp, &ap, &ad, &bd]), 1);
}

#[test]
fn single_jobs_and_incomplete_multi_jobs_open_no_run() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, _) = pudo(6, 9);

    assert_eq!(runs_of(&[&service(2), &bp, &service(3)]), 0);
    assert_eq!(runs_of(&[&service(2), &ap, &service(3), &ad, &bp]), 1);
}

#[test]
fn route_run_cost_is_none_without_per_run_cost() {
    let (_a, ap, ad) = pudo(1, 5);
    let mut route_ctx = route_with(None, &[&ap, &ad]);
    update_route_runs(&mut route_ctx);

    assert!(route_ctx.state().get_stop_runs().is_none());
    assert_eq!(get_route_run_cost(&route_ctx), None);
    assert_eq!(estimate(&route_ctx, &ad, 1), None);
}

#[test]
fn route_run_cost_charges_every_run() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    let mut route_ctx = route_with(Some(PER_RUN), &[&ap, &bp, &ad, &bd]);

    assert_eq!(get_route_run_cost(&route_ctx), Some(2. * PER_RUN));
    update_route_runs(&mut route_ctx);
    assert_eq!(route_ctx.state().get_stop_runs().map(|stop_runs| stop_runs.runs.len()), Some(2));
    assert_eq!(get_route_run_cost(&route_ctx), Some(2. * PER_RUN));
}

#[test]
fn total_cost_includes_run_fees() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(6, 9);
    let with_fee = |per_run| {
        let mut route_ctx = route_with(per_run, &[&ap, &ad, &bp, &bd]);
        route_ctx.state_mut().set_total_distance(10.);
        route_ctx.state_mut().set_total_duration(20.);
        TestInsertionContextBuilder::default().with_routes(vec![route_ctx]).build().get_total_cost().unwrap()
    };

    // vehicle and driver both cost 1 per distance and time unit
    assert_eq!(with_fee(None), 60.);
    assert_eq!(with_fee(Some(PER_RUN)), 60. + 2. * PER_RUN);
}

#[test]
fn estimate_is_none_for_activity_which_neither_completes_a_job_nor_splits_a_stop() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, _) = pudo(6, 9);
    let route_ctx = route_with(Some(PER_RUN), &[&ap, &ad]);

    assert_eq!(estimate(&route_ctx, &bp, 2), None);
    assert_eq!(estimate(&route_ctx, &service(5), 2), None);
}

#[test]
fn estimate_charges_delivery_inserted_right_before_pickup_at_its_stop() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    // [start, ap, bp, bd, end]: delivering a at b's pickup stop is a handover, before or after the pickup.
    let mut route_ctx = route_with(Some(PER_RUN), &[&ap, &bp, &bd]);
    update_route_runs(&mut route_ctx);

    assert_eq!(estimate(&route_ctx, &ad, 1), Some(PER_RUN));
    assert_eq!(estimate(&route_ctx, &ad, 2), Some(PER_RUN));
}

#[test]
fn estimate_refunds_service_which_splits_a_handover() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    // [start, ap, bp, ad, bd, end]: a service between bp and ad keeps both riders on board.
    let mut route_ctx = route_with(Some(PER_RUN), &[&ap, &bp, &ad, &bd]);
    update_route_runs(&mut route_ctx);

    assert_eq!(estimate(&route_ctx, &service(7), 2), Some(-PER_RUN));
    assert_eq!(estimate(&route_ctx, &service(5), 2), None);
}

#[test]
fn estimates_match_fee_change_for_deliveries() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    let (c, cp, _) = pudo(3, 0);

    [1, 3, 5, 7, 9, 0].into_iter().for_each(|location| {
        let cd = ActivityBuilder::with_location(location).job(Some(c.jobs[1].clone())).build();
        assert_estimates_match_fee_change(&[&ap, &cp, &bp, &ad, &bd], &cd);
        assert_estimates_match_fee_change(&[&ap, &ad, &cp, &bp, &bd], &cd);
        assert_estimates_match_fee_change(&[&cp, &ap, &ad, &bp, &bd], &cd);
    });
}

#[test]
fn estimates_match_fee_change_for_pickups() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    let (c, _, cd) = pudo(0, 7);

    [1, 3, 5, 7, 9, 0].into_iter().for_each(|location| {
        let cp = ActivityBuilder::with_location(location).job(Some(c.jobs[0].clone())).build();
        assert_estimates_match_fee_change(&[&ap, &bp, &ad, &cd, &bd], &cp);
        assert_estimates_match_fee_change(&[&ap, &ad, &bp, &bd, &cd], &cp);
    });
}

#[test]
fn estimates_match_fee_change_for_activities_which_do_not_complete_a_job() {
    let (_a, ap, ad) = pudo(1, 5);
    let (_b, bp, bd) = pudo(5, 9);
    let (_c, cp, cd) = pudo(5, 5);
    let (d, ..) = pudo(2, 8);

    [1, 3, 5, 7, 9, 0].into_iter().for_each(|location| {
        let routes: [&[&Activity]; 3] = [&[&ap, &bp, &ad, &bd], &[&ap, &ad, &bp, &bd], &[&ap, &cp, &bp, &ad, &cd, &bd]];
        routes.into_iter().for_each(|activities| {
            assert_estimates_match_fee_change(activities, &service(location));
            let dp = ActivityBuilder::with_location(location).job(Some(d.jobs[0].clone())).build();
            assert_estimates_match_fee_change(activities, &dp);
        });
    });
}
