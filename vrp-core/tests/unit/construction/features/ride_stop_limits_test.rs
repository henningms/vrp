use super::*;
use crate::construction::heuristics::{ActivityContext, MoveContext};
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::TestSingleBuilder;
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::common::{Demand, SingleDimLoad};
use crate::models::problem::Multi;
use crate::models::solution::Activity;
use std::sync::Arc;

const RIDE_STOP_LIMITS_CODE: ViolationCode = ViolationCode(1400);

fn create_feature() -> Feature {
    create_ride_stop_limits_feature("ride_stop_limits", RIDE_STOP_LIMITS_CODE).unwrap()
}

fn limits(pickups: Option<usize>, deliveries: Option<usize>, stops: Option<usize>) -> Option<RideStopLimits> {
    Some(RideStopLimits { pickups, deliveries, stops })
}

fn create_pudo_job(
    job_id: &str,
    limits: Option<RideStopLimits>,
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
    if let Some(limits) = limits {
        dimens.set_job_ride_stop_limits(limits);
    }

    let multi = Multi::new_shared(vec![pickup, delivery], dimens);

    let pickup_activity = ActivityBuilder::with_location(pickup_loc).job(Some(multi.jobs[0].clone())).build();
    let delivery_activity = ActivityBuilder::with_location(delivery_loc).job(Some(multi.jobs[1].clone())).build();

    (multi, pickup_activity, delivery_activity)
}

/// A job activity without demand, which is how breaks and reloads look to the constraint.
fn create_break_activity(location: usize) -> Activity {
    ActivityBuilder::with_location(location)
        .job(Some(TestSingleBuilder::default().location(Some(location)).build_shared()))
        .build()
}

fn create_route_ctx(activities: Vec<Activity>) -> RouteContext {
    let mut route = RouteBuilder::with_default_vehicle();
    activities.into_iter().for_each(|activity| {
        route.add_activity(activity);
    });

    RouteContextBuilder::default().with_route(route.build()).build()
}

/// Evaluates inserting `target` right after the tour activity at `prev_idx`.
fn evaluate_insertion(route_ctx: &RouteContext, target: &Activity, prev_idx: usize) -> Option<ConstraintViolation> {
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let prev = route_ctx.route().tour.get(prev_idx).unwrap();
    let next = route_ctx.route().tour.get(prev_idx + 1);

    let activity_ctx = ActivityContext { index: prev_idx, prev, target, next };
    create_feature().constraint.unwrap().evaluate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
}

fn violation_code(result: Option<ConstraintViolation>) -> Option<ViolationCode> {
    result.map(|violation| violation.code)
}

#[test]
fn can_create_ride_stop_limits_feature() {
    assert_eq!(create_feature().name, "ride_stop_limits");
}

#[test]
fn can_use_ride_stop_limits_dimension() {
    let mut dimens = Dimensions::default();
    dimens.set_job_ride_stop_limits(RideStopLimits { pickups: Some(0), deliveries: None, stops: Some(2) });

    let value = dimens.get_job_ride_stop_limits().unwrap();

    assert_eq!((value.pickups, value.deliveries, value.stops), (Some(0), None, Some(2)));
}

#[test]
fn refuses_another_pickup_inside_a_ride_limited_to_zero_pickups() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(Some(0), None, None), 1, 10);
    let (_other, other_pickup, _) = create_pudo_job("other", None, 2, 11);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, 1);

    assert_eq!(violation_code(result), Some(RIDE_STOP_LIMITS_CODE));
}

#[test]
fn allows_another_pickup_at_the_same_location_as_the_limited_jobs_pickup() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(Some(0), None, None), 1, 10);
    let (_other, other_pickup, _) = create_pudo_job("other", None, 1, 11);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, 1);

    assert_eq!(violation_code(result), None);
}

#[test]
fn allows_a_delivery_only_stop_inside_a_ride_limited_to_zero_pickups() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(Some(0), None, None), 1, 10);
    let (_other, other_pickup, other_delivery) = create_pudo_job("other", None, 2, 3);
    let route_ctx = create_route_ctx(vec![other_pickup, limited_pickup, limited_delivery]);

    // Tour: [start(0), other_pickup(1), limited_pickup(2), limited_delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &other_delivery, 2);

    assert_eq!(violation_code(result), None);
}

#[test]
fn refuses_a_delivery_only_stop_inside_a_ride_limited_to_zero_deliveries() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(0), None), 1, 10);
    let (_other, other_pickup, other_delivery) = create_pudo_job("other", None, 2, 3);
    let route_ctx = create_route_ctx(vec![other_pickup, limited_pickup, limited_delivery]);

    // Tour: [start(0), other_pickup(1), limited_pickup(2), limited_delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &other_delivery, 2);

    assert_eq!(violation_code(result), Some(RIDE_STOP_LIMITS_CODE));
}

