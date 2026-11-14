use super::*;
use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::TestSingleBuilder;
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::Demand;
use crate::models::problem::Multi;
use crate::models::solution::Activity;
use std::sync::Arc;

const SHARING_CODE: ViolationCode = ViolationCode(1500);

fn create_feature() -> Feature {
    create_job_sharing_feature("job_sharing", SHARING_CODE).unwrap()
}

fn sharing(labels: &[&str], run_none_of: &[&str], tour_none_of: &[&str]) -> Option<JobSharing> {
    let to_vec = |labels: &[&str]| labels.iter().map(|label| label.to_string()).collect();

    Some(JobSharing { labels: to_vec(labels), run_none_of: to_vec(run_none_of), tour_none_of: to_vec(tour_none_of) })
}

/// Creates pickup and delivery activities of one pickup-delivery job with optional sharing.
/// The job is returned too: activities reach it through a weak link, so callers keep it alive.
fn pudo(id: &str, sharing: Option<JobSharing>) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup = TestSingleBuilder::default();
    pickup.location(Some(1)).demand(Demand::pudo_pickup(1));
    let mut delivery = TestSingleBuilder::default();
    delivery.location(Some(10)).demand(Demand::pudo_delivery(1));

    let mut dimens = Dimensions::default();
    dimens.set_job_id(id.to_string());
    if let Some(sharing) = sharing {
        dimens.set_job_sharing(sharing);
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

/// Evaluates inserting `target` after the tour activity at `leg_index`.
fn evaluate_activity(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Option<ViolationCode> {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let tour = &route_ctx.route().tour;
    let activity_ctx =
        ActivityContext { index: leg_index, prev: tour.get(leg_index).unwrap(), target, next: tour.get(leg_index + 1) };

    create_feature()
        .constraint
        .unwrap()
        .evaluate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
        .map(|violation| violation.code)
}

fn evaluate_route(route_ctx: &RouteContext, job: &Job) -> Option<ViolationCode> {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;

    create_feature()
        .constraint
        .unwrap()
        .evaluate(&MoveContext::route(&solution_ctx, route_ctx, job))
        .map(|violation| violation.code)
}

#[test]
fn allows_conflicting_labels_in_different_runs_of_one_tour() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    // b's pickup is already placed (as in the evaluator's shadow route): [start, ap, ad, bp, end].
    let route_ctx = route_with(vec![ap, ad, bp]);

    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(b)), None);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), None);
}

#[test]
fn rejects_conflicting_labels_in_one_run() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    // [start, ap, bp, ad, end]: b is picked up while a is on board.
    let route_ctx = route_with(vec![ap, bp, ad]);

    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), Some(SHARING_CODE));
}

#[test]
fn rejects_delivery_which_pulls_job_into_conflicting_run() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    // [start, bp, ap, ad, end]: delivering b after ap keeps it on board when a is picked up.
    let route_ctx = route_with(vec![bp, ap, ad]);

    assert_eq!(evaluate_activity(&route_ctx, &bd, 2), Some(SHARING_CODE));
    assert_eq!(evaluate_activity(&route_ctx, &bd, 1), None);
}

#[test]
fn rejects_conflict_stated_by_either_job_only() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    let route_ctx = route_with(vec![bp, ap, ad]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 2), Some(SHARING_CODE), "stated by the job in the tour");

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &["x"], &[]));
    let route_ctx = route_with(vec![bp, ap, ad]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 2), Some(SHARING_CODE), "stated by the inserted job");
}

#[test]
fn rejects_job_with_two_labels_conflicting_through_either_one() {
    for label in ["x", "y"] {
        let (_a, ap, ad) = pudo("a", sharing(&["z"], &[label], &[label]));
        let (b, bp, bd) = pudo("b", sharing(&["x", "y"], &[], &[]));
        assert_eq!(evaluate_activity(&route_with(vec![ap.clone(), bp, ad.clone()]), &bd, 3), Some(SHARING_CODE));
        assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));

        let (_a, ap, ad) = pudo("a", sharing(&["x", "y"], &[], &[]));
        let (b, bp, bd) = pudo("b", sharing(&["z"], &[label], &[label]));
        assert_eq!(evaluate_activity(&route_with(vec![ap.clone(), bp, ad.clone()]), &bd, 3), Some(SHARING_CODE));
        assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));
    }
}

