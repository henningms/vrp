use crate::checker::CheckerContext;
use crate::format::problem::*;
use crate::format::solution::{FerryLegDirection, Stop, serialize_solution};
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
    let boarding_buffer_sec = 2.;
    let mut crossing = create_ferry_crossing("crossing1", (3., 0.), (90., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = boarding_buffer_sec;
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
    assert_eq!(leg.shift_index, tour.shift_index);
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
        leg.sailing_departure >= leg.arrive_quay_at + boarding_buffer_sec,
        "the sailing caught must be at or after the vehicle reaches the quay plus the boarding buffer"
    );
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

/// Builds a problem where the from-point-stop's real departure is fractional (via a vehicle
/// profile scale, not a fractional job duration - the latter trips an unrelated, pre-existing
/// checker limitation on matching activities to jobs that has nothing to do with ferries). job0's
/// arrival/departure and the approach to the quay are both scaled, so the real departure from
/// job0 is 6.5s, landing at the quay at 7.6s - a value the wire format's whole-second `"6"` could
/// never reproduce.
fn scaled_departure_problem(sailings: Vec<FerrySailing>) -> Problem {
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: sailings, b_to_a: vec![] };

    let vehicle = VehicleType {
        shifts: vec![VehicleShift {
            start: ShiftStart { earliest: format_time(0.), latest: None, location: (-5., 0.).to_loc() },
            end: None,
            breaks: None,
            reloads: None,
            recharges: None,
            required_stops: None,
            via: None,
        }],
        profile: VehicleProfile { matrix: "car".to_string(), scale: Some(1.1) },
        ..create_default_vehicle_type()
    };
    Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job0", (0., 0.)), create_delivery_job("job1", (100., 0.))],
            ..create_empty_plan()
        },
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    }
}

/// The real (6.5s) departure reaches the quay at 7.6s, missing a sailing at 7.3s and catching one
/// at 20s instead. A mutant that anchors on the truncated wire departure ("6") would compute
/// reaching the quay at 7.1s, catch the 7.3s sailing, and report that one - a real, different
/// sailing from the one the solve actually caught, not merely an off-by-a-bit timestamp.
#[test]
fn anchors_on_the_solves_exact_departure_and_catches_the_sailing_a_truncated_one_would_have_missed() {
    let problem =
        scaled_departure_problem(vec![FerrySailing { dep: 7.3, arr: 7.35 }, FerrySailing { dep: 20., arr: 25. }]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed leg");
    assert_eq!(ferry_legs.len(), 1);
    assert_eq!(ferry_legs[0].crossing_id, "crossing1");
    assert_eq!(ferry_legs[0].sailing_departure, 20., "the truncated-anchor sailing at 7.3s must not be reported");
    assert_eq!(ferry_legs[0].sailing_arrival, 25.);
    // approximate: `arrive_quay_at` is a real sum of scaled (fractional-in-f64) road distances,
    // not a clean literal like the sailing times above.
    assert!(
        (ferry_legs[0].arrive_quay_at - 7.6).abs() < 1e-6,
        "expected arrive_quay_at ~= 7.6, got {}",
        ferry_legs[0].arrive_quay_at
    );
}

/// Same real 7.6s quay arrival, but now the *only* sailing is the one at 7.3s that the real
/// departure misses entirely: the vehicle drives the road instead, so no ferry leg exists at all.
/// A mutant that anchors on the truncated wire departure ("6") would compute reaching the quay at
/// 7.1s, catch that 7.3s sailing, and report a ferry leg for a leg that actually drove the road in
/// full - a phantom crossing a consuming ride plan would splice a quay stop and boarding time into.
#[test]
fn anchors_on_the_solves_exact_departure_and_does_not_report_a_sailing_it_missed() {
    let problem = scaled_departure_problem(vec![FerrySailing { dep: 7.3, arr: 7.35 }]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    assert!(solution.ferry_legs.is_none(), "the real departure missed the only sailing; nothing may be reported");
}

/// A crossing whose only sailing is reachable and catchable, but whose total (1 approach + 25
/// crossing + 5 egress = 31s) is slower than the 30s direct road: the real solve drives.
fn slower_than_the_road_problem() -> Problem {
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (25., 0.));
    crossing.crossing_sec = 25.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 1., arr: 26. }], b_to_a: vec![] };

    let vehicle = VehicleType { shifts: vec![create_default_open_vehicle_shift()], ..create_default_vehicle_type() };
    Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (30., 0.))], ..create_empty_plan() },
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    }
}

