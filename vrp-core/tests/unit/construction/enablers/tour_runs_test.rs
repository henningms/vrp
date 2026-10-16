use super::*;
use crate::construction::enablers::{TotalDistanceTourState, TotalDurationTourState};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::Demand;
use crate::models::problem::{JobIdDimension, Multi};
use crate::models::solution::Activity;

/// Creates pickup and delivery activities of one pickup-delivery job. The job is returned too:
/// activities reach it through a weak link, so callers keep it alive.
fn pudo(id: &str) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup = TestSingleBuilder::default();
    pickup.location(Some(1)).demand(Demand::pudo_pickup(1));
    let mut delivery = TestSingleBuilder::default();
    delivery.location(Some(10)).demand(Demand::pudo_delivery(1));

    let mut dimens = crate::models::common::Dimensions::default();
    dimens.set_job_id(id.to_string());
    let multi = Multi::new_shared(vec![pickup.build_shared(), delivery.build_shared()], dimens);

    let pickup = ActivityBuilder::with_location(1).job(Some(multi.jobs[0].clone())).build();
    let delivery = ActivityBuilder::with_location(10).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup, delivery)
}

fn service(location: usize) -> Activity {
    ActivityBuilder::with_location(location)
        .job(Some(TestSingleBuilder::default().location(Some(location)).build_shared()))
        .build()
}

fn route_with(per_run: Option<Cost>, activities: Vec<Activity>) -> RouteContext {
    let mut vehicle = TestVehicleBuilder::default();
    vehicle.id("v1");
    if let Some(per_run) = per_run {
        vehicle.dimens_mut().set_vehicle_per_run_cost(per_run);
    }
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle.build()).build();

    let mut builder = RouteBuilder::default();
    builder.with_vehicle(&fleet, "v1");
    activities.into_iter().for_each(|activity| {
        builder.add_activity(activity);
    });

    RouteContextBuilder::default().with_route(builder.build()).build()
}

fn spans_of(route_ctx: &RouteContext) -> Vec<(usize, usize)> {
    get_tour_runs(&route_ctx.route().tour, |_: &mut (), _| {}).into_iter().map(|run| (run.start, run.end)).collect()
}

/// Returns the run span of inserting `target` after the tour activity at `leg_index`.
fn insertion_span(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Option<(usize, usize)> {
    let tour = &route_ctx.route().tour;
    let activity_ctx =
        ActivityContext { index: leg_index, prev: tour.get(leg_index).unwrap(), target, next: tour.get(leg_index + 1) };

    get_insertion_run_span(route_ctx, &activity_ctx)
}

#[test]
fn empty_tour_has_no_runs() {
    let route_ctx = route_with(None, vec![]);

    assert_eq!(count_tour_runs(&route_ctx.route().tour), 0);
}

#[test]
fn one_rider_makes_one_run() {
    let (_a, ap, ad) = pudo("a");
    let route_ctx = route_with(None, vec![ap, ad]);

    assert_eq!(spans_of(&route_ctx), vec![(1, 2)]);
}

#[test]
fn back_to_back_riders_make_two_runs() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    let route_ctx = route_with(None, vec![ap, ad, bp, bd]);

    assert_eq!(spans_of(&route_ctx), vec![(1, 2), (3, 4)]);
}

#[test]
fn riders_on_board_together_make_one_run() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    let route_ctx = route_with(None, vec![ap, bp, ad, bd]);

    assert_eq!(spans_of(&route_ctx), vec![(1, 4)]);
}

#[test]
fn single_jobs_and_incomplete_multi_jobs_open_no_run() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, _) = pudo("b");
    // [start, s, ap, s, ad, bp, end]: b's delivery is missing, so b opens no run.
    let route_ctx = route_with(None, vec![service(5), ap, service(6), ad, bp]);

    assert_eq!(spans_of(&route_ctx), vec![(2, 4)]);
}

#[test]
fn run_folds_values_of_its_jobs() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    let (_c, cp, cd) = pudo("c");
    let route_ctx = route_with(None, vec![ap, bp, ad, bd, cp, cd]);

    let runs = get_tour_runs(&route_ctx.route().tour, |ids: &mut Vec<String>, job| {
        ids.push(job.dimens().get_job_id().unwrap().clone())
    });
    let ids = runs.into_iter().map(|run| run.value).collect::<Vec<_>>();

    assert_eq!(ids, vec![vec!["a".to_string(), "b".to_string()], vec!["c".to_string()]]);
}

#[test]
fn insertion_span_is_none_until_target_completes_multi_job() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, _) = pudo("b");
    let route_ctx = route_with(None, vec![ap, ad]);

    assert_eq!(insertion_span(&route_ctx, &bp, 1), None);
    assert_eq!(insertion_span(&route_ctx, &service(3), 1), None);
}

#[test]
fn insertion_span_covers_activities_between_placed_pickup_and_target() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    // [start, bp, ap, ad, end]: delivering b after ad keeps it on board over activities 1..=3.
    let route_ctx = route_with(None, vec![bp, ap, ad]);

    assert_eq!(insertion_span(&route_ctx, &bd, 3), Some((1, 3)));
}

#[test]
fn insertion_span_covers_activities_between_target_and_placed_delivery() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    // [start, ap, ad, bd, end]: picking b up after ap keeps it on board over activities 2..=3.
    let route_ctx = route_with(None, vec![ap, ad, bd]);

    assert_eq!(insertion_span(&route_ctx, &bp, 1), Some((2, 3)));
}

#[test]
fn route_run_cost_is_none_without_per_run_cost() {
    let (_a, ap, ad) = pudo("a");
    let mut route_ctx = route_with(None, vec![ap, ad]);
    update_route_runs(&mut route_ctx);

    assert!(route_ctx.state().get_route_runs().is_none());
    assert_eq!(get_route_run_cost(&route_ctx), None);
}

#[test]
fn route_run_cost_charges_every_run() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    let mut route_ctx = route_with(Some(100.), vec![ap, ad, bp, bd]);

    assert_eq!(get_route_run_cost(&route_ctx), Some(200.));
    update_route_runs(&mut route_ctx);
    assert_eq!(route_ctx.state().get_route_runs().map(Vec::len), Some(2));
    assert_eq!(get_route_run_cost(&route_ctx), Some(200.));
}

#[test]
fn total_cost_includes_run_fees() {
    let (_a, ap, ad) = pudo("a");
    let (_b, bp, bd) = pudo("b");
    let with_fee = |per_run| {
        let mut route_ctx = route_with(per_run, vec![ap.deep_copy(), ad.deep_copy(), bp.deep_copy(), bd.deep_copy()]);
        route_ctx.state_mut().set_total_distance(10.);
        route_ctx.state_mut().set_total_duration(20.);
        TestInsertionContextBuilder::default().with_routes(vec![route_ctx]).build().get_total_cost().unwrap()
    };

    // vehicle and driver both cost 1 per distance and time unit
    assert_eq!(with_fee(None), 60.);
    assert_eq!(with_fee(Some(100.)), 260.);
}
