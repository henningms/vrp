use crate::format::problem::*;
use crate::format::solution::*;
use crate::format::{CoordIndex, Location};
use crate::helpers::*;
use crate::{format_time, parse_time};

/// A delivery without service time at matrix index `index`, served within `window`.
fn create_job_at_index(id: &str, index: usize, window: (f64, f64)) -> Job {
    let mut job = create_delivery_job_with_index(id, index);
    let place = &mut job.deliveries.as_mut().expect("a delivery")[0].places[0];
    place.duration = 0.;
    place.times = Some(vec![vec![format_time(window.0), format_time(window.1)]]);
    job
}

fn create_shift_start(location: Location, earliest: f64, latest: Option<f64>) -> ShiftStart {
    ShiftStart { earliest: format_time(earliest), latest: latest.map(format_time), location }
}

fn create_fleet(start: ShiftStart, end: Option<ShiftEnd>) -> Fleet {
    Fleet {
        vehicles: vec![VehicleType {
            shifts: vec![VehicleShift { start, end, ..create_default_open_vehicle_shift() }],
            ..create_default_vehicle_type()
        }],
        ..create_default_fleet()
    }
}

fn interval(start: f64, end: f64) -> Option<Interval> {
    Some(Interval { start: format_time(start), end: format_time(end) })
}

#[test]
fn serves_two_of_three_jobs_when_a_delay_easing_out_would_make_the_third_late() {
    // B is in time only from A, and X, A, B is the only order serving all three: A is served at 08:56, and A -> B
    // takes 600 s plus a delay easing out from 480 s at 09:00 to none at 09:12, so B is reached at 09:14 at the
    // earliest, after its window closes. Without the delay it would be reached at 09:06.
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_job_at_index("A", 1, (clock(8., 0.), clock(12., 0.))),
                create_job_at_index("B", 2, (clock(8., 0.), clock(9., 12.))),
                create_job_at_index("X", 3, (clock(8., 55.), clock(8., 56.))),
            ],
            ..create_empty_plan()
        },
        fleet: create_fleet(create_shift_start(Location::Reference { index: 0 }, clock(8., 0.), None), None),
        ..create_empty_problem()
    };
    // rows 0 (depot), 1 (A), 2 (B) and 3 (X): 60 s apart, except 600 s for A -> B and two hours for depot -> B and
    // either way between B and X
    let matrix = create_matrix(vec![0, 60, 7200, 60, 60, 0, 600, 60, 60, 60, 0, 7200, 60, 60, 7200, 0]);
    let delay = create_duration_offset_profile(&[
        (clock(6., 30.), 0.),
        (clock(7., 0.), 480.),
        (clock(9., 0.), 480.),
        (clock(9., 12.), 0.),
    ]);
    let matrix = set_duration_offsets(matrix, vec![delay], &[(1, 2, 1)]);

    let solution = solve_with_metaheuristic_and_iterations(problem, Some(vec![matrix]), 200);

    assert_eq!(solution.unassigned.map(|jobs| jobs.len()), Some(1));
}

#[test]
fn adds_the_offset_on_the_road_and_the_stop_duration_at_the_stop() {
    // leaving location 1 at 06:56, the job at location 0 is 10 s plus 240 s of the rising offset away; the way back
    // has no offset
    let end = ShiftEnd { earliest: None, latest: format_time(clock(23., 0.)), location: (0., 0.).to_loc() };
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job_with_duration("job1", (10., 0.), 5.), Some(30.))],
            ..create_empty_plan()
        },
        fleet: create_fleet(create_shift_start((0., 0.).to_loc(), clock(6., 56.), Some(clock(6., 56.))), Some(end)),
        ..create_empty_problem()
    };
    let matrix = set_duration_offsets(
        create_matrix_from_problem(&problem),
        vec![create_rush_hour_offset_profile()],
        &[(1, 0, 1)],
    );

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    let tour = &solution.tours[0];
    let stop = tour.stops[1].as_point().expect("a point stop");
    assert_eq!(stop.time.arrival, format_time(clock(7., 0.) + 10.));
    assert_eq!(stop.parking, interval(clock(7., 0.) + 10., clock(7., 0.) + 40.));
    assert_eq!(stop.activities[0].time, interval(clock(7., 0.) + 40., clock(7., 0.) + 45.));
    assert_eq!(tour.stops[2].schedule().arrival, format_time(clock(7., 0.) + 55.));
    let statistic = &tour.statistic;
    assert_eq!((statistic.times.driving, statistic.times.parking, statistic.times.serving), (260, 30, 5));
    assert_eq!(statistic.duration, 295);
}

#[test]
fn takes_the_offset_on_the_drive_after_a_sailing() {
    // the leg leaves at 06:20, before the band, but its drive off the ferry starts at 07:20, inside it
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 1200.;
    crossing.boarding_buffer_sec = 600.;
    crossing.sailings =
        FerrySailings { a_to_b: vec![FerrySailing { dep: clock(7., 0.), arr: clock(7., 20.) }], b_to_a: vec![] };
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_fleet(create_shift_start((0., 0.).to_loc(), clock(6., 20.), Some(clock(6., 20.))), None),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let index = |location: (f64, f64)| coord_index.get_by_loc(&location.to_loc()).expect("a known location");
    let size = coord_index.unique().len();
    let mut matrix = create_matrix_from_problem(&problem);
    // a slow direct road, so that the crossing wins even with the offset on its drive off
    matrix.travel_times[index((0., 0.)) * size + index((100., 0.))] = 20_000;
    let matrix = set_duration_offsets(
        matrix,
        vec![create_rush_hour_offset_profile()],
        &[(index((99., 0.)), index((100., 0.)), 1)],
    );

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    // the sailing arrives at 07:20, then 1 s to the job plus the 480 s offset
    assert_eq!(solution.tours[0].stops[1].schedule().arrival, format_time(clock(7., 20.) + 1. + 480.));
    let legs = solution.ferry_legs.as_ref().expect("a ferry leg");
    assert_eq!(legs.len(), 1);
    assert_eq!(legs[0].arrive_quay_at, clock(6., 20.) + 1.);
    assert_eq!(legs[0].sailing_departure, clock(7., 0.));
}

#[test]
fn starts_a_tour_no_later_than_a_rising_delay_allows() {
    // the default post-processing delays the start to cut waiting; aimed at 07:20 by the 600 s base alone, it would
    // leave at 07:10, when the leg takes 480 s more, and arrive at 07:28
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_job_at_index("job1", 1, (clock(7., 20.), clock(7., 25.)))],
            ..create_empty_plan()
        },
        fleet: create_fleet(create_shift_start(Location::Reference { index: 0 }, clock(6., 0.), None), None),
        ..create_empty_problem()
    };
    let matrix = set_duration_offsets(
        create_matrix(vec![0, 600, 600, 0]),
        vec![create_rush_hour_offset_profile()],
        &[(0, 1, 1)],
    );

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    let arrival = parse_time(&solution.tours[0].stops[1].schedule().arrival);
    assert!((clock(7., 20.)..=clock(7., 25.)).contains(&arrival), "arrival at {arrival}");
}