#[test]
fn rejects_label_refused_by_a_job_without_labels() {
    let (_a, ap, ad) = pudo("a", sharing(&[], &["x"], &["x"]));
    let (b, bp, bd) = pudo("b", sharing(&["x"], &[], &[]));
    assert_eq!(evaluate_activity(&route_with(vec![ap.clone(), bp, ad.clone()]), &bd, 3), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &[]));
    let (b, bp, bd) = pudo("b", sharing(&[], &["x"], &["x"]));
    assert_eq!(evaluate_activity(&route_with(vec![ap.clone(), bp, ad.clone()]), &bd, 3), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));

    let (_a, ap, ad) = pudo("a", sharing(&[], &["y"], &["y"]));
    let (b, bp, bd) = pudo("b", sharing(&["x"], &[], &[]));
    assert_eq!(evaluate_activity(&route_with(vec![ap.clone(), bp, ad.clone()]), &bd, 3), None);
    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), None);
}

#[test]
fn rejects_job_without_sharing_which_merges_conflicting_runs() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (_c, cp, cd) = pudo("c", sharing(&["y"], &[], &[]));
    let (_n, np, nd) = pudo("n", None);
    // [start, ap, np, ad, cp, cd, end]: n is picked up during a's ride.
    let route_ctx = route_with(vec![ap, np, ad, cp, cd]);

    assert_eq!(evaluate_activity(&route_ctx, &nd, 4), Some(SHARING_CODE), "delivered during c's ride");
    assert_eq!(evaluate_activity(&route_ctx, &nd, 3), None, "delivered before c is picked up");
}

#[test]
fn rejects_conflicting_labels_in_one_tour_even_in_different_runs() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &["y"]));
    let (b, _, _) = pudo("b", sharing(&["y"], &[], &[]));
    let route_ctx = route_with(vec![ap, ad]);

    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(b)), Some(SHARING_CODE));
}

#[test]
fn rejects_tour_conflict_stated_by_the_inserted_job_only() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &[]));
    let (b, _, _) = pudo("b", sharing(&["y"], &[], &["x"]));
    let route_ctx = route_with(vec![ap, ad]);

    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(b)), Some(SHARING_CODE));
}

#[test]
fn allows_tour_conflicting_labels_in_different_tours() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &["y"]));
    let (_c, cp, cd) = pudo("c", sharing(&["z"], &[], &[]));
    let (b, _, _) = pudo("b", sharing(&["y"], &[], &[]));

    assert_eq!(evaluate_route(&route_with(vec![]), &Job::Multi(b.clone())), None);
    assert_eq!(evaluate_route(&route_with(vec![cp, cd]), &Job::Multi(b.clone())), None);
    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));
}

#[test]
fn applies_tour_sharing_to_a_single_job() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &["y"]));
    let mut single = TestSingleBuilder::default();
    single.location(Some(5)).dimens_mut().set_job_sharing(sharing(&["y"], &[], &[]).unwrap());
    let single = single.build_as_job_ref();

    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &single), Some(SHARING_CODE));
}

#[test]
fn ignores_run_sharing_of_a_single_job() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let mut single = TestSingleBuilder::default();
    single.location(Some(5)).dimens_mut().set_job_sharing(sharing(&["y"], &[], &[]).unwrap());
    let single = single.build_shared();
    let target = ActivityBuilder::with_location(5).job(Some(single)).build();
    let route_ctx = route_with(vec![ap, ad]);

    assert_eq!(evaluate_activity(&route_ctx, &target, 1), None);
}

#[test]
fn rejects_two_jobs_with_a_label_one_of_them_lists() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["x"], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["x"], &[], &[]));
    let route_ctx = route_with(vec![ap, bp, ad]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), Some(SHARING_CODE));

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["x"], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["x"], &[], &[]));
    let route_ctx = route_with(vec![ap, ad, bp]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), None);

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &[], &["x"]));
    let (b, _, _) = pudo("b", sharing(&["x"], &[], &[]));
    assert_eq!(evaluate_route(&route_with(vec![ap, ad]), &Job::Multi(b)), Some(SHARING_CODE));
}

#[test]
fn allows_job_alone_when_it_lists_its_own_label() {
    let (a, ap, ad) = pudo("a", sharing(&["x"], &["x"], &["x"]));
    let route_ctx = route_with(vec![ap]);

    assert_eq!(evaluate_route(&route_with(vec![]), &Job::Multi(a)), None);
    assert_eq!(evaluate_activity(&route_ctx, &ad, 1), None);
}

