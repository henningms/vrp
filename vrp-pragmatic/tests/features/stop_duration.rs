use crate::checker::CheckerContext;
use crate::format::problem::*;
use crate::format::solution::*;
use crate::format_time;
use crate::helpers::*;
use std::sync::Arc;
use vrp_core::prelude::GenericError;

/// One vehicle with capacity 20 at (0, 0), departing at 0: a fixed departure keeps waiting visible.
fn create_fleet() -> Fleet {
    Fleet {
        vehicles: vec![VehicleType {
            shifts: vec![VehicleShift {
                start: ShiftStart {
                    earliest: format_time(0.),
                    latest: Some(format_time(0.)),
                    location: (0., 0.).to_loc(),
                },
                ..create_default_vehicle_shift()
            }],
            ..create_vehicle_with_capacity("my_vehicle", vec![20])
        }],
        ..create_default_fleet()
    }
}

fn solve_jobs(jobs: Vec<Job>) -> Tour {
    let problem =
        Problem { plan: Plan { jobs, ..create_empty_plan() }, fleet: create_fleet(), ..create_empty_problem() };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none(), "{:?}", solution.unassigned);
    solution.tours.into_iter().next().expect("one tour")
}

fn interval(start: f64, end: f64) -> Option<Interval> {
    Some(Interval { start: format_time(start), end: format_time(end) })
}

fn point(tour: &Tour, idx: usize) -> &PointStop {
    tour.stops[idx].as_point().expect("a point stop")
}

#[test]
fn serves_many_jobs_at_one_stop_paying_the_stop_duration_once() {
    let jobs = (1..=16)
        .map(|idx| set_stop_duration(create_delivery_job_with_duration(&format!("job{idx}"), (10., 0.), 5.), Some(30.)))
        .collect();

    let tour = solve_jobs(jobs);

    assert_eq!(tour.stops.len(), 3);
    let stop = point(&tour, 1);
    assert_eq!(stop.time.arrival, format_time(10.));
    // 30 + 16 * 5 from arrival to departure
    assert_eq!(stop.time.departure, format_time(120.));
    assert_eq!(stop.parking, interval(10., 40.));
    assert_eq!(stop.activities.len(), 16);
    assert_eq!(stop.activities.first().unwrap().time, interval(40., 45.));
    assert_eq!(stop.activities.last().unwrap().time, interval(115., 120.));
    assert_eq!(point(&tour, 2).time.arrival, format_time(130.));
    assert_eq!(point(&tour, 2).parking, None);

    let statistic = &tour.statistic;
    let times = &statistic.times;
    assert_eq!((statistic.distance, statistic.duration), (20, 130));
    assert_eq!((times.driving, times.parking, times.serving, times.waiting), (20, 30, 80, 0));
    assert_eq!(times.driving + times.parking + times.serving + times.waiting, statistic.duration);
    // fixed 10, distance 20, time 130
    assert_eq!(statistic.cost, 160.);
}

#[test]
fn starts_service_at_the_later_of_the_stop_duration_end_and_the_window_start() {
    // ready at 10 + 30 = 40: a window opening at 25 is open by then, one opening at 60 is not
    let early = solve_jobs(vec![set_stop_duration(
        create_delivery_job_with_times("job", (10., 0.), vec![(25, 100)], 5.),
        Some(30.),
    )]);
    assert_eq!(point(&early, 1).parking, interval(10., 40.));
    assert_eq!(point(&early, 1).activities[0].time, interval(40., 45.));
    assert_eq!(early.statistic.times.waiting, 0);

    let late = solve_jobs(vec![set_stop_duration(
        create_delivery_job_with_times("job", (10., 0.), vec![(60, 100)], 5.),
        Some(30.),
    )]);
    assert_eq!(point(&late, 1).time.arrival, format_time(10.));
    assert_eq!(point(&late, 1).parking, interval(10., 40.));
    assert_eq!(point(&late, 1).activities[0].time, interval(60., 65.));
    assert_eq!((late.statistic.times.parking, late.statistic.times.waiting), (30, 20));
    assert_eq!(late.statistic.duration, 75);
    let times = &late.statistic.times;
    assert_eq!(times.driving + times.parking + times.serving + times.waiting, late.statistic.duration);
}