/// A reimplementation that reports a ferry leg whenever a catchable sailing exists - rather than
/// resolving through `FerryAwareTransportCost::resolve` and trusting its own "strictly better than
/// road" comparison - would report one here: the sailing exists and the real departure catches it
/// with zero wait. Only the actual comparison against the 30s road excludes it, so this is the one
/// case that tells the real resolver apart from a timetable-presence check standing in for it.
#[test]
fn does_not_report_a_leg_whose_only_catchable_sailing_is_slower_than_the_road() {
    let problem = slower_than_the_road_problem();
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    assert!(solution.ferry_legs.is_none(), "a leg that drove the road must not be reported as a ferry leg");
}

/// The same fixture as above, but this time the point is the wire bytes: the problem carries a
/// real, attached crossing (unlike `omits_ferry_legs_key_when_the_problem_has_no_crossings`, whose
/// problem has none at all), yet nothing crosses it. Production attaches every crossing whose quay
/// falls within the problem's area, so "crossings attached, nothing taken" is the common case, not
/// the edge case - a mutant that emits the key whenever the problem *carries* crossings (rather
/// than whenever a leg actually used one) passes the no-crossings test but fails this one.
#[test]
fn omits_ferry_legs_key_when_crossings_are_attached_but_none_are_taken() {
    let problem = slower_than_the_road_problem();
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));
    assert!(solution.ferry_legs.is_none());

    let mut writer = BufWriter::new(Vec::new());
    serialize_solution(&solution, &mut writer).expect("solution should serialize");
    let json = String::from_utf8(writer.into_inner().expect("buffer")).expect("utf8 json");

    assert!(!json.contains("ferryLegs"), "crossings attached but unused must still omit the ferryLegs key");
}

/// A real solve (not a hand-built fixture) proving the writer correctly reports a leg split by a
/// required break, exercising the exact fix for finding 1 end to end. Uses
/// `solve_with_metaheuristic_and_iterations_without_check` rather than
/// `solve_with_cheapest_insertion`: a separate, pre-existing gap in `check_routing_rules`'s own
/// leg-by-leg fold rejects *any* required break with real travel before it (reproducible with no
/// ferry crossings at all - see `ferry_leg_reconciles_across_a_required_break_spliced_into_the_leg`
/// in `tests/unit/checker/routing_test.rs`), so a full checked solve of this scenario isn't
/// available today - but the solve and the writer's reporting of it are real, and reverting the
/// point-stops-only walk in `create_ferry_legs` back to a plain `windows(2)` over every stop makes
/// this test fail (the leg would silently vanish, as it did before the fix).
#[test]
fn reports_a_ferry_leg_split_by_a_required_break_from_a_real_solve() {
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (99., 0.));
    crossing.crossing_sec = 5.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 1., arr: 6. }], b_to_a: vec![] };

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

    let solution = solve_with_metaheuristic_and_iterations_without_check(problem, Some(vec![matrix]), 100);

    let tour = solution.tours.first().expect("job should be assigned to a tour");
    assert!(
        matches!(tour.stops.get(1), Some(Stop::Transit(_))),
        "the break must have split the leg's display into a transit stop at index 1"
    );

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed leg");
    assert_eq!(ferry_legs.len(), 1, "exactly one leg crossed a ferry");
    assert_eq!(ferry_legs[0].from_stop_index, 0);
    assert_eq!(ferry_legs[0].to_stop_index, 2, "must skip the spliced transit stop, not name it");
    assert_eq!(ferry_legs[0].crossing_id, "crossing1");
    assert_eq!(ferry_legs[0].direction, FerryLegDirection::AToB);
}

