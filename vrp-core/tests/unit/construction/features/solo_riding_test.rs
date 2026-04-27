use super::*;
use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::TestSingleBuilder;
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::Demand;
use crate::models::problem::Multi;
use crate::models::solution::Activity;
use std::sync::Arc;

const SOLO_RIDING_VIOLATION_CODE: ViolationCode = ViolationCode(1300);

fn create_feature() -> Feature {
    create_solo_riding_feature("solo_riding", SOLO_RIDING_VIOLATION_CODE).unwrap()
}

fn create_pudo_job_activities(
    job_id: &str,
    solo_riding: bool,
    pickup_loc: usize,
    delivery_loc: usize,
) -> (Arc<Multi>, Activity, Activity) {
    let mut pickup_builder = TestSingleBuilder::default();
    pickup_builder.location(Some(pickup_loc)).demand(Demand::pudo_pickup(1));
    let pickup = pickup_builder.build_shared();

    let mut delivery_builder = TestSingleBuilder::default();
    delivery_builder.location(Some(delivery_loc)).demand(Demand::pudo_delivery(1));
    let delivery = delivery_builder.build_shared();

    let mut dimens = Dimensions::default();
    dimens.set_job_id(job_id.to_string());
    if solo_riding {
        dimens.set_job_solo_riding(true);
    }

    let multi = Multi::new_shared(vec![pickup, delivery], dimens);

    let pickup_activity = ActivityBuilder::with_location(pickup_loc).job(Some(multi.jobs[0].clone())).build();
    let delivery_activity = ActivityBuilder::with_location(delivery_loc).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup_activity, delivery_activity)
}

fn create_regular_activity(location: usize) -> Activity {
    ActivityBuilder::with_location(location)
        .job(Some(TestSingleBuilder::default().location(Some(location)).build_shared()))
        .build()
}

fn create_single_job(job_id: &str, solo_riding: bool) -> Job {
    let mut builder = TestSingleBuilder::default();
    builder.id(job_id);
    if solo_riding {
        builder.dimens_mut().set_job_solo_riding(true);
    }

    Job::Single(builder.build_shared())
}

fn evaluate_insertion(
    route_ctx: &RouteContext,
    target: &Activity,
    insertion_idx: usize,
    prev_idx: usize,
    next_idx: Option<usize>,
) -> Option<ConstraintViolation> {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let prev = route_ctx.route().tour.get(prev_idx).unwrap();
    let next = next_idx.and_then(|idx| route_ctx.route().tour.get(idx));

    let activity_ctx = ActivityContext { index: insertion_idx, prev, target, next };
    create_feature().constraint.unwrap().evaluate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
}

#[test]
fn can_create_solo_riding_feature() {
    assert_eq!(create_feature().name, "solo_riding");
}

#[test]
fn can_use_solo_riding_dimension() {
    let mut builder = TestSingleBuilder::default();
    builder.dimens_mut().set_job_solo_riding(true);
    let single = builder.build();

    assert_eq!(single.dimens.get_job_solo_riding(), Some(&true));
}

#[test]
fn rejects_non_solo_pickup_when_solo_job_is_onboard() {
    let (_solo, solo_pickup, _) = create_pudo_job_activities("solo", true, 1, 10);
    let (_other, other_pickup, _) = create_pudo_job_activities("other", false, 2, 11);

    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::with_default_vehicle().add_activity(solo_pickup).build())
        .build();

    // Tour: [start(0), solo_pickup(1), end(2)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, 2, 1, Some(2));

    assert!(result.is_some());
    assert_eq!(result.unwrap().code, SOLO_RIDING_VIOLATION_CODE);
}

#[test]
fn rejects_solo_pickup_when_other_job_is_onboard() {
    let (_solo, solo_pickup, _) = create_pudo_job_activities("solo", true, 1, 10);
    let (_other, other_pickup, _) = create_pudo_job_activities("other", false, 2, 11);

    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::with_default_vehicle().add_activity(other_pickup).build())
        .build();

    // Tour: [start(0), other_pickup(1), end(2)].
    let result = evaluate_insertion(&route_ctx, &solo_pickup, 2, 1, Some(2));

    assert!(result.is_some());
    assert_eq!(result.unwrap().code, SOLO_RIDING_VIOLATION_CODE);
}

