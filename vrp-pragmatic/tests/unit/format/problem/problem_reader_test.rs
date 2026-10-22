use super::*;
use crate::format_time;
use crate::helpers::*;
use vrp_core::models::common::Profile;
use vrp_core::models::problem::{DurationOffsetsExtraProperty, TravelTime};
use vrp_core::models::solution::Route;

#[test]
fn no_ferry_crossings_leaves_extras_without_a_ferry_transport_entry() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    // "empty means unchanged": no ferryCrossings must not publish a ferry transport entry, the
    // only externally-observable trace `get_problem_blocks` leaves of having installed the
    // wrapper at all.
    assert!(core_problem.extras.get_ferry_transport().is_none());
}

#[test]
fn ferry_crossings_install_a_ferry_aware_transport_reachable_through_extras() {
    // quays (1,0) and (99,0) sit right beside the vehicle start and the job, so the ferry leg
    // (1 approach + 5 crossing + 1 egress = 7) is far shorter than the direct road leg (100) -
    // proof that `core_problem.transport`, the object the full solve and `FeasibilityContext`
    // actually use, is the ferry-aware one and not the plain road transport `get_problem_blocks`
    // builds first.
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 0., arr: 5. }], b_to_a: vec![] };

    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    let ferry_transport = core_problem.extras.get_ferry_transport().expect("ferry transport published");
    assert_eq!(ferry_transport.index.crossings().len(), 1);

    // plan jobs enter the coordinate index before fleet locations, so job1 is 0 and the vehicle's
    // (0,0) start/end is 1.
    let profile = Profile::default();
    let road_duration = ferry_transport.road.duration_approx(&profile, 1, 0);
    let installed_duration = core_problem.transport.duration_approx(&profile, 1, 0);

    assert_eq!(road_duration, 100.);
    assert_eq!(installed_duration, 7.);
}

/// Pins the order `get_problem_blocks` wraps in: ferry-aware inside, reserved-time outside.
/// Moving the ferry wrap to the other side leaves every other test in this crate green (the two
/// wrappers otherwise compose without error either way), so only a test that reads the *charged*
/// duration for a leg where both a crossing and a required break apply can tell the orders apart.
#[test]
fn wrapping_order_charges_both_the_ferry_and_an_overlapping_required_break() {
    // ferry total (zero wait): 1 approach + 5 crossing + 1 egress = 7s, far under the 100s direct
    // road leg, so the ferry wins under either wrapping order - only the *charged* total tells
    // them apart (see the comment on the final assertion).
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 1., arr: 6. }], b_to_a: vec![] };

    // window [3,4] falls strictly inside the ferry's own [0,7] leg window but outside its 1s
    // approach/egress sub-windows ([0,1] each) - so whether the break's 30s is charged at all
    // depends entirely on which wrapper sees which duration, not on overlap arithmetic.
    let vehicle_shift = VehicleShift {
        breaks: Some(vec![VehicleBreak::Required {
            time: VehicleRequiredBreakTime::ExactTime { earliest: format_time(3.), latest: format_time(4.) },
            duration: 30.,
        }]),
        ..create_default_vehicle_shift()
    };

    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType { shifts: vec![vehicle_shift], ..create_default_vehicle_type() }],
            ..create_default_fleet()
        },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    // the actual fleet actor, not a fresh one: the reserved-time index is keyed by `Arc<Actor>`
    // pointer identity, so only this exact Arc looks the break up.
    let actor = core_problem.fleet.actors.first().expect("one vehicle").clone();
    let route = Route { actor, tour: Default::default() };

    let duration = core_problem.transport.duration(&route, 1, 0, TravelTime::Departure(0.));

    // ferry-inside/reserved-outside (as installed): the outer reserved-time wrapper sees the
    // ferry's own [0,7] window, the break overlaps it, and its 30s is added on top: 7 + 30 = 37.
    // Wrapped the other way round, the break would be computed against the 100s road window
    // instead (still overlapping - it only inflates the number the ferry compares itself against,
    // never the total the ferry itself returns when it wins), so the 30s would never reach the
    // final answer and this would read 7 instead.
    assert_eq!(duration, 37.);
}

#[test]
fn reads_stop_duration_from_json() {
    let place: JobPlace =
        serde_json::from_str(r#"{"location":{"lat":1,"lng":0},"duration":5,"stopDuration":30}"#).unwrap();

    assert_eq!(place.stop_duration, Some(30.));
    assert!(!serde_json::to_string(&create_job_place((1., 0.), None)).unwrap().contains("stopDuration"));
}

#[test]
fn leaves_the_transport_unwrapped_without_a_positive_stop_duration() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (100., 0.)),
                set_stop_duration(create_delivery_job("job2", (50., 0.)), Some(0.)),
            ],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    // job1 is location 0, job2 location 1 and the depot location 2
    assert!(core_problem.extras.get_stop_durations().is_none());
    assert_eq!(core_problem.transport.duration_approx(&Profile::default(), 2, 0), 100.);
}