#[test]
fn pays_none_from_the_start_but_pays_it_into_an_end_location_shared_with_a_job() {
    // a service, as the checker's load rule does not count demand served at the departure stop
    let mut at_depot = create_service_job("at_depot", (0., 0.));
    at_depot.services.as_mut().unwrap()[0].places[0].times = Some(vec![vec![format_time(0.), format_time(5.)]]);

    let tour = solve_jobs(vec![
        set_stop_duration(at_depot, Some(30.)),
        set_stop_duration(create_delivery_job("far", (10., 0.)), Some(30.)),
    ]);

    assert_eq!(get_ids_from_tour(&tour), vec![vec!["departure", "at_depot"], vec!["far"], vec!["arrival"]]);
    assert_eq!(point(&tour, 0).parking, None);
    assert_eq!(point(&tour, 1).parking, interval(11., 41.));
    // the end location is the place of `at_depot`, so the return pays its stop duration
    assert_eq!(point(&tour, 2).time.arrival, format_time(52.));
    assert_eq!(point(&tour, 2).parking, interval(52., 82.));
    assert_eq!(tour.statistic.times.parking, 60);
}

#[test]
fn pays_no_stop_duration_for_a_break_without_a_location() {
    // the break, without a location, sits at job1's location after its service
    let mut fleet = create_fleet();
    fleet.vehicles[0].shifts[0].breaks = Some(vec![VehicleBreak::Optional {
        time: VehicleOptionalBreakTime::TimeWindow(vec![format_time(35.), format_time(40.)]),
        places: vec![VehicleOptionalBreakPlace { duration: 2., location: None, tag: None }],
        policy: None,
    }]);
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                set_stop_duration(create_delivery_job("job1", (5., 0.)), Some(30.)),
                set_stop_duration(create_delivery_job("job2", (10., 0.)), Some(30.)),
            ],
            ..create_empty_plan()
        },
        fleet,
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    let tour = &solution.tours[0];
    assert_eq!(get_ids_from_tour(tour), vec![vec!["departure"], vec!["job1", "break"], vec!["job2"], vec!["arrival"]]);
    assert_eq!(point(tour, 1).parking, interval(5., 35.));
    assert_eq!(point(tour, 2).parking, interval(43., 73.));
    assert_eq!((tour.statistic.times.parking, tour.statistic.times.break_time), (60, 2));
}

parameterized_test! {can_count_the_delivery_stop_duration_in_the_ride, (limit, is_assigned), {
    can_count_the_delivery_stop_duration_in_the_ride_impl(limit, is_assigned);
}}

can_count_the_delivery_stop_duration_in_the_ride! {
    case01_at_limit: (32., true),
    case02_over_limit: (31., false),
}

/// The pickup at (1, 0) ends at 1 + 30 + 1 = 32 and the delivery at (3, 0) starts at 32 + 2 + 30 = 64: a ride of 32.
fn can_count_the_delivery_stop_duration_in_the_ride_impl(limit: f64, is_assigned: bool) {
    let rider =
        create_pickup_delivery_job_with_params("rider", vec![1], ((1., 0.), 1., vec![]), ((3., 0.), 1., vec![]));
    let problem = Problem {
        plan: Plan {
            jobs: vec![Job { max_ride_duration: Some(limit), ..set_stop_duration(rider, Some(30.)) }],
            ..create_empty_plan()
        },
        fleet: create_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.unassigned.is_none(), is_assigned);
}

/// The problem both parity tests solve.
fn create_parity_problem(stop_duration: Option<f64>) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![
                set_stop_duration(create_delivery_job_with_times("job1", (1., 0.), vec![(0, 10)], 1.), stop_duration),
                set_stop_duration(create_delivery_job_with_times("job2", (2., 0.), vec![(10, 20)], 1.), stop_duration),
                set_stop_duration(create_delivery_job("job3", (3., 0.)), stop_duration),
            ],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    }
}

