use super::*;
use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::TestSingleBuilder;
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::Demand;
use crate::models::problem::Multi;
use crate::models::solution::Activity;
use std::sync::Arc;

fn create_feature() -> Feature {
    create_site_mixing_feature("site_mixing").unwrap()
}

/// Creates pickup and delivery activities of one pickup-delivery job with an optional site.
/// The job is returned too: activities reach it through a weak link, so callers keep it alive.
fn pudo(id: &str, site: Option<u64>) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup = TestSingleBuilder::default();
    pickup.location(Some(1)).demand(Demand::pudo_pickup(1));
    let mut delivery = TestSingleBuilder::default();
    delivery.location(Some(10)).demand(Demand::pudo_delivery(1));

    let mut dimens = Dimensions::default();
    dimens.set_job_id(id.to_string());
    if let Some(site) = site {
        dimens.set_job_site(site);
    }
    let multi = Multi::new_shared(vec![pickup.build_shared(), delivery.build_shared()], dimens);

    let pickup = ActivityBuilder::with_location(1).job(Some(multi.jobs[0].clone())).build();
    let delivery = ActivityBuilder::with_location(10).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup, delivery)
}

/// Builds a route with given activities (after the start) and accepts the feature's route state.
fn route_with(activities: Vec<Activity>) -> RouteContext {
    let mut builder = RouteBuilder::with_default_vehicle();
    for activity in activities {
        builder.add_activity(activity);
    }
    let mut route_ctx = RouteContextBuilder::default().with_route(builder.build()).build();
    create_feature().state.unwrap().accept_route_state(&mut route_ctx);

    route_ctx
}

fn fitness_of(routes: Vec<RouteContext>) -> Cost {
    let insertion_ctx = TestInsertionContextBuilder::default().with_routes(routes).build();
    create_feature().objective.unwrap().fitness(&insertion_ctx)
}

/// Estimates inserting `target` after the tour activity at `leg_index`.
fn estimate(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Cost {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let tour = &route_ctx.route().tour;
    let activity_ctx =
        ActivityContext { index: leg_index, prev: tour.get(leg_index).unwrap(), target, next: tour.get(leg_index + 1) };

    create_feature().objective.unwrap().estimate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
}

#[test]
fn fitness_is_zero_when_run_serves_one_site() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(1));

    assert_eq!(fitness_of(vec![route_with(vec![ap, bp, ad, bd])]), 0.);
}

#[test]
fn fitness_counts_each_extra_site_in_a_run() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));
    let (_c, cp, cd) = pudo("c", Some(3));

    assert_eq!(fitness_of(vec![route_with(vec![ap, bp, cp, ad, bd, cd])]), 2.);
}

#[test]
fn fitness_ignores_sites_in_separate_runs() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));

    assert_eq!(fitness_of(vec![route_with(vec![ap, ad, bp, bd])]), 0.);
}

#[test]
fn job_without_site_bridges_runs_without_adding_a_site() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_x, xp, xd) = pudo("x", None);
    let (_b, bp, bd) = pudo("b", Some(2));

    // x is on board from before a's delivery until after b's pickup: one run holding sites 1 and 2.
    assert_eq!(fitness_of(vec![route_with(vec![ap, xp, ad, bp, xd, bd])]), 1.);
}

#[test]
fn fitness_sums_over_routes() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));
    let (_c, cp, cd) = pudo("c", Some(1));
    let (_d, dp, dd) = pudo("d", Some(3));

    let routes = vec![route_with(vec![ap, bp, ad, bd]), route_with(vec![cp, dp, cd, dd])];

    assert_eq!(fitness_of(routes), 2.);
}

#[test]
fn estimate_charges_delivery_that_completes_job_in_run_with_other_site() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));
    // b's pickup is already placed (as in the evaluator's shadow route): [start, ap, bp, ad, end].
    let route_ctx = route_with(vec![ap, bp, ad]);

    assert_eq!(estimate(&route_ctx, &bd, 3), 1.);
}

#[test]
fn estimate_is_zero_when_job_joins_run_with_same_site() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(1));
    let route_ctx = route_with(vec![ap, bp, ad]);

    assert_eq!(estimate(&route_ctx, &bd, 3), 0.);
}

#[test]
fn estimate_is_zero_for_job_completed_while_vehicle_is_otherwise_empty() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));
    // [start, ap, ad, bp, end]: b rides alone after a is delivered.
    let route_ctx = route_with(vec![ap, ad, bp]);

    assert_eq!(estimate(&route_ctx, &bd, 3), 0.);
}