#[test]
fn reads_the_largest_stop_duration_per_location_into_the_transport() {
    let alternatives = Job {
        deliveries: Some(vec![JobTask {
            places: vec![
                JobPlace { stop_duration: Some(10.), ..create_job_place((50., 0.), None) },
                JobPlace { stop_duration: Some(20.), ..create_job_place((70., 0.), None) },
            ],
            demand: Some(vec![1]),
            named_demand: None,
            order: None,
        }]),
        ..create_job("alternatives")
    };
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                set_stop_duration(create_delivery_job("short", (100., 0.)), Some(30.)),
                set_stop_duration(create_delivery_job("long", (100., 0.)), Some(60.)),
                alternatives,
            ],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    // (100, 0) is location 0, (50, 0) is 1, (70, 0) is 2 and the depot 3
    let stop_durations = core_problem.extras.get_stop_durations().expect("stop durations published");
    assert_eq!((0..4).map(|location| stop_durations.get(location)).collect::<Vec<_>>(), vec![60., 10., 20., 0.]);
    let profile = Profile::default();
    assert_eq!(core_problem.transport.duration_approx(&profile, 3, 0), 160.);
    assert_eq!(core_problem.transport.duration_approx(&profile, 0, 3), 100.);
    assert_eq!(core_problem.transport.duration_approx(&profile, 0, 0), 0.);
    assert_eq!(core_problem.transport.distance_approx(&profile, 3, 0), 100.);
}

#[test]
fn adds_the_stop_duration_outside_the_ferry_aware_transport() {
    // the setup of `ferry_crossings_install_a_ferry_aware_transport_reachable_through_extras`: 7 by ferry, 100 by road
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 0., arr: 5. }], b_to_a: vec![] };
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

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    let profile = Profile::default();
    let road = core_problem.extras.get_ferry_transport().expect("ferry transport published").road.clone();
    assert_eq!(road.duration_approx(&profile, 1, 0), 100.);
    assert_eq!(core_problem.transport.duration_approx(&profile, 1, 0), 37.);
}

/// A job at (100, 0) is location 0 and the depot at (0, 0) location 1.
fn create_one_job_problem() -> Problem {
    Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    }
}

fn route_of(core_problem: &CoreProblem, profile_index: usize) -> Route {
    let actor = core_problem.fleet.actors.iter().find(|actor| actor.vehicle.profile.index == profile_index).unwrap();
    Route { actor: actor.clone(), tour: Default::default() }
}

#[test]
fn leaves_the_transport_unwrapped_without_duration_offsets_or_with_zero_cells() {
    let problem = create_one_job_problem();
    let none = create_matrix_from_problem(&problem);
    let zero = set_duration_offsets(none.clone(), vec![create_rush_hour_offset_profile()], &[]);

    for matrix in [none, zero] {
        let core_problem = (problem.clone(), vec![matrix]).read_pragmatic().expect("valid problem");

        assert!(core_problem.extras.get_duration_offsets().is_none());
        // a zero-cell wrapper would return the base too, so only this sees one
        assert!(!core_problem.transport.has_duration_offsets());
    }
}

#[test]
fn adds_duration_offsets_at_the_departure_inside_the_transport() {
    let problem = create_one_job_problem();
    let matrix = set_duration_offsets(
        create_matrix_from_problem(&problem),
        vec![create_rush_hour_offset_profile()],
        &[(1, 0, 1)],
    );

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    let (transport, route) = (&core_problem.transport, route_of(&core_problem, 0));
    assert_eq!(transport.duration(&route, 1, 0, TravelTime::Departure(clock(6., 56.))), 340.);
    assert_eq!(transport.duration(&route, 1, 0, TravelTime::Departure(clock(7., 30.))), 580.);
    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Departure(clock(7., 30.))), 100.);
    assert_eq!(transport.duration_approx(&Profile::default(), 1, 0), 100.);
    assert_eq!(transport.distance(&route, 1, 0, TravelTime::Departure(clock(7., 30.))), 100.);
    assert!(core_problem.extras.get_duration_offsets().is_some());
}

