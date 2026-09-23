use super::*;
use crate::helpers::*;
use vrp_core::models::common::Profile;

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