#[test]
fn allows_jobs_without_sharing_and_non_conflicting_labels() {
    let (_a, ap, ad) = pudo("a", None);
    let (b, bp, bd) = pudo("b", None);
    let route_ctx = route_with(vec![ap, bp, ad]);
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(b)), None);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), None);

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &["z"]));
    let (b, bp, bd) = pudo("b", sharing(&["w"], &["v"], &["v"]));
    let route_ctx = route_with(vec![ap, bp, ad]);
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(b)), None);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), None);

    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &[]));
    let (_n, np, nd) = pudo("n", None);
    let route_ctx = route_with(vec![ap, np, ad]);
    assert_eq!(evaluate_activity(&route_ctx, &nd, 3), None);
}

#[test]
fn allows_insertion_once_the_conflicting_job_is_removed() {
    let (a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &["z"]));
    let (_c, cp, cd) = pudo("c", sharing(&["w"], &[], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    let (d, _, _) = pudo("d", sharing(&["z"], &[], &[]));
    let feature = create_feature();
    let mut route_ctx = route_with(vec![cp, ap, bp, ad, cd]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 4), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d.clone())), Some(SHARING_CODE));

    assert!(route_ctx.route_mut().tour.remove(&Job::Multi(a)));
    feature.state.as_ref().unwrap().accept_route_state(&mut route_ctx);

    // [start, cp, bp, cd, end]
    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), None);
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d)), None);
}

#[test]
fn keeps_a_label_after_removing_one_of_two_jobs_carrying_it() {
    let (a, ap, ad) = pudo("a", sharing(&["x"], &[], &[]));
    let (c, cp, cd) = pudo("c", sharing(&["x"], &[], &[]));
    let (_b, bp, bd) = pudo("b", sharing(&[], &["x"], &[]));
    let (d, _, _) = pudo("d", sharing(&[], &[], &["x"]));
    let feature = create_feature();
    // [start, ap, cp, bp, ad, cd, end]: a and c form one run, b is picked up inside it.
    let mut route_ctx = route_with(vec![ap, cp, bp, ad, cd]);
    assert_eq!(evaluate_activity(&route_ctx, &bd, 4), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d.clone())), Some(SHARING_CODE));

    assert!(route_ctx.route_mut().tour.remove(&Job::Multi(a)));
    feature.state.as_ref().unwrap().accept_route_state(&mut route_ctx);

    // [start, cp, bp, cd, end]: c still carries the label.
    assert_eq!(evaluate_activity(&route_ctx, &bd, 2), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d.clone())), Some(SHARING_CODE));

    assert!(route_ctx.route_mut().tour.remove(&Job::Multi(c)));
    feature.state.as_ref().unwrap().accept_route_state(&mut route_ctx);

    // [start, bp, end]
    assert_eq!(evaluate_activity(&route_ctx, &bd, 1), None);
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d)), None);
}

#[test]
fn evaluates_without_route_state() {
    let (_a, ap, ad) = pudo("a", sharing(&["x"], &["y"], &["z"]));
    let (_b, bp, bd) = pudo("b", sharing(&["y"], &[], &[]));
    let (d, _, _) = pudo("d", sharing(&["z"], &[], &[]));
    let mut builder = RouteBuilder::with_default_vehicle();
    builder.add_activities(vec![ap, bp, ad]);
    let route_ctx = RouteContextBuilder::default().with_route(builder.build()).build();

    assert_eq!(evaluate_activity(&route_ctx, &bd, 3), Some(SHARING_CODE));
    assert_eq!(evaluate_route(&route_ctx, &Job::Multi(d)), Some(SHARING_CODE));
}

#[test]
fn refuses_to_merge_jobs_with_sharing() {
    let constraint = create_feature().constraint.unwrap();
    let with = || {
        let mut single = TestSingleBuilder::default();
        single.dimens_mut().set_job_sharing(sharing(&["x"], &[], &[]).unwrap());
        single.build_as_job_ref()
    };
    let without = || TestSingleBuilder::default().build_as_job_ref();

    assert_eq!(constraint.merge(with(), without()).err(), Some(SHARING_CODE));
    assert_eq!(constraint.merge(without(), with()).err(), Some(SHARING_CODE));
    assert!(constraint.merge(without(), without()).is_ok());
}
