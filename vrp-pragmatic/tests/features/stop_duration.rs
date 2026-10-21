use crate::format::problem::*;
use crate::format::solution::*;
use crate::format_time;
use crate::helpers::*;

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

#[test]
fn solves_identically_with_a_zero_or_no_stop_duration() {
    let create_problem = |stop_duration| Problem {
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
    };
    let solve_problem = |problem: Problem| {
        let matrix = create_matrix_from_problem(&problem);
        solve_with_cheapest_insertion(problem, Some(vec![matrix]))
    };

    let zero = solve_problem(create_problem(Some(0.)));
    let none = solve_problem(create_problem(None));

    assert_eq!(zero.tours, none.tours);
    assert_eq!(zero.statistic, none.statistic);
}