parameterized_test! {refuses_the_limited_jobs_delivery_after_more_delivery_stops_than_allowed, (prev_idx, expected), {
    refuses_the_limited_jobs_delivery_after_more_delivery_stops_than_allowed_impl(prev_idx, expected);
}}

refuses_the_limited_jobs_delivery_after_more_delivery_stops_than_allowed! {
    case_01_after_one_delivery_stop: (4, None),
    case_02_after_two_delivery_stops: (5, Some(RIDE_STOP_LIMITS_CODE)),
}

fn refuses_the_limited_jobs_delivery_after_more_delivery_stops_than_allowed_impl(
    prev_idx: usize,
    expected: Option<ViolationCode>,
) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(1), None), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 4);
    let (_second, second_pickup, second_delivery) = create_pudo_job("second", None, 3, 5);
    let route_ctx =
        create_route_ctx(vec![first_pickup, second_pickup, limited_pickup, first_delivery, second_delivery]);

    // Tour: [start(0), first_pickup(1), second_pickup(2), limited_pickup(3), first_delivery(4),
    //        second_delivery(5), end(6)].
    let result = evaluate_insertion(&route_ctx, &limited_delivery, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {counts_two_deliveries_at_one_location_as_one_stop, (deliveries, stops, expected), {
    counts_two_deliveries_at_one_location_as_one_stop_impl(deliveries, stops, expected);
}}

counts_two_deliveries_at_one_location_as_one_stop! {
    case_01_deliveries_one: (Some(1), None, None),
    case_02_stops_one: (None, Some(1), None),
    case_03_deliveries_zero: (Some(0), None, Some(RIDE_STOP_LIMITS_CODE)),
}

fn counts_two_deliveries_at_one_location_as_one_stop_impl(
    deliveries: Option<usize>,
    stops: Option<usize>,
    expected: Option<ViolationCode>,
) {
    let (_limited, limited_pickup, limited_delivery) =
        create_pudo_job("limited", limits(None, deliveries, stops), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 4);
    let (_second, second_pickup, second_delivery) = create_pudo_job("second", None, 3, 4);
    let route_ctx =
        create_route_ctx(vec![first_pickup, second_pickup, limited_pickup, first_delivery, second_delivery]);

    // Tour: [start(0), first_pickup(1), second_pickup(2), limited_pickup(3), first_delivery(4),
    //        second_delivery(5), end(6)].
    let result = evaluate_insertion(&route_ctx, &limited_delivery, 5);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {counts_a_stop_split_by_a_different_location_insertion_as_two, (prev_idx, expected), {
    counts_a_stop_split_by_a_different_location_insertion_as_two_impl(prev_idx, expected);
}}

counts_a_stop_split_by_a_different_location_insertion_as_two! {
    case_01_splits_the_stop: (4, Some(RIDE_STOP_LIMITS_CODE)),
    case_02_after_the_stop: (5, None),
}

fn counts_a_stop_split_by_a_different_location_insertion_as_two_impl(prev_idx: usize, expected: Option<ViolationCode>) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(1), None), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 4);
    let (_second, second_pickup, second_delivery) = create_pudo_job("second", None, 3, 4);
    let (_third, third_pickup, _) = create_pudo_job("third", None, 6, 11);
    let route_ctx = create_route_ctx(vec![
        first_pickup,
        second_pickup,
        limited_pickup,
        first_delivery,
        second_delivery,
        limited_delivery,
    ]);

    // Tour: [start(0), first_pickup(1), second_pickup(2), limited_pickup(3), first_delivery(4),
    //        second_delivery(5), limited_delivery(6), end(7)].
    let result = evaluate_insertion(&route_ctx, &third_pickup, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {ignores_stops_after_the_limited_jobs_delivery, (prev_idx, expected), {
    ignores_stops_after_the_limited_jobs_delivery_impl(prev_idx, expected);
}}

ignores_stops_after_the_limited_jobs_delivery! {
    case_01_inside_the_ride: (1, Some(RIDE_STOP_LIMITS_CODE)),
    case_02_right_after_the_delivery: (2, None),
    case_03_inside_a_later_ride: (3, None),
}

fn ignores_stops_after_the_limited_jobs_delivery_impl(prev_idx: usize, expected: Option<ViolationCode>) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, None, Some(0)), 1, 10);
    let (_later, later_pickup, later_delivery) = create_pudo_job("later", None, 2, 3);
    let (_other, other_pickup, _) = create_pudo_job("other", None, 4, 11);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery, later_pickup, later_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), later_pickup(3), later_delivery(4), end(5)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, prev_idx);

    assert_eq!(violation_code(result), expected);
}