#[test]
fn estimate_charges_job_that_bridges_two_runs() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_c, cp, cd) = pudo("c", Some(2));
    let (_b, bp, bd) = pudo("b", Some(1));
    // [start, ap, bp, ad, cp, cd, end]: delivering b after cp keeps the vehicle loaded between both runs.
    let route_ctx = route_with(vec![ap, bp, ad, cp, cd]);

    assert_eq!(estimate(&route_ctx, &bd, 4), 1.);
}

#[test]
fn estimate_is_zero_for_first_activity_of_job() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, _) = pudo("b", Some(2));
    let route_ctx = route_with(vec![ap, ad]);

    assert_eq!(estimate(&route_ctx, &bp, 1), 0.);
}

#[test]
fn estimate_matches_fitness_change_after_insertion() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_c, cp, cd) = pudo("c", Some(2));
    let (_b, bp, bd) = pudo("b", Some(3));

    let before = fitness_of(vec![route_with(vec![ap.deep_copy(), ad.deep_copy(), cp.deep_copy(), cd.deep_copy()])]);
    let shadow = route_with(vec![ap.deep_copy(), bp.deep_copy(), ad.deep_copy(), cp.deep_copy(), cd.deep_copy()]);
    let delta = estimate(&shadow, &bd, 4);
    let after = fitness_of(vec![route_with(vec![ap, bp, ad, cp, bd, cd])]);

    assert_eq!(after - before, delta);
}

fn create_visits_feature() -> Feature {
    create_site_visits_feature("site_visits").unwrap()
}

fn visits_fitness_of(routes: Vec<RouteContext>) -> Cost {
    let insertion_ctx = TestInsertionContextBuilder::default().with_routes(routes).build();
    create_visits_feature().objective.unwrap().fitness(&insertion_ctx)
}

fn visits_estimate(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Cost {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let tour = &route_ctx.route().tour;
    let activity_ctx =
        ActivityContext { index: leg_index, prev: tour.get(leg_index).unwrap(), target, next: tour.get(leg_index + 1) };

    create_visits_feature().objective.unwrap().estimate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
}

#[test]
fn visits_fitness_counts_every_site_of_every_run() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(1));
    let (_c, cp, cd) = pudo("c", Some(2));
    let (_d, dp, dd) = pudo("d", Some(1));

    // runs: {a, b} -> site 1, then {c, d} -> sites 2 and 1
    assert_eq!(visits_fitness_of(vec![route_with(vec![ap, bp, ad, bd, cp, dp, cd, dd])]), 3.);
}

#[test]
fn visits_estimate_charges_job_that_opens_a_new_run() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(1));
    let route_ctx = route_with(vec![ap, ad, bp]);

    assert_eq!(visits_estimate(&route_ctx, &bd, 3), 1.);
}

#[test]
fn visits_estimate_is_zero_when_job_joins_run_with_same_site() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(1));
    let route_ctx = route_with(vec![ap, bp, ad]);

    assert_eq!(visits_estimate(&route_ctx, &bd, 3), 0.);
}

#[test]
fn visits_estimate_matches_fitness_change_when_bridging_runs() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_c, cp, cd) = pudo("c", Some(2));
    let (_b, bp, bd) = pudo("b", Some(1));

    let before =
        visits_fitness_of(vec![route_with(vec![ap.deep_copy(), ad.deep_copy(), cp.deep_copy(), cd.deep_copy()])]);
    let shadow = route_with(vec![ap.deep_copy(), bp.deep_copy(), ad.deep_copy(), cp.deep_copy(), cd.deep_copy()]);
    let delta = visits_estimate(&shadow, &bd, 4);
    let after = visits_fitness_of(vec![route_with(vec![ap, bp, ad, cp, bd, cd])]);

    assert_eq!(after - before, delta);
}

#[test]
fn site_mixing_and_site_visits_can_share_route_state() {
    let (_a, ap, ad) = pudo("a", Some(1));
    let (_b, bp, bd) = pudo("b", Some(2));
    let mut route_ctx = route_with(vec![ap, bp, ad, bd]);
    create_visits_feature().state.unwrap().accept_route_state(&mut route_ctx);
    create_feature().state.unwrap().accept_route_state(&mut route_ctx);
    let insertion_ctx = TestInsertionContextBuilder::default().with_routes(vec![route_ctx]).build();

    assert_eq!(create_feature().objective.unwrap().fitness(&insertion_ctx), 1.);
    assert_eq!(create_visits_feature().objective.unwrap().fitness(&insertion_ctx), 2.);
}
