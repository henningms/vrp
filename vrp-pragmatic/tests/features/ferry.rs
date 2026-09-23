use crate::checker::CheckerContext;
use crate::format::problem::*;
use crate::format::solution::{FerryLegDirection, serialize_solution};
use crate::format_time;
use crate::helpers::*;
use std::io::BufWriter;
use std::sync::Arc;

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

/// A mutant that made `check_routing`'s ferry branch return success unconditionally (or skip
/// ferry legs entirely) would still pass `solves_and_checks_a_job_reached_by_crossing_a_ferry`
/// above - a solution that is actually correct looks the same either way. This test tells a
/// checker that genuinely validates ferry legs apart from one that merely waves them through: it
/// takes the same real, valid, solved solution and corrupts the ferry leg's own reported arrival,
/// then asserts the checker rejects it.
#[test]
fn rejects_a_solution_with_a_corrupted_ferry_leg_arrival() {
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

    // a real, valid solve (implicitly checked and passing, as in the test above).
    let mut solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));

    let stop = solution
        .tours
        .first_mut()
        .expect("job should be assigned to a tour")
        .stops
        .iter_mut()
        .find(|stop| stop.activities().iter().any(|activity| activity.job_id == "job1"))
        .expect("job1 should have a point stop");
    // 57s instead of the real 7s - the road-only leg is ~100s, so this is not merely "the road
    // number instead of the ferry number" (already proven distinct by construction); it is wrong
    // by every measure, and must be rejected regardless of which number a checker compares it to.
    stop.schedule_mut().arrival = format_time(57.);

    let core_problem = Arc::new((problem.clone(), vec![matrix.clone()]).read_pragmatic().expect("valid problem"));
    let ctx = CheckerContext::new(core_problem, problem, Some(vec![matrix]), solution).expect("valid context");

    assert!(ctx.check().is_err(), "checker must reject a corrupted ferry leg arrival, not wave it through");
}

/// A real solve reports which sailing it relied on. Approach (3s) and egress (10s) are distinct
/// and non-zero, so a mutant that swapped `arrive_quay_at`'s road anchor, dropped a term from the
/// duration, or reported the wrong end of the crossing would move a value this test pins exactly
/// - a symmetric fixture could not tell those apart.
#[test]
fn reports_the_ferry_leg_taken_by_a_solved_crossing() {
    let mut crossing = create_ferry_crossing("crossing1", (3., 0.), (90., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 2.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 10., arr: 15. }], b_to_a: vec![] };

    let vehicle = VehicleType { shifts: vec![create_default_open_vehicle_shift()], ..create_default_vehicle_type() };
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let tour = solution.tours.first().expect("job should be assigned to a tour");
    // open shift, single job: exactly the start stop and the job stop - fixes what
    // from/to_stop_index must be below, independent of solver internals.
    assert_eq!(tour.stops.len(), 2, "expected only the start stop and the job stop");

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed leg");
    assert_eq!(ferry_legs.len(), 1, "exactly one leg crossed a ferry");

    let leg = &ferry_legs[0];
    assert_eq!(leg.vehicle_id, tour.vehicle_id);
    // pinned indices: a mutant that reported the pair (i+1, i+2), or swapped the two, changes
    // these without changing anything else observable about the solve.
    assert_eq!(leg.from_stop_index, 0);
    assert_eq!(leg.to_stop_index, 1);
    assert_eq!(leg.crossing_id, "crossing1");
    // a_to_b is the only direction with any sailings at all, so a mutant that hard-coded or
    // swapped the direction mapping can only be caught by asserting the value, not just its type.
    assert_eq!(leg.direction, FerryLegDirection::AToB);
    assert_eq!(leg.arrive_quay_at, 3.);
    assert_eq!(leg.sailing_departure, 10.);
    assert_eq!(leg.sailing_arrival, 15.);
    assert!(
        leg.sailing_departure >= leg.arrive_quay_at + crossing_boarding_buffer(),
        "the sailing caught must be at or after the vehicle reaches the quay plus the boarding buffer"
    );
}

