use super::*;
use crate::format_time;
use crate::helpers::*;
use vrp_core::models::common::Profile;
use vrp_core::models::problem::TravelTime;
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
