use crate::format::problem::*;
use crate::format_time;
use crate::helpers::*;

/// End-to-end proof that a real solve (not just the wrapper's own unit tests) produces a solution
/// the crate's own checker accepts. Before the checker fix this test would have panicked inside
/// `solve_with_cheapest_insertion` (which runs `CheckerContext::check` on every solve): the ferry
/// leg is solved and scheduled using the ferry-aware duration/distance, but `check_routing` used
/// to re-derive every leg from the raw road-only matrix, so it would reject the very solution the
/// solver just produced with an "arrival time mismatch" (road ~100s vs the actual ferry-crossed
/// leg).
#[test]
fn solves_and_checks_a_job_reached_by_crossing_a_ferry() {
    // quays (1,0)/(99,0) sit close to the vehicle start and the job; the ferry (1 approach + 5
    // crossing + 1 egress = 7s) is far shorter than the ~100s direct road leg, so a real solve
    // takes it.
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 1., arr: 6. }], b_to_a: vec![] };

    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    // `solve_with_cheapest_insertion` runs the checker on the resulting solution and panics if it
    // rejects it - that panic, not an assertion here, is what proves the fix.
    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let tour = solution.tours.first().expect("job should be assigned to a tour");
    let job_stop = tour
        .stops
        .iter()
        .filter_map(|stop| stop.as_point())
        .find(|stop| stop.activities.iter().any(|activity| activity.job_id == "job1"))
        .expect("job1 should have a point stop");

    // the arrival the solver actually scheduled must reflect the ferry crossing (7s), not the
    // ~100s road-only figure - otherwise this test would pass on a checker fix that merely stopped
    // erroring without actually consulting the ferry-aware transport.
    assert_eq!(job_stop.time.arrival, format_time(7.));
}