#[test]
fn allows_other_job_after_solo_job_is_completed() {
    let (_solo, solo_pickup, solo_delivery) = create_pudo_job_activities("solo", true, 1, 10);
    let (_other, other_pickup, _) = create_pudo_job_activities("other", false, 2, 11);

    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::with_default_vehicle().add_activity(solo_pickup).add_activity(solo_delivery).build())
        .build();

    // Tour: [start(0), solo_pickup(1), solo_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, 3, 2, Some(3));

    assert!(result.is_none());
}

#[test]
fn allows_other_job_after_solo_job_is_completed_real_leg_index() {
    // Mirrors `allows_other_job_after_solo_job_is_completed` but uses the real evaluator
    // convention where `activity_ctx.index` equals `prev`'s index in the tour
    // (see `ride_duration.rs` for documentation of this convention).
    let (_solo, solo_pickup, solo_delivery) = create_pudo_job_activities("solo", true, 1, 10);
    let (_other, other_pickup, _) = create_pudo_job_activities("other", false, 2, 11);

    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::with_default_vehicle().add_activity(solo_pickup).add_activity(solo_delivery).build())
        .build();

    // Tour: [start(0), solo_pickup(1), solo_delivery(2), end(3)].
    // Real evaluator passes leg index = prev's index = 2 when inserting between sd and end.
    let result = evaluate_insertion(&route_ctx, &other_pickup, 2, 2, Some(3));

    assert!(result.is_none(), "should allow other job after solo job is completed");
}

#[test]
fn allows_solo_job_after_other_job_is_completed() {
    let (_solo, solo_pickup, _) = create_pudo_job_activities("solo", true, 1, 10);
    let (_other, other_pickup, other_delivery) = create_pudo_job_activities("other", false, 2, 11);

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::with_default_vehicle().add_activity(other_pickup).add_activity(other_delivery).build(),
        )
        .build();

    // Tour: [start(0), other_pickup(1), other_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &solo_pickup, 3, 2, Some(3));

    assert!(result.is_none());
}

#[test]
fn allows_solo_job_after_multiple_other_jobs_are_completed() {
    let (_solo, solo_pickup, _) = create_pudo_job_activities("solo", true, 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job_activities("first", false, 2, 11);
    let (_second, second_pickup, second_delivery) = create_pudo_job_activities("second", false, 3, 12);

    let route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::with_default_vehicle()
                .add_activity(first_pickup)
                .add_activity(second_pickup)
                .add_activity(first_delivery)
                .add_activity(second_delivery)
                .build(),
        )
        .build();

    // Tour: [start(0), p1(1), p2(2), d1(3), d2(4), end(5)].
    let result = evaluate_insertion(&route_ctx, &solo_pickup, 5, 4, Some(5));

    assert!(result.is_none());
}

#[test]
fn allows_regular_non_dynamic_activity_while_solo_job_is_onboard() {
    let (_solo, solo_pickup, _) = create_pudo_job_activities("solo", true, 1, 10);
    let regular = create_regular_activity(5);

    let route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::with_default_vehicle().add_activity(solo_pickup).build())
        .build();

    // Tour: [start(0), solo_pickup(1), end(2)].
    let result = evaluate_insertion(&route_ctx, &regular, 2, 1, Some(2));

    assert!(result.is_none());
}

parameterized_test! {can_merge_jobs, (source_solo, candidate_solo, expected), {
    can_merge_jobs_impl(source_solo, candidate_solo, expected);
}}

can_merge_jobs! {
    case_01: (false, false, Ok(())),
    case_02: (true, false, Err(SOLO_RIDING_VIOLATION_CODE)),
    case_03: (false, true, Err(SOLO_RIDING_VIOLATION_CODE)),
    case_04: (true, true, Err(SOLO_RIDING_VIOLATION_CODE)),
}

fn can_merge_jobs_impl(source_solo: bool, candidate_solo: bool, expected: Result<(), ViolationCode>) {
    let source = create_single_job("source", source_solo);
    let candidate = create_single_job("candidate", candidate_solo);
    let constraint = create_feature().constraint.unwrap();

    let result = constraint.merge(source, candidate).map(|_| ());

    assert_eq!(result, expected);
}