/// Boarding buffer used by `reports_the_ferry_leg_taken_by_a_solved_crossing`, kept as one
/// constant so the buffer check can't silently drift from the fixture's own value.
fn crossing_boarding_buffer() -> f64 {
    2.
}

/// A closed shift returns the vehicle to its start, but the return sailing list is left empty -
/// the road-only return is a control leg. A mutant that reports every leg as a ferry crossing (a
/// deleted "strictly better than road" guard) or that reports none (an inverted guard) both fail
/// this test where `solves_and_checks_a_job_reached_by_crossing_a_ferry` alone could not tell
/// them apart from correct code.
#[test]
fn reports_only_the_leg_that_crossed_not_the_road_only_return() {
    let mut crossing = create_ferry_crossing("crossing1", (3., 0.), (90., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 2.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 10., arr: 15. }], b_to_a: vec![] };

    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (100., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));
    let tour = solution.tours.first().expect("job should be assigned to a tour");
    assert_eq!(tour.stops.len(), 3, "expected start, job1 and the return-to-start stop");

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed outbound leg");
    assert_eq!(ferry_legs.len(), 1, "only the outbound leg crossed; the road-only return must not be reported");
    assert_eq!(ferry_legs[0].from_stop_index, 0);
    assert_eq!(ferry_legs[0].to_stop_index, 1);
}

/// Mirrors the direction of `reports_the_ferry_leg_taken_by_a_solved_crossing`: the vehicle
/// starts on the B side and only `bToA` has any sailings. A mutant that hard-coded `aToB`, or
/// swapped the `FerryDirection` match arms when translating to the wire enum, passes the AToB
/// test above (by coincidence, since AToB is also the default-looking variant) but fails here.
#[test]
fn reports_the_b_to_a_direction_when_that_is_what_was_crossed() {
    let mut crossing = create_ferry_crossing("crossing1", (3., 0.), (90., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 2.;
    crossing.sailings = FerrySailings { a_to_b: vec![], b_to_a: vec![FerrySailing { dep: 15., arr: 20. }] };

    let vehicle = VehicleType {
        shifts: vec![VehicleShift {
            start: ShiftStart { earliest: format_time(0.), latest: None, location: (100., 0.).to_loc() },
            end: None,
            breaks: None,
            reloads: None,
            recharges: None,
            required_stops: None,
            via: None,
        }],
        ..create_default_vehicle_type()
    };
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (0., 0.))], ..create_empty_plan() },
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed leg");
    assert_eq!(ferry_legs.len(), 1);
    assert_eq!(ferry_legs[0].direction, FerryLegDirection::BToA);
    assert_eq!(ferry_legs[0].crossing_id, "crossing1");
    assert_eq!(ferry_legs[0].sailing_departure, 15.);
    assert_eq!(ferry_legs[0].sailing_arrival, 20.);
}

/// A problem with no ferry crossings at all must serialize exactly as it did before this field
/// existed. Asserted on the serialized string, not a round trip through `Option`, because a
/// mutant that emits `"ferryLegs":[]` or `"ferryLegs":null` instead of omitting the key still
/// round-trips to `None` through `#[serde(skip_serializing_if = "Option::is_none")]` on read but
/// changes the bytes actually sent over the wire.
#[test]
fn omits_ferry_legs_key_when_the_problem_has_no_crossings() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (10., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));
    assert!(solution.ferry_legs.is_none());

    let mut writer = BufWriter::new(Vec::new());
    serialize_solution(&solution, &mut writer).expect("solution should serialize");
    let json = String::from_utf8(writer.into_inner().expect("buffer")).expect("utf8 json");

    assert!(!json.contains("ferryLegs"), "a problem with no ferry crossings must not emit the ferryLegs key at all");
}