#[test]
fn solves_identically_with_a_zero_or_no_stop_duration() {
    let solve_problem = |problem: Problem| {
        let matrix = create_matrix_from_problem(&problem);
        solve_with_cheapest_insertion(problem, Some(vec![matrix]))
    };

    let zero = solve_problem(create_parity_problem(Some(0.)));
    let none = solve_problem(create_parity_problem(None));

    assert_eq!(zero.tours, none.tours);
    assert_eq!(zero.statistic, none.statistic);
}

#[test]
fn solves_identically_without_duration_offsets_or_with_zero_cells() {
    // A full solve with the default heuristic, whose post-processing moves departures; cheapest insertion runs none.
    // The repeatable generator is a per-thread generator seeded with 0 and not reset between solves, so the second
    // solve draws other numbers than the first. The comparison holds because this problem has a single best
    // solution: 30 sequential solves of it serialize identically.
    let problem = create_parity_problem(None);
    let matrix = create_matrix_from_problem(&problem);
    // a profile on at every time, so that a zero cell read as an offset would change the solution
    let zero_cells = set_duration_offsets(matrix.clone(), vec![create_duration_offset_profile(&[(0., 100.)])], &[]);

    let none = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));
    let zero = solve_with_metaheuristic(problem, Some(vec![zero_cells]));

    assert_eq!(serde_json::to_string(&zero).unwrap(), serde_json::to_string(&none).unwrap());
}

/// Sixteen jobs of 5 at (10, 0) with a stop duration of 30, as in `serves_many_jobs_at_one_stop_paying_the_stop_duration_once`.
fn create_one_stop_problem() -> (Problem, Matrix) {
    let jobs = (1..=16)
        .map(|idx| set_stop_duration(create_delivery_job_with_duration(&format!("job{idx}"), (10., 0.), 5.), Some(30.)))
        .collect();
    let problem =
        Problem { plan: Plan { jobs, ..create_empty_plan() }, fleet: create_fleet(), ..create_empty_problem() };
    let matrix = create_matrix_from_problem(&problem);

    (problem, matrix)
}

fn check(problem: &Problem, matrix: &Matrix, solution: Solution) -> Result<(), Vec<GenericError>> {
    let core_problem = Arc::new((problem.clone(), vec![matrix.clone()]).read_pragmatic().expect("valid problem"));

    CheckerContext::new(core_problem, problem.clone(), Some(vec![matrix.clone()]), solution)
        .expect("valid context")
        .check()
}

fn point_mut(solution: &mut Solution, idx: usize) -> &mut PointStop {
    match &mut solution.tours[0].stops[idx] {
        Stop::Point(point) => point,
        Stop::Transit(_) => panic!("a point stop"),
    }
}

#[test]
fn checker_rejects_a_parking_which_does_not_match_the_stop_duration() {
    let (problem, matrix) = create_one_stop_problem();
    let solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));
    assert_eq!(check(&problem, &matrix, solution.clone()), Ok(()));

    let cases = [(1, None, 30), (1, interval(10., 30.), 30), (1, interval(20., 50.), 30), (2, interval(130., 140.), 0)];
    for (stop_idx, parking, expected) in cases {
        let mut solution = solution.clone();
        point_mut(&mut solution, stop_idx).parking = parking.clone();

        let errors = check(&problem, &matrix, solution).expect_err("a wrong parking");

        let error: GenericError = format!(
            "parking at stop {stop_idx} in tour 'my_vehicle_1' does not match its stop duration of {expected}s"
        )
        .into();
        assert!(errors.contains(&error), "{parking:?}: {errors:?}");
    }
}

#[test]
fn checker_rejects_an_activity_which_starts_within_the_stop_duration() {
    let (problem, matrix) = create_one_stop_problem();
    let mut solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));
    let activity = &mut point_mut(&mut solution, 1).activities[0];
    activity.time = interval(30., 35.);
    let job_id = activity.job_id.clone();

    let errors = check(&problem, &matrix, solution).expect_err("an early activity");

    let error: GenericError =
        format!("activity '{job_id}' at stop 1 in tour 'my_vehicle_1' starts before its stop duration ends").into();
    assert!(errors.contains(&error), "{errors:?}");
}

