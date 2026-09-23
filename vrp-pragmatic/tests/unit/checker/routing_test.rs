use super::*;
use crate::helpers::*;
use std::sync::Arc;
use vrp_core::models::examples::create_example_problem;

fn create_test_problem() -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (1., 0.)), create_delivery_job("job2", (2., 0.))],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    }
}

fn create_test_statistic() -> Statistic {
    Statistic { cost: 10., distance: 4, duration: 6, times: Timing { driving: 4, serving: 2, ..Timing::default() } }
}

fn create_test_solution(statistic: Statistic, stop_data: &[(Float, i64); 3]) -> Solution {
    let [first, second, third] = stop_data;
    SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default().coordinate((0., 0.)).schedule_stamp(0., 0.).load(vec![2]).build_departure(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(first.0, 2.)
                        .load(vec![1])
                        .distance(first.1)
                        .build_single("job1", "delivery"),
                    StopBuilder::default()
                        .coordinate((2., 0.))
                        .schedule_stamp(second.0, 4.)
                        .load(vec![0])
                        .distance(second.1)
                        .build_single("job2", "delivery"),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(third.0, 6.)
                        .load(vec![0])
                        .distance(third.1)
                        .build_arrival(),
                ])
                .statistic(statistic)
                .build(),
        )
        .build()
}

fn duration_error(stop_idx: usize, actual: usize, expected: usize) -> GenericError {
    format!("arrival time mismatch for {stop_idx} stop in the tour: my_vehicle_1, expected: '1970-01-01T00:00:0{expected}Z', got: '1970-01-01T00:00:0{actual}Z'").into()
}

fn distance_error(stop_idx: usize, actual: usize, expected: usize) -> GenericError {
    format!("distance mismatch for {stop_idx} stop in the tour: my_vehicle_1, expected: '{expected}', got: '{actual}'")
        .into()
}

parameterized_test! {can_check_stop, (stop_data, expected_result), {
    can_check_stop_impl(stop_data, expected_result);
}}

can_check_stop! {
    case_01: (&[(1., 1), (3., 2), (6., 4)], Ok(())),

    // NOTE due to rounding issues, we have to compare with tolerance 1
    case_02: (&[(2., 1), (3., 2), (6., 4)], Ok(())),
    case_03: (&[(1., 1), (3., 1), (6., 4)], Ok(())),

    case_04: (&[(3., 1), (3., 2), (6., 4)], Err(vec![duration_error(1, 3, 1)])),
    case_05: (&[(1., 1), (1., 2), (6., 4)], Err(vec![duration_error(2, 1, 3)])),
    case_06: (&[(1., 1), (3., 2), (8., 4)], Err(vec![duration_error(3, 8, 6)])),

    case_07: (&[(1., 3), (3., 2), (6., 4)], Err(vec![distance_error(1, 3, 1)])),
    case_08: (&[(1., 1), (3., 0), (6., 4)], Err(vec![distance_error(2, 0, 2)])),
    case_09: (&[(1., 1), (3., 2), (6., 6)], Err(vec![distance_error(3, 6, 4)])),
}

fn can_check_stop_impl(stop_data: &[(Float, i64); 3], expected_result: Result<(), Vec<GenericError>>) {
    let problem = create_test_problem();
    let matrix = create_matrix_from_problem(&problem);
    let solution = create_test_solution(create_test_statistic(), stop_data);
    let ctx = CheckerContext::new(create_example_problem(), problem, Some(vec![matrix]), solution).unwrap();

    let result = check_routing(&ctx);

    assert_eq!(result, expected_result);
}

parameterized_test! {can_check_tour_statistic, (statistic, expected_result), {
    can_check_tour_statistic_impl(statistic, expected_result);
}}

can_check_tour_statistic! {
    case_01: (create_test_statistic(), Ok(())),

    case_02: (Statistic {
        distance: 1,
        ..create_test_statistic()
    }, Err(vec!["distance mismatch for tour statistic: my_vehicle_1, expected: '4', got: '1'".into()])),

    case_03: (Statistic {
        duration: 1,
        ..create_test_statistic()
    }, Err(vec!["duration mismatch for tour statistic: my_vehicle_1, expected: '6', got: '1'".into()])),
}