#[test]
fn allows_anything_while_only_the_limited_jobs_pickup_is_in_the_tour() {
    let (_limited, limited_pickup, _) = create_pudo_job("limited", limits(Some(0), Some(0), Some(0)), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 3);
    let (_second, second_pickup, _) = create_pudo_job("second", None, 4, 11);
    let route_ctx = create_route_ctx(vec![limited_pickup, first_pickup, first_delivery]);

    // Tour: [start(0), limited_pickup(1), first_pickup(2), first_delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &second_pickup, 2);

    assert_eq!(violation_code(result), None);
}

#[test]
fn ignores_breaks_between_activities_when_forming_stops() {
    let (_limited, limited_pickup, limited_delivery) =
        create_pudo_job("limited", limits(Some(0), None, Some(0)), 1, 10);
    let (_other, other_pickup, _) = create_pudo_job("other", None, 1, 11);
    let route_ctx = create_route_ctx(vec![limited_pickup, create_break_activity(7), limited_delivery]);

    // Tour: [start(0), limited_pickup(1), break(2), limited_delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, 2);

    assert_eq!(violation_code(result), None);
}

#[test]
fn ignores_an_inserted_break_inside_the_ride() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, None, Some(0)), 1, 10);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &create_break_activity(7), 1);

    assert_eq!(violation_code(result), None);
}

#[test]
fn ignores_breaks_between_deliveries_at_one_location() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(1), None), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 4);
    let (_second, second_pickup, second_delivery) = create_pudo_job("second", None, 3, 4);
    let route_ctx = create_route_ctx(vec![
        first_pickup,
        second_pickup,
        limited_pickup,
        first_delivery,
        create_break_activity(7),
        second_delivery,
    ]);

    // Tour: [start(0), first_pickup(1), second_pickup(2), limited_pickup(3), first_delivery(4), break(5),
    //        second_delivery(6), end(7)].
    let result = evaluate_insertion(&route_ctx, &limited_delivery, 6);

    assert_eq!(violation_code(result), None);
}

#[test]
fn refuses_the_limited_jobs_pickup_before_too_many_stops() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, None, Some(0)), 1, 10);
    let (_other, other_pickup, other_delivery) = create_pudo_job("other", None, 2, 3);
    let route_ctx = create_route_ctx(vec![other_pickup, other_delivery, limited_delivery]);

    // Tour: [start(0), other_pickup(1), other_delivery(2), limited_delivery(3), end(4)].
    let inside = evaluate_insertion(&route_ctx, &limited_pickup, 0);
    let after = evaluate_insertion(&route_ctx, &limited_pickup, 2);

    assert_eq!(violation_code(inside), Some(RIDE_STOP_LIMITS_CODE));
    assert_eq!(violation_code(after), None);
}

/// Creates a job with several pickups and deliveries, all on board together.
fn create_companion_job(
    job_id: &str,
    limits: Option<RideStopLimits>,
    pickup_locs: &[usize],
    delivery_locs: &[usize],
) -> (Arc<Multi>, Vec<Activity>, Vec<Activity>) {
    let pickups = pickup_locs.iter().map(|&location| (location, Demand::pudo_pickup(1)));
    let deliveries = delivery_locs.iter().map(|&location| (location, Demand::pudo_delivery(1)));
    let singles = pickups
        .chain(deliveries)
        .map(|(location, demand)| {
            let mut builder = TestSingleBuilder::default();
            builder.location(Some(location)).demand(demand);
            builder.build_shared()
        })
        .collect::<Vec<_>>();

    let mut dimens = Dimensions::default();
    dimens.set_job_id(job_id.to_string());
    if let Some(limits) = limits {
        dimens.set_job_ride_stop_limits(limits);
    }

    let multi = Multi::new_shared(singles, dimens);
    let mut activities = multi
        .jobs
        .iter()
        .map(|single| {
            ActivityBuilder::with_location(single.places[0].location.unwrap()).job(Some(single.clone())).build()
        })
        .collect::<Vec<_>>();
    let deliveries = activities.split_off(pickup_locs.len());

    (multi, activities, deliveries)
}

fn create_static_activity(location: usize, demand: Demand<SingleDimLoad>) -> Activity {
    let mut builder = TestSingleBuilder::default();
    builder.location(Some(location)).demand(demand);
    ActivityBuilder::with_location(location).job(Some(builder.build_shared())).build()
}