#[test]
fn reads_duration_offsets_into_the_road_of_the_ferry_aware_transport() {
    let problem = Problem {
        ferry_crossings: Some(vec![create_ferry_crossing("crossing1", (1., 0.), (99., 0.))]),
        ..create_one_job_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let depot = coord_index.get_by_loc(&Location::new_coordinate(0., 0.)).unwrap();
    let job = coord_index.get_by_loc(&Location::new_coordinate(100., 0.)).unwrap();
    let matrix = set_duration_offsets(
        create_matrix_from_problem(&problem),
        vec![create_rush_hour_offset_profile()],
        &[(depot, job, 1)],
    );

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    let road = core_problem.extras.get_ferry_transport().expect("ferry transport published").road.clone();
    let route = route_of(&core_problem, 0);
    assert_eq!(road.duration(&route, depot, job, TravelTime::Departure(clock(7., 30.))), 580.);
}

#[test]
fn applies_duration_offsets_only_to_vehicles_of_their_matrix_profile() {
    let problem = Problem {
        fleet: Fleet {
            vehicles: vec![
                create_default_vehicle_type(),
                VehicleType { profile: create_vehicle_profile_with_name("truck"), ..create_default_vehicle("truck") },
            ],
            profiles: vec![
                MatrixProfile { name: "car".to_string(), speed: None },
                MatrixProfile { name: "truck".to_string(), speed: None },
            ],
            ..create_default_fleet()
        },
        ..create_one_job_problem()
    };
    let car = create_matrix_from_problem(&problem);
    let truck = set_duration_offsets(
        Matrix { profile: Some("truck".to_string()), ..car.clone() },
        vec![create_rush_hour_offset_profile()],
        &[(1, 0, 1)],
    );

    // the truck matrix comes first, so its position is not its profile index
    let core_problem = (problem, vec![truck, car]).read_pragmatic().expect("valid problem");

    let departure = TravelTime::Departure(clock(7., 30.));
    assert_eq!(core_problem.transport.duration(&route_of(&core_problem, 1), 1, 0, departure), 580.);
    assert_eq!(core_problem.transport.duration(&route_of(&core_problem, 0), 1, 0, departure), 100.);
}

#[test]
fn moves_the_offset_cells_into_the_problem_without_copying() {
    let problem = create_one_job_problem();
    let matrix = set_duration_offsets(
        create_matrix_from_problem(&problem),
        vec![create_rush_hour_offset_profile()],
        &[(1, 0, 1)],
    );
    let pointer = matrix.duration_offsets.as_ref().unwrap().cells.as_ptr();

    let core_problem = (problem, vec![matrix]).read_pragmatic().expect("valid problem");

    assert_eq!(core_problem.extras.get_duration_offsets().unwrap().cells(0).unwrap().as_ptr(), pointer);
}

#[test]
fn refuses_duration_offsets_which_bypass_validation_as_e0002() {
    let problem = create_one_job_problem();
    let matrix = create_matrix_from_problem(&problem);
    let unparsable =
        DurationOffsetProfile { points: vec![DurationOffsetPoint { time: "07:00".to_string(), offset: 0. }] };
    let falling = create_duration_offset_profile(&[(clock(7., 0.), 600.), (clock(7., 5.), 0.)]);
    let short = MatrixDurationOffsets { profiles: vec![create_rush_hour_offset_profile()], cells: vec![1] };

    for (matrix, cause) in [
        (set_duration_offsets(matrix.clone(), vec![unparsable], &[(1, 0, 1)]), "cannot parse date"),
        (set_duration_offsets(matrix.clone(), vec![falling], &[(1, 0, 1)]), "falls faster than time"),
        (Matrix { duration_offsets: Some(short), ..matrix }, "have 1 cells, expected 4"),
    ] {
        let props = get_problem_properties(&problem, std::slice::from_ref(&matrix));
        let coord_index = Arc::new(CoordIndex::new(&problem));

        let result = get_problem_blocks(&problem, vec![matrix], coord_index, &mut JobIndex::default(), &props, &[]);

        let errors = result.err().expect("refused").errors;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E0002");
        assert!(errors[0].action.starts_with("check matrix duration offsets: "), "{}", errors[0].action);
        assert!(errors[0].action.contains(cause), "{}", errors[0].action);
    }
}

#[test]
fn refuses_duration_offsets_for_more_locations_than_error_codes_leave_as_e0002() {
    // validation compares neither with the other: one errorCodes entry leaves a routing matrix of one location, while
    // the four cells cover the two of travelTimes
    let problem = create_one_job_problem();
    let matrix = set_duration_offsets(
        Matrix { error_codes: Some(vec![0]), ..create_matrix_from_problem(&problem) },
        vec![create_rush_hour_offset_profile()],
        &[(1, 0, 1)],
    );

    let errors = (problem, vec![matrix]).read_pragmatic().expect_err("refused").errors;

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "E0002");
    assert!(errors[0].action.starts_with("check matrix duration offsets: "), "{}", errors[0].action);
    assert!(errors[0].action.contains("have 4 cells, but the routing matrix has 1"), "{}", errors[0].action);
}