/// The cost model charges `crossing.crossingSec`, not the sailing's own `arr - dep`: this
/// crossing's sailing spans 9s (2 -> 11) while the crossing itself takes only 4s - exactly what
/// production timetables look like (thirty-three real crossings carry sailings of differing
/// lengths, since the crossing duration sent is a single number per crossing, not derived from
/// whichever sailing is actually caught). Before deriving the expected next-stop arrival from
/// `sailingDeparture + crossingSec` instead of `sailingArrival`, the checker would have computed
/// 11 + 1 = 12 instead of the real 2 + 4 + 1 = 7 and rejected this correct solve outright.
#[test]
fn solves_and_checks_a_crossing_whose_sailing_span_differs_from_its_crossing_duration() {
    let mut crossing = create_ferry_crossing("crossing1", (1., 0.), (9., 0.));
    crossing.crossing_sec = 4.;
    crossing.boarding_buffer_sec = 0.;
    crossing.sailings = FerrySailings { a_to_b: vec![FerrySailing { dep: 2., arr: 11. }], b_to_a: vec![] };

    let vehicle = VehicleType { shifts: vec![create_default_open_vehicle_shift()], ..create_default_vehicle_type() };
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", (10., 0.))], ..create_empty_plan() },
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ferry_crossings: Some(vec![crossing]),
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    // `solve_with_cheapest_insertion` runs the checker and panics on rejection - that panic, not
    // an assertion here, is what proves the fix.
    let solution = solve_with_cheapest_insertion(problem, Some(vec![matrix]));

    let ferry_legs = solution.ferry_legs.as_ref().expect("solution should report the crossed leg");
    assert_eq!(ferry_legs.len(), 1);
    assert_eq!(ferry_legs[0].sailing_departure, 2.);
    assert_eq!(ferry_legs[0].sailing_arrival, 11.);
}

/// A leg whose elapsed time the road alone cannot explain must carry a `FerryLeg` entry: takes a
/// real, valid, checked solve and simply deletes its one reported leg, then asserts the checker
/// rejects the result. Before `check_ferry_legs_rules` walked every tour's own point-stop pairs
/// (rather than only validating whatever entries happened to be present), a missing entry was
/// invisible to the checker - which is exactly how finding 1 escaped in the first place.
#[test]
fn rejects_a_solution_missing_the_ferry_leg_entry_for_a_leg_that_crossed() {
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

    let mut solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));
    assert!(solution.ferry_legs.is_some(), "sanity: the real solve must have crossed the ferry");
    solution.ferry_legs = None;

    let core_problem = Arc::new((problem.clone(), vec![matrix.clone()]).read_pragmatic().expect("valid problem"));
    let ctx = CheckerContext::new(core_problem, problem, Some(vec![matrix]), solution).expect("valid context");

    assert!(ctx.check().is_err(), "a leg the road alone cannot explain must be reported, not silently dropped");
}

/// Nothing ties `sailingDeparture`/`sailingArrival` back to the timetable except this check: takes
/// a real, valid, checked solve and corrupts its reported sailing to a pair of timestamps that
/// exist in neither the crossing's `aToB` nor `bToA` lists, then asserts rejection.
#[test]
fn rejects_a_ferry_leg_reporting_a_sailing_not_in_the_timetable() {
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

    let mut solution = solve_with_cheapest_insertion(problem.clone(), Some(vec![matrix.clone()]));
    let leg = solution.ferry_legs.as_mut().expect("solution should report the crossed leg").first_mut().unwrap();
    assert_eq!(leg.sailing_departure, 1., "sanity: the real solve caught the only sailing, departing at 1s");
    // leave `sailing_departure` real (1s, matched above) and corrupt only `sailing_arrival`: the
    // next-stop-arrival formula is `sailing_departure + crossing.crossingSec`, so it never reads
    // `sailing_arrival` at all - corrupting both fields (as an earlier version of this test did)
    // lets the arrival-time check catch the corruption first, leaving the timetable guard this
    // test names unpinned. No sailing on this crossing ever arrives at 999999s.
    leg.sailing_arrival = 999999.;

    let core_problem = Arc::new((problem.clone(), vec![matrix.clone()]).read_pragmatic().expect("valid problem"));
    let ctx = CheckerContext::new(core_problem, problem, Some(vec![matrix]), solution).expect("valid context");

    assert!(ctx.check().is_err(), "an invented sailing must be rejected, not accepted on its say-so");
}

// `rejects_a_ferry_leg_whose_sailing_departs_before_the_vehicle_can_board` (the boarding-buffer
// guard test) moved to `tests/unit/checker/routing_test.rs`: isolating it needs a hand-built,
// self-consistent fixture that calls `check_ferry_legs_rules` directly, because `check_routing`'s
// unrelated leg-by-leg fold independently re-derives the correct sailing from the crossing-aware
// transport cost and rejects a swapped-sailing shape on its own terms - see that test's doc for
// why a real solve (which would just catch the correct, boardable sailing) can't reproduce this.