#[test]
fn does_not_count_the_limited_jobs_own_activities() {
    let (_limited, pickups, mut deliveries) =
        create_companion_job("limited", limits(Some(0), Some(0), Some(0)), &[1, 2], &[10]);
    let delivery = deliveries.remove(0);
    let route_ctx = create_route_ctx(pickups);

    // Tour: [start(0), first_pickup(1), second_pickup(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &delivery, 2);

    assert_eq!(violation_code(result), None);
}

#[test]
fn counts_a_static_delivery_job_as_a_delivery_stop() {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(0), None), 1, 10);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &create_static_activity(5, Demand::delivery(1)), 1);

    assert_eq!(violation_code(result), Some(RIDE_STOP_LIMITS_CODE));
}

parameterized_test! {counts_a_static_pickup_job_as_a_pickup_stop, (pickups, expected), {
    counts_a_static_pickup_job_as_a_pickup_stop_impl(pickups, expected);
}}

counts_a_static_pickup_job_as_a_pickup_stop! {
    case_01_zero_pickups: (0, Some(RIDE_STOP_LIMITS_CODE)),
    case_02_one_pickup: (1, None),
}

fn counts_a_static_pickup_job_as_a_pickup_stop_impl(pickups: usize, expected: Option<ViolationCode>) {
    let (_limited, limited_pickup, limited_delivery) =
        create_pudo_job("limited", limits(Some(pickups), None, None), 1, 10);
    let route_ctx = create_route_ctx(vec![limited_pickup, limited_delivery]);

    // Tour: [start(0), limited_pickup(1), limited_delivery(2), end(3)].
    let result = evaluate_insertion(&route_ctx, &create_static_activity(5, Demand::pickup(1)), 1);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {excludes_the_stop_holding_the_limited_jobs_delivery, (prev_idx, target_loc, expected), {
    excludes_the_stop_holding_the_limited_jobs_delivery_impl(prev_idx, target_loc, expected);
}}

excludes_the_stop_holding_the_limited_jobs_delivery! {
    case_01_same_location_right_before_the_delivery: (4, 10, None),
    case_02_other_location_right_before_the_delivery: (4, 5, Some(RIDE_STOP_LIMITS_CODE)),
    case_03_same_location_merges_with_next: (3, 10, None),
    case_04_other_location_before_the_stop: (3, 5, Some(RIDE_STOP_LIMITS_CODE)),
}

fn excludes_the_stop_holding_the_limited_jobs_delivery_impl(
    prev_idx: usize,
    target_loc: usize,
    expected: Option<ViolationCode>,
) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, Some(0), None), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, target_loc);
    let (_second, second_pickup, second_delivery) = create_pudo_job("second", None, 3, 10);
    let route_ctx =
        create_route_ctx(vec![first_pickup, second_pickup, limited_pickup, second_delivery, limited_delivery]);

    // Tour: [start(0), first_pickup(1), second_pickup(2), limited_pickup(3), second_delivery(4),
    //        limited_delivery(5), end(6)]; second_delivery and limited_delivery share one stop.
    let result = evaluate_insertion(&route_ctx, &first_delivery, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {bounds_the_ride_by_the_first_pickup, (prev_idx, expected), {
    bounds_the_ride_by_the_first_pickup_impl(prev_idx, expected);
}}

bounds_the_ride_by_the_first_pickup! {
    case_01_between_own_pickups: (1, Some(RIDE_STOP_LIMITS_CODE)),
    case_02_before_first_pickup: (0, None),
}

fn bounds_the_ride_by_the_first_pickup_impl(prev_idx: usize, expected: Option<ViolationCode>) {
    let (_limited, pickups, deliveries) = create_companion_job("limited", limits(Some(0), None, None), &[1, 2], &[10]);
    let (_other, other_pickup, _) = create_pudo_job("other", None, 5, 11);
    let route_ctx = create_route_ctx(pickups.into_iter().chain(deliveries).collect());

    // Tour: [start(0), first_pickup(1), second_pickup(2), delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &other_pickup, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {bounds_the_ride_by_the_last_delivery, (prev_idx, expected), {
    bounds_the_ride_by_the_last_delivery_impl(prev_idx, expected);
}}

bounds_the_ride_by_the_last_delivery! {
    case_01_between_own_deliveries: (2, Some(RIDE_STOP_LIMITS_CODE)),
    case_02_after_last_delivery: (3, None),
}