#[test]
fn checker_rejects_a_parking_statistic_without_the_stop_durations() {
    let (problem, matrix) = create_one_stop_problem();
    let mut solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));
    solution.tours[0].statistic.times.parking = 0;

    let errors = check(&problem, &matrix, solution).expect_err("a wrong statistic");

    let error: GenericError = "parking statistic mismatch for tour 'my_vehicle_1': expected '30', got '0'".into();
    assert!(errors.contains(&error), "{errors:?}");
}

/// The problem of `pays_none_from_the_start_but_pays_it_into_an_end_location_shared_with_a_job` with a shift ending
/// at `latest` and an optional break of 5 in [60, 70].
fn create_shared_end_problem(latest: f64) -> (Problem, Matrix) {
    let mut at_depot = create_service_job("at_depot", (0., 0.));
    at_depot.services.as_mut().unwrap()[0].places[0].times = Some(vec![vec![format_time(0.), format_time(5.)]]);
    let mut fleet = create_fleet();
    let shift = &mut fleet.vehicles[0].shifts[0];
    shift.end.as_mut().expect("a shift end").latest = format_time(latest);
    shift.breaks = Some(vec![VehicleBreak::Optional {
        time: VehicleOptionalBreakTime::TimeWindow(vec![format_time(60.), format_time(70.)]),
        places: vec![VehicleOptionalBreakPlace { duration: 5., location: None, tag: None }],
        policy: None,
    }]);
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                set_stop_duration(at_depot, Some(30.)),
                set_stop_duration(create_delivery_job("far", (10., 0.)), Some(30.)),
            ],
            ..create_empty_plan()
        },
        fleet,
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    (problem, matrix)
}

/// The solver's solution to `create_shared_end_problem(82.)`: the tour pulls in at its end at 52 and ends at 82, too
/// late for the break.
fn create_shared_end_solution() -> Solution {
    let activity = |job_id: &str, activity_type: &str, time: (f64, f64)| {
        ActivityBuilder::default().job_id(job_id).activity_type(activity_type).time_stamp(time.0, time.1).build()
    };
    let stop = |location: (f64, f64), time: (f64, f64), distance, load, parking, activities| {
        Stop::Point(PointStop {
            location: location.to_loc(),
            time: Schedule { arrival: format_time(time.0), departure: format_time(time.1) },
            distance,
            load: vec![load],
            layout: None,
            parking,
            activities,
        })
    };
    let mut at_depot = activity("at_depot", "service", (0., 1.));
    at_depot.location = Some((0., 0.).to_loc());
    let times = Timing { driving: 20, serving: 2, parking: 60, ..Timing::default() };

    SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    stop((0., 0.), (0., 1.), 0, 1, None, vec![activity("departure", "departure", (0., 0.)), at_depot]),
                    stop(
                        (10., 0.),
                        (11., 42.),
                        10,
                        0,
                        interval(11., 41.),
                        vec![activity("far", "delivery", (41., 42.))],
                    ),
                    stop(
                        (0., 0.),
                        (52., 82.),
                        20,
                        0,
                        interval(52., 82.),
                        vec![activity("arrival", "arrival", (82., 82.))],
                    ),
                ])
                .statistic(Statistic { cost: 112., distance: 20, duration: 82, times })
                .build(),
        )
        .violations(Some(vec![Violation::Break { vehicle_id: "my_vehicle_1".to_string(), shift_index: 0 }]))
        .build()
}

#[test]
fn checker_ends_a_tour_after_the_stop_duration_at_its_end_location() {
    let (problem, matrix) = create_shared_end_problem(82.);

    // the break window opens after the pull-in at 52 but within the tour, so the unassigned break is due
    assert_eq!(check(&problem, &matrix, create_shared_end_solution()), Ok(()));
}

#[test]
fn checker_rejects_a_tour_whose_end_location_stop_duration_ends_after_the_shift() {
    let (problem, matrix) = create_shared_end_problem(81.);

    let errors = check(&problem, &matrix, create_shared_end_solution()).expect_err("a late tour end");

    let error: GenericError = "tour time is outside shift time, vehicle id 'my_vehicle_1', shift index: 0".into();
    assert!(errors.contains(&error), "{errors:?}");
}