fn can_check_tour_statistic_impl(statistic: Statistic, expected_result: Result<(), Vec<GenericError>>) {
    let problem = create_test_problem();
    let matrix = create_matrix_from_problem(&problem);
    let solution = create_test_solution(statistic, &[(1., 1), (3., 2), (6., 4)]);
    let ctx = CheckerContext::new(create_example_problem(), problem, Some(vec![matrix]), solution).unwrap();

    let result = check_routing(&ctx);

    assert_eq!(result, expected_result);
}

#[test]
fn can_check_solution_statistic() {
    let problem = create_test_problem();
    let matrix = create_matrix_from_problem(&problem);
    let solution = create_test_solution(create_test_statistic(), &[(1., 1), (3., 2), (6., 4)]);
    let wrong_statistic = Statistic { duration: 1, ..create_test_statistic() };
    let solution = Solution { statistic: wrong_statistic.clone(), ..solution };
    let ctx = CheckerContext::new(create_example_problem(), problem, Some(vec![matrix]), solution).unwrap();

    let result = check_routing(&ctx);

    assert_eq!(
        result,
        Err(vec![
            format!(
                "solution statistic mismatch, expected: '{:?}', got: '{:?}'",
                create_test_statistic(),
                wrong_statistic
            )
            .into()
        ])
    );
}

/// The solution format only stores whole-second timestamps: a real (sub-second) departure of
/// 3.5s round-trips through it as "3". A road leg doesn't care when you leave, but a ferry's
/// sailing choice is a step function of departure - reproduces the scenario that motivated
/// bracketing the truncated departure with `departure + 1`: the vehicle's real 3.5s departure
/// reaches the quay at 4.5s, missing the 4.2s sailing and catching the 20s one instead (arriving
/// at 26.0s: 3.5 + 1 approach + 15.5 wait + 5 crossing + 1 egress). A checker that only tried the
/// truncated 3s departure would compute reaching the quay at 4.0s, catch the 4.2s sailing, and
/// expect an arrival around 10s - sixteen seconds off and far outside the 1s tolerance.
#[test]
fn ferry_leg_resolves_correctly_despite_departure_truncated_to_whole_seconds() {
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings {
        a_to_b: vec![FerrySailing { dep: 4.2, arr: 4.3 }, FerrySailing { dep: 20., arr: 21. }],
        b_to_a: vec![],
    };

    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);
    // the real core problem (with the ferry index published in `Extras`), not
    // `create_example_problem()` - this test is specifically about the ferry-aware branch of
    // `check_routing`, which only activates when `Extras` actually carries one.
    let core_problem = Arc::new((problem.clone(), vec![matrix.clone()]).read_pragmatic().expect("valid problem"));

    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    // real departure 3.5s; `schedule_stamp` truncates it to "3" via `format_time`,
                    // exactly as a real solve's serialized output would.
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(0., 3.5)
                        .distance(0)
                        .load(vec![1])
                        .build_departure(),
                    // the real, correct arrival, from the true 3.5s departure catching the 20s
                    // sailing (see the function doc for the arithmetic).
                    StopBuilder::default()
                        .coordinate((100., 0.))
                        .schedule_stamp(26., 26.)
                        .distance(0)
                        .load(vec![0])
                        .build_single("job1", "delivery"),
                    // the return leg has no B-to-A sailings, so it resolves as the direct 100s
                    // road leg regardless of departure: 26 + 100 = 126.
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(126., 126.)
                        .distance(0)
                        .load(vec![0])
                        .build_arrival(),
                ])
                .statistic(Statistic {
                    cost: 0.,
                    distance: 0,
                    duration: 123, // 126 (final departure) - 3 (truncated first departure)
                    times: Timing { driving: 123, serving: 0, waiting: 0, break_time: 0, commuting: 0, parking: 0 },
                })
                .build(),
        )
        .build();

    let ctx = CheckerContext::new(core_problem, problem, Some(vec![matrix]), solution).expect("valid context");

    assert_eq!(check_routing(&ctx), Ok(()));
}