fn bounds_the_ride_by_the_last_delivery_impl(prev_idx: usize, expected: Option<ViolationCode>) {
    let (_limited, pickups, deliveries) = create_companion_job("limited", limits(None, Some(0), None), &[1], &[10, 11]);
    let (_other, _, other_delivery) = create_pudo_job("other", None, 2, 5);
    let route_ctx = create_route_ctx(pickups.into_iter().chain(deliveries).collect());

    // Tour: [start(0), pickup(1), first_delivery(2), second_delivery(3), end(4)].
    let result = evaluate_insertion(&route_ctx, &other_delivery, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {merges_a_target_into_an_adjacent_stop_inside_the_ride, (prev_idx, target_loc, expected), {
    merges_a_target_into_an_adjacent_stop_inside_the_ride_impl(prev_idx, target_loc, expected);
}}

merges_a_target_into_an_adjacent_stop_inside_the_ride! {
    case_01_same_location_merges_with_next: (2, 5, None),
    case_02_same_location_merges_with_prev: (3, 5, None),
    case_03_other_location_adds_a_stop: (3, 6, Some(RIDE_STOP_LIMITS_CODE)),
}

fn merges_a_target_into_an_adjacent_stop_inside_the_ride_impl(
    prev_idx: usize,
    target_loc: usize,
    expected: Option<ViolationCode>,
) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits(None, None, Some(1)), 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 5);
    let (_second, second_pickup, _) = create_pudo_job("second", None, target_loc, 11);
    let route_ctx = create_route_ctx(vec![first_pickup, limited_pickup, first_delivery, limited_delivery]);

    // Tour: [start(0), first_pickup(1), limited_pickup(2), first_delivery(3), limited_delivery(4), end(5)].
    let result = evaluate_insertion(&route_ctx, &second_pickup, prev_idx);

    assert_eq!(violation_code(result), expected);
}

parameterized_test! {counts_a_mixed_stop_once, (limits, expected), {
    counts_a_mixed_stop_once_impl(limits, expected);
}}

counts_a_mixed_stop_once! {
    case_01_one_stop: (limits(None, None, Some(1)), None),
    case_02_one_delivery: (limits(None, Some(1), None), None),
    case_03_zero_pickups: (limits(Some(0), None, None), Some(RIDE_STOP_LIMITS_CODE)),
}

fn counts_a_mixed_stop_once_impl(limits: Option<RideStopLimits>, expected: Option<ViolationCode>) {
    let (_limited, limited_pickup, limited_delivery) = create_pudo_job("limited", limits, 1, 10);
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 2, 5);
    let (_second, second_pickup, _) = create_pudo_job("second", None, 5, 11);
    let route_ctx = create_route_ctx(vec![first_pickup, limited_pickup, first_delivery, limited_delivery]);

    // Tour: [start(0), first_pickup(1), limited_pickup(2), first_delivery(3), limited_delivery(4), end(5)].
    let result = evaluate_insertion(&route_ctx, &second_pickup, 3);

    assert_eq!(violation_code(result), expected);
}

#[test]
fn allows_any_insertion_in_a_route_without_limited_jobs() {
    let (_first, first_pickup, first_delivery) = create_pudo_job("first", None, 1, 10);
    let (_second, second_pickup, _) = create_pudo_job("second", None, 2, 11);
    let route_ctx = create_route_ctx(vec![first_pickup, first_delivery]);

    let result = evaluate_insertion(&route_ctx, &second_pickup, 1);

    assert_eq!(violation_code(result), None);
}

parameterized_test! {merge_refuses_a_limited_job, (source_limited, candidate_limited, expected), {
    merge_refuses_a_limited_job_impl(source_limited, candidate_limited, expected);
}}

merge_refuses_a_limited_job! {
    case_01: (false, false, Ok(())),
    case_02: (true, false, Err(RIDE_STOP_LIMITS_CODE)),
    case_03: (false, true, Err(RIDE_STOP_LIMITS_CODE)),
    case_04: (true, true, Err(RIDE_STOP_LIMITS_CODE)),
}

fn merge_refuses_a_limited_job_impl(
    source_limited: bool,
    candidate_limited: bool,
    expected: Result<(), ViolationCode>,
) {
    let create_job = |job_id: &str, limited: bool| {
        let mut builder = TestSingleBuilder::default();
        builder.id(job_id);
        if limited {
            builder.dimens_mut().set_job_ride_stop_limits(RideStopLimits {
                pickups: Some(0),
                deliveries: None,
                stops: None,
            });
        }
        Job::Single(builder.build_shared())
    };
    let constraint = create_feature().constraint.unwrap();

    let result = constraint.merge(create_job("source", source_limited), create_job("candidate", candidate_limited));

    assert_eq!(result.map(|_| ()), expected);
}