#[test]
fn checker_rejects_a_parking_in_a_problem_without_stop_durations() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (10., 0.))], ..create_empty_plan() },
        fleet: create_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);
    let solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));

    for (stop_idx, parking) in [(1, interval(10., 40.)), (2, interval(21., 51.))] {
        let mut solution = solution.clone();
        point_mut(&mut solution, stop_idx).parking = parking;

        let errors = check(&problem, &matrix, solution).expect_err("a parking without a stop duration");

        let error: GenericError =
            format!("parking at stop {stop_idx} in tour 'my_vehicle_1' does not match its stop duration of 0s").into();
        assert!(errors.contains(&error), "{errors:?}");
    }
}

#[test]
fn checker_accepts_a_location_revisited_paying_its_stop_duration_again() {
    // a1 closes before b can be served, and a2 opens after b closes: the tour visits a, b and a again
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                set_stop_duration(create_delivery_job_with_times("a1", (10., 0.), vec![(0, 45)], 1.), Some(30.)),
                set_stop_duration(create_delivery_job_with_times("b", (20., 0.), vec![(0, 200)], 1.), Some(30.)),
                set_stop_duration(create_delivery_job_with_times("a2", (10., 0.), vec![(300, 400)], 1.), Some(30.)),
            ],
            ..create_empty_plan()
        },
        fleet: create_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));

    let tour = &solution.tours[0];
    assert_eq!(get_ids_from_tour(tour), vec![vec!["departure"], vec!["a1"], vec!["b"], vec!["a2"], vec!["arrival"]]);
    assert_eq!(point(tour, 1).parking, interval(10., 40.));
    assert_eq!(point(tour, 2).parking, interval(51., 81.));
    assert_eq!(point(tour, 3).time.arrival, format_time(92.));
    assert_eq!(point(tour, 3).parking, interval(92., 122.));
    assert_eq!(point(tour, 3).activities[0].time, interval(300., 301.));
    assert_eq!(tour.statistic.times.parking, 90);
    assert_eq!(check(&problem, &matrix, solution), Ok(()));
}

#[test]
fn pays_the_stop_duration_once_after_a_ferry_crossing() {
    // the crossing of `solves_and_checks_a_job_reached_by_crossing_a_ferry`: (0, 0) to job1 at (100, 0) in 7 by ferry
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 1., arr: 6. }], b_to_a: vec![] };
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job("job1", (100., 0.)), Some(30.))],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let tour = &solution.tours[0];
    assert_eq!(point(tour, 1).time.arrival, format_time(7.));
    assert_eq!(point(tour, 1).parking, interval(7., 37.));
    assert_eq!(tour.statistic.times.parking, 30);
    assert_eq!(solution.ferry_legs.as_ref().map(|legs| legs.len()), Some(1));
}

#[test]
fn does_not_scale_the_stop_duration_with_the_vehicle_profile() {
    let mut fleet = create_fleet();
    fleet.vehicles[0].profile.scale = Some(2.);
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job("job1", (10., 0.)), Some(30.))],
            ..create_empty_plan()
        },
        fleet,
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let tour = &solution.tours[0];
    assert_eq!(point(tour, 1).time.arrival, format_time(20.));
    assert_eq!(point(tour, 1).parking, interval(20., 50.));
    assert_eq!(point(tour, 2).time.arrival, format_time(71.));
    assert_eq!((tour.statistic.times.driving, tour.statistic.times.parking), (40, 30));
}

#[test]
fn checks_a_fractional_stop_duration_in_whole_seconds() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job("job1", (10., 0.)), Some(30.5))],
            ..create_empty_plan()
        },
        fleet: create_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    // the solve helper runs the checker, which must accept the whole-second output of 40.5 and 41.5
    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let tour = &solution.tours[0];
    assert_eq!(point(tour, 1).parking, interval(10., 40.));
    assert_eq!(point(tour, 1).activities[0].time, interval(40., 41.));
    assert_eq!(tour.statistic.times.parking, 30);
}
