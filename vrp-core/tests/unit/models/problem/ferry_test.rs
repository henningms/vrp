use super::*;
use crate::helpers::models::solution::test_actor_with_profile;
use crate::models::solution::Route;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::collections::HashMap;
use std::sync::Arc;

fn sailing(dep: Timestamp, arr: Timestamp) -> FerrySailing {
    FerrySailing { dep, arr }
}

/// Minutes-since-midnight to seconds, so test fixtures can read as clock times.
fn min(m: f64) -> Timestamp {
    m * 60.
}

/// Sailings every 30 minutes from 11:00 to 15:00, a 20 minute crossing each way.
fn half_hourly_sailings() -> Vec<FerrySailing> {
    (22..=30)
        .map(|half_hours: i64| {
            let dep = min((half_hours * 30) as f64);
            sailing(dep, dep + min(20.))
        })
        .collect::<Vec<_>>()
}

/// A crossing with a sailing every 30 minutes from 11:00 to 15:00 in both directions, a 20
/// minute crossing and a 10 minute boarding buffer - the fixture the brief's boundary examples
/// are built on.
fn half_hourly_crossing() -> FerryCrossing {
    FerryCrossing::new("c1".to_string(), 1, 2, min(20.), min(10.), half_hourly_sailings(), half_hourly_sailings())
}

/// A `road` closure with no approach/egress cost, so a test's `departure`/`arrival` value maps
/// directly onto quay arrival/sailing-arrival without an offset to account for.
fn zero_road(_from: Location, _to: Location) -> Duration {
    0.
}

#[test]
fn sailings_returns_the_list_for_the_requested_direction() {
    // distinct dep/arr per direction so swapping the match arms in `sailings` fails this test
    let crossing = FerryCrossing::new(
        "crossing1".to_string(),
        1,
        2,
        1500.,
        600.,
        vec![sailing(0., 900.)],
        vec![sailing(1800., 2700.)],
    );

    let a_to_b = crossing.sailings(FerryDirection::AToB);
    assert_eq!(a_to_b.len(), 1);
    assert_eq!(a_to_b[0].dep, 0.);
    assert_eq!(a_to_b[0].arr, 900.);

    let b_to_a = crossing.sailings(FerryDirection::BToA);
    assert_eq!(b_to_a.len(), 1);
    assert_eq!(b_to_a[0].dep, 1800.);
    assert_eq!(b_to_a[0].arr, 2700.);
}

#[test]
fn new_sorts_both_sailing_directions_by_departure_ascending() {
    let crossing = FerryCrossing::new(
        "crossing1".to_string(),
        1,
        2,
        1500.,
        600.,
        vec![sailing(3600., 4500.), sailing(0., 900.), sailing(1800., 2700.)],
        vec![sailing(5400., 6300.), sailing(900., 1800.), sailing(2700., 3600.)],
    );

    let a_to_b_deps = crossing.sailings(FerryDirection::AToB).iter().map(|s| s.dep).collect::<Vec<_>>();
    assert_eq!(a_to_b_deps, vec![0., 1800., 3600.]);

    let b_to_a_deps = crossing.sailings(FerryDirection::BToA).iter().map(|s| s.dep).collect::<Vec<_>>();
    assert_eq!(b_to_a_deps, vec![900., 2700., 5400.]);
}

#[test]
fn new_drops_inverted_sailings_in_every_build_not_just_debug() {
    // an inverted sailing (arr < dep) is nonsense, and best_arrival's lower-bound prune depends
    // on arr >= dep holding for real - a debug_assert alone would compile out in release, which
    // is what the solver ships, so this must be a real filter, not only an assertion.
    let crossing = FerryCrossing::new(
        "crossing1".to_string(),
        1,
        2,
        1500.,
        600.,
        vec![sailing(0., 900.), sailing(1000., 500.)], // second one inverted: arrives before it departs
        vec![sailing(1800., 1700.)],                   // inverted
    );

    let a_to_b = crossing.sailings(FerryDirection::AToB);
    assert_eq!(a_to_b.len(), 1);
    assert_eq!(a_to_b[0].dep, 0.);

    assert!(crossing.sailings(FerryDirection::BToA).is_empty());
}

#[test]
fn unreachable_duration_threshold_is_a_large_finite_seconds_value() {
    // doc claims: finite (not infinity, so it round-trips through JSON and downstream duration
    // arithmetic stays defined) and, at 1e9 seconds (~31.7 years), far larger than any real
    // driving/crossing duration a problem could legitimately contain.
    assert!(UNREACHABLE_DURATION_THRESHOLD.is_finite());
    assert_eq!(UNREACHABLE_DURATION_THRESHOLD, 1e9);
}

#[test]
fn best_departure_boundary_examples_from_the_brief() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);

    // reaching the quay at 12:50 with a 10 minute buffer needs dep >= 13:00, so it catches the
    // 13:00 sailing exactly (boundary is inclusive), not the preceding 12:30 one.
    let reaching_1250 = best_departure(&index, &zero_road, 0, 3, min(770.)).expect("reachable");
    assert_eq!(reaching_1250.sailing_dep, min(780.));

    // one minute later, the same buffer pushes the earliest-boardable time past 13:00, so the
    // vehicle misses it and catches the next sailing, 13:30, instead.
    let reaching_1251 = best_departure(&index, &zero_road, 0, 3, min(771.)).expect("reachable");
    assert_eq!(reaching_1251.sailing_dep, min(810.));
}

#[test]
fn best_departure_returns_none_when_all_sailings_have_departed() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);

    // departing at 15:00 pushes the earliest-boardable time past the last sailing (15:00).
    let result = best_departure(&index, &zero_road, 0, 3, min(900.));

    assert!(result.is_none());
}

#[test]
fn best_departure_returns_none_when_approach_road_is_unreachable() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);
    // blocks the approach leg from `from` to either quay, in either direction.
    let road =
        |from: Location, _to: Location| -> Duration { if from == 0 { UNREACHABLE_DURATION_THRESHOLD } else { 0. } };

    let result = best_departure(&index, &road, 0, 3, min(770.));

    assert!(result.is_none());
}

#[test]
fn best_departure_prefers_the_crossing_with_the_smaller_total() {
    let slow = FerryCrossing::new(
        "slow".to_string(),
        1,
        2,
        min(50.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let fast = FerryCrossing::new(
        "fast".to_string(),
        3,
        4,
        min(20.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let index = FerryIndex::new(vec![slow, fast]);

    let result = best_departure(&index, &zero_road, 0, 5, min(770.)).expect("reachable");

    assert_eq!(result.crossing_idx, 1);
    assert_eq!(result.total_duration, min(30.)); // 10 min wait + 20 min crossing
}

/// 7 minutes from `from` to either quay, nothing on the egress leg. A zero-road fixture makes
/// `depart_at` (leaves `from`) and `arrive_quay_at` (reaches the quay) numerically identical,
/// which would let a mutant conflate the two survive every test; this tells them apart.
fn seven_minute_approach_road(from: Location, _to: Location) -> Duration {
    if from == 0 { min(7.) } else { 0. }
}

#[test]
fn best_arrival_picks_the_latest_sailing_that_still_makes_the_deadline_and_round_trips() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);

    // 14:00 deadline: ten minutes of slack past the 13:30 sailing's 13:50 arrival. A zero-slack
    // deadline (e.g. exactly 13:50) would make the round trip hold by accident, since there'd be
    // only one departure consistent with catching that sailing at all.
    let arrival = min(840.);
    let path = best_arrival(&index, &seven_minute_approach_road, 0, 3, arrival).expect("reachable");

    assert_eq!(path.sailing_dep, min(810.));
    assert_eq!(path.arrive_quay_at, min(800.)); // 13:20 - mirrors the 12:50 departure boundary example
    // depart_at is 7 minutes before arrive_quay_at (the approach), not the same instant; a
    // mutant setting depart_at = arrive_quay_at would report this vehicle leaving `from` at
    // 13:20 instead of 13:13, seven minutes late.
    assert_eq!(path.depart_at, min(793.)); // 13:13 = arrive_quay_at - the 7 minute approach
    assert_ne!(path.depart_at, path.arrive_quay_at);
    assert_eq!(path.total_duration, min(47.)); // arrival - depart_at, includes the ten minutes of slack

    let departure_path = best_departure(&index, &seven_minute_approach_road, 0, 3, path.depart_at).expect("reachable");

    // the round trip criterion is the same sailing, not equal totals: best_departure's total is
    // the journey's own length, which legitimately differs from best_arrival's arrival-anchored
    // total by exactly the slack to the deadline (47 min vs 37 min here).
    assert_eq!(departure_path.crossing_idx, path.crossing_idx);
    assert_eq!(departure_path.direction, path.direction);
    assert_eq!(departure_path.sailing_dep, path.sailing_dep);
    assert_eq!(path.total_duration - departure_path.total_duration, min(10.));
}

#[test]
fn best_arrival_prefers_the_crossing_that_allows_leaving_latest_not_the_shortest_journey() {
    // a short crossing with one infrequent sailing can force a far earlier departure than a
    // longer crossing that sails often; the useful answer for a deadline is whichever crossing
    // lets the vehicle leave `from` latest, not whichever has the shorter total journey.
    let short_infrequent = FerryCrossing::new(
        "short-infrequent".to_string(),
        1,
        2,
        min(20.),
        0.,
        vec![sailing(min(660.), min(680.))], // the only sailing: 11:00
        vec![],
    );
    let long_frequent = FerryCrossing::new(
        "long-frequent".to_string(),
        3,
        4,
        min(60.),
        0.,
        (0..=24).map(|i| min(660. + (i as f64) * 5.)).map(|dep| sailing(dep, dep + min(60.))).collect(),
        vec![],
    );
    let index = FerryIndex::new(vec![short_infrequent, long_frequent]);

    // deadline 13:35: short_infrequent forces leaving by 11:00 (depart_at); long_frequent's
    // 12:35 sailing (13:35 arrival) allows leaving 95 minutes later.
    let result = best_arrival(&index, &zero_road, 0, 5, min(815.)).expect("reachable");

    assert_eq!(result.crossing_idx, 1);
    assert_eq!(result.depart_at, min(755.)); // 12:35, not short_infrequent's 11:00
}

#[test]
fn best_arrival_finds_the_latest_departure_even_when_arrivals_are_not_in_departure_order() {
    // a deliberately non-monotone timetable: sorted by dep (as `FerryCrossing::new` requires),
    // but arr does not track it - real timetables aren't guaranteed to either (see review
    // finding 4). A binary search on arr over this dep-sorted list would be unsound; only a
    // backward scan is exact here.
    let crossing = FerryCrossing::new(
        "scrambled".to_string(),
        1,
        2,
        min(20.),
        0.,
        vec![sailing(100., 600.), sailing(200., 300.), sailing(300., 700.), sailing(400., 450.)],
        vec![],
    );
    let index = FerryIndex::new(vec![crossing]);

    // two sailings meet the 500 deadline (dep 200 -> arr 300, and dep 400 -> arr 450); the
    // later-departing one must win.
    let result = best_arrival(&index, &zero_road, 0, 3, 500.).expect("reachable");

    assert_eq!(result.sailing_dep, 400.);
}

#[test]
fn best_arrival_requires_egress_to_meet_the_deadline_not_just_the_sailing() {
    // egress (far quay -> destination) must count toward the deadline, not just the sailing's
    // own arrival. A 20 minute crossing, sailings every 30 minutes, and a 30 minute egress leg.
    let index = FerryIndex::new(vec![half_hourly_crossing()]);
    let road = |_from: Location, to: Location| -> Duration { if to == 3 { min(30.) } else { 0. } };

    // 14:10 deadline: the 13:30 sailing (arr 13:50) plus 30 min egress lands at 14:20, too late;
    // the 13:00 sailing (arr 13:20) plus egress lands at 13:50, within the deadline. A mutant
    // dropping the `+ egress` term from the feasibility check (`ferry.rs`'s
    // `sailing.arr + egress <= arrival`) would consider 13:50 <= 14:10 and wrongly pick the
    // later-departing 13:30 sailing instead - still monotone in the deadline, so both property
    // tests would miss it too.
    let result = best_arrival(&index, &road, 0, 3, min(850.)).expect("reachable");

    assert_eq!(result.sailing_dep, min(780.)); // 13:00, not the 13:30 a missing egress term would pick
}

#[test]
fn best_arrival_returns_none_when_approach_road_is_unreachable() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);
    // blocks the approach leg from `from` to either quay. Unlike egress, an unreachable
    // approach does not fail the arrival-side feasibility check on its own (it only tests
    // `sailing.arr + egress <= arrival`, never `approach`), so without best_path's sentinel
    // guard this would silently return a `Some` path with `depart_at` pushed about a billion
    // seconds into the past instead of `None`.
    let road =
        |from: Location, _to: Location| -> Duration { if from == 0 { UNREACHABLE_DURATION_THRESHOLD } else { 0. } };

    let result = best_arrival(&index, &road, 0, 3, min(800.));

    assert!(result.is_none());
}

#[test]
fn best_departure_returns_none_when_egress_road_is_unreachable() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);
    // blocks the egress leg from either quay to `to`. Unlike approach, an unreachable egress
    // does not fail the departure-side sailing search on its own (the search only uses
    // `approach`; egress is added into `total_duration` afterwards), so without best_path's
    // sentinel guard this would silently return a `Some` path with a total around a billion
    // seconds instead of `None`. (The existing approach-unreachable test for this function
    // passes regardless of the guard, since a sentinel approach pushes `earliest` past every
    // sailing on its own - this is the counterpart that actually exercises it.)
    let road =
        |_from: Location, to: Location| -> Duration { if to == 3 { UNREACHABLE_DURATION_THRESHOLD } else { 0. } };

    let result = best_departure(&index, &road, 0, 3, min(770.));

    assert!(result.is_none());
}

#[test]
fn best_departure_prune_recheck_keeps_the_true_best_not_whichever_cleared_the_bound_last() {
    // crossing 0: a 30 minute crossing whose sailing is 10 minutes away (total 40 min).
    // crossing 1: a 20 minute crossing whose sailing is an hour away (total 80 min). Crossing
    // 1's zero-wait bound (20 min) clears the prune against crossing 0's already-found 40 min
    // total, so without re-checking the *actual* total once the real wait is known, this would
    // wrongly overwrite the best candidate with the worse one.
    let near_but_slow =
        FerryCrossing::new("near-but-slow".to_string(), 1, 2, min(30.), 0., vec![sailing(min(10.), min(40.))], vec![]);
    let far_but_fast =
        FerryCrossing::new("far-but-fast".to_string(), 3, 4, min(20.), 0., vec![sailing(min(60.), min(80.))], vec![]);
    let index = FerryIndex::new(vec![near_but_slow, far_but_fast]);

    let result = best_departure(&index, &zero_road, 0, 5, min(0.)).expect("reachable");

    assert_eq!(result.crossing_idx, 0);
    assert_eq!(result.total_duration, min(40.));
}

#[test]
fn best_departure_skips_a_degenerate_crossing_whose_quays_are_the_same_location() {
    let degenerate = FerryCrossing::new(
        "degenerate".to_string(),
        7,
        7,
        min(20.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let index = FerryIndex::new(vec![degenerate]);

    let result = best_departure(&index, &zero_road, 0, 3, min(770.));

    assert!(result.is_none());
}

#[test]
fn best_departure_skips_degenerate_crossing_but_still_finds_a_normal_one() {
    let degenerate = FerryCrossing::new(
        "degenerate".to_string(),
        7,
        7,
        min(20.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let index = FerryIndex::new(vec![degenerate, half_hourly_crossing()]);

    let result = best_departure(&index, &zero_road, 0, 3, min(770.)).expect("normal crossing still reachable");

    assert_eq!(result.crossing_idx, 1);
}

#[test]
fn best_arrival_skips_a_degenerate_crossing_whose_quays_are_the_same_location() {
    let degenerate = FerryCrossing::new(
        "degenerate".to_string(),
        7,
        7,
        min(20.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let index = FerryIndex::new(vec![degenerate]);

    let result = best_arrival(&index, &zero_road, 0, 3, min(800.));

    assert!(result.is_none());
}

#[test]
fn best_arrival_skips_degenerate_crossing_but_still_finds_a_normal_one() {
    let degenerate = FerryCrossing::new(
        "degenerate".to_string(),
        7,
        7,
        min(20.),
        min(10.),
        half_hourly_sailings(),
        half_hourly_sailings(),
    );
    let index = FerryIndex::new(vec![degenerate, half_hourly_crossing()]);

    let result = best_arrival(&index, &zero_road, 0, 3, min(800.)).expect("normal crossing still reachable");

    assert_eq!(result.crossing_idx, 1);
}

/// Sorted-by-construction sailing list: departures strictly increase, so no explicit sort is
/// needed to satisfy `FerryCrossing::new`'s expectations. `arr` is a positive offset from `dep`
/// drawn independently of the previous sailing's `arr`, so consecutive sailings are not
/// guaranteed to be monotone in `arr` - real timetables aren't either (see
/// `best_arrival_finds_the_latest_departure_even_when_arrivals_are_not_in_departure_order`).
fn random_sorted_sailings(rng: &mut SmallRng, count: usize) -> Vec<FerrySailing> {
    let mut dep = rng.gen_range(0.0..2000.0);
    (0..count)
        .map(|_| {
            dep += rng.gen_range(1.0..600.0);
            let arr = dep + rng.gen_range(60.0..3600.0);
            sailing(dep, arr)
        })
        .collect()
}

/// The fixed `from`/`to` locations used by every generated property-test case; every crossing's
/// quay ids are chosen well clear of these.
const RANDOM_CASE_FROM: Location = 0;
const RANDOM_CASE_TO: Location = 1;

/// Builds a random ferry index (1-3 crossings, a handful of sailings each direction, a per-quay
/// leg road duration - occasionally the unreachable sentinel - so approach and egress are never
/// forced equal) plus the span of departure times its sailings cover, so a caller can sample
/// queries that mostly land inside the timetable instead of past every sailing.
fn random_case(
    rng: &mut SmallRng,
) -> (FerryIndex, impl Fn(Location, Location) -> Duration + use<>, Timestamp, Timestamp) {
    let crossing_count = rng.gen_range(1..=3);
    let mut crossings = Vec::with_capacity(crossing_count);
    let mut road_matrix: HashMap<(Location, Location), Duration> = HashMap::new();
    let mut span_start = Timestamp::INFINITY;
    let mut span_end = Timestamp::NEG_INFINITY;

    for i in 0..crossing_count {
        let quay_a = 100 + i * 10 + 1;
        let quay_b = 100 + i * 10 + 2;

        let a_to_b_count = rng.gen_range(1..=6);
        let b_to_a_count = rng.gen_range(1..=6);
        let a_to_b = random_sorted_sailings(rng, a_to_b_count);
        let b_to_a = random_sorted_sailings(rng, b_to_a_count);
        for s in a_to_b.iter().chain(b_to_a.iter()) {
            span_start = span_start.min(s.dep);
            span_end = span_end.max(s.dep);
        }

        let crossing_sec = rng.gen_range(60.0..7200.0);
        let boarding_buffer_sec = rng.gen_range(0.0..1800.0);
        crossings.push(FerryCrossing::new(
            format!("c{i}"),
            quay_a,
            quay_b,
            crossing_sec,
            boarding_buffer_sec,
            a_to_b,
            b_to_a,
        ));

        for pair in
            [(RANDOM_CASE_FROM, quay_a), (RANDOM_CASE_FROM, quay_b), (quay_a, RANDOM_CASE_TO), (quay_b, RANDOM_CASE_TO)]
        {
            // 10% unreachable: exercises the sentinel-skip path inside the property, not just a
            // dedicated unit test.
            let duration = if rng.gen_bool(0.1) { UNREACHABLE_DURATION_THRESHOLD } else { rng.gen_range(1.0..500.0) };
            road_matrix.insert(pair, duration);
        }
    }

    let index = FerryIndex::new(crossings);
    let road = move |a: Location, b: Location| -> Duration { *road_matrix.get(&(a, b)).unwrap_or(&0.0) };

    (index, road, span_start, span_end)
}

#[test]
fn best_departure_is_fifo_leaving_later_never_arrives_earlier() {
    // fixed seed: a failure must be reproducible, not a one-off flake.
    let mut rng = SmallRng::seed_from_u64(20260923);

    for _ in 0..300 {
        let (index, road, span_start, span_end) = random_case(&mut rng);

        // sampled inside (and a little around) the timetable's own span, so most cases compare
        // two real ferry paths rather than two `None`s.
        let t1 = rng.gen_range((span_start - 600.0)..=(span_end + 600.0));
        let t2 = t1 + rng.gen_range(0.0..900.0);

        let arrival_time = |t: Timestamp| match best_departure(&index, &road, RANDOM_CASE_FROM, RANDOM_CASE_TO, t) {
            Some(path) => t + path.total_duration,
            None => Duration::INFINITY,
        };

        assert!(arrival_time(t1) <= arrival_time(t2) + 1e-6, "leaving later must not arrive earlier: t1={t1} t2={t2}");
    }
}

#[test]
fn best_arrival_is_monotone_in_the_deadline_a_later_deadline_never_forces_an_earlier_departure() {
    // the mirror of the FIFO property above, for the backward variant: relaxing (raising) the
    // deadline must never force leaving earlier. This is exactly the property whose violation
    // review finding 3 reproduced (a 2-ULP tie in a re-derived wait silently kept a worse
    // candidate); `select` now returns its own `total_duration` with no re-derivation, so this
    // guards against that class of bug regressing.
    let mut rng = SmallRng::seed_from_u64(20260924);

    for _ in 0..300 {
        let (index, road, span_start, span_end) = random_case(&mut rng);

        // padded well past the last departure so a deadline can plausibly fall after a sailing's
        // crossing (up to 7200s) plus egress (up to 500) plus buffer (up to 1800).
        let a1 = rng.gen_range((span_start - 600.0)..=(span_end + 9600.0));
        let a2 = a1 + rng.gen_range(0.0..900.0);

        let latest_departure = |a: Timestamp| match best_arrival(&index, &road, RANDOM_CASE_FROM, RANDOM_CASE_TO, a) {
            Some(path) => path.depart_at,
            None => Duration::NEG_INFINITY,
        };

        assert!(
            latest_departure(a1) <= latest_departure(a2) + 1e-6,
            "a later deadline must never force an earlier departure: a1={a1} a2={a2}"
        );
    }
}

mod ferry_aware_transport_cost {
    use super::*;
    use crate::models::problem::SimpleTransportCost;

    /// Builds a 4-location `SimpleTransportCost` (0=from, 1=quayA, 2=quayB, 3=to) with a fixed
    /// approach (from->quayA, 300s/53) and egress (quayB->to, 600s/89) leg and the given direct
    /// from->to road duration/distance.
    ///
    /// `one_way_crossing`'s B-to-A direction carries no sailings, so a real (timetable-aware)
    /// query always excludes it on its own - but `duration_approx`/`distance_approx` have no
    /// timetable to check, so without deliberately padding B-to-A's own approach/egress
    /// (from->quayB, quayA->to) durations to 5000, that direction's zero-wait bound (0 + 1200
    /// crossing + 0 = 1200) would look cheaper than A-to-B's real one (2100) - not a bug in the
    /// wrapper, just the zero-wait bound being direction-blind by design, but it would make a
    /// fixture built with the road matrix's untouched-entries default of 0 silently assert on the
    /// wrong direction. The same two entries double as "trap" distance values (997, 991): only a
    /// mutant reading the wrong quay for the chosen (A-to-B) direction would ever read them for
    /// distance, since the real distance path never visits quayB from `from` or `to` from quayA.
    fn ferry_test_transport(direct_duration: Duration, direct_distance: Distance) -> Arc<dyn TransportCost> {
        let size = 4;
        let at = |from: usize, to: usize| from * size + to;
        let mut durations = vec![0.; size * size];
        let mut distances = vec![0.; size * size];

        durations[at(0, 1)] = 300.;
        durations[at(2, 3)] = 600.;
        durations[at(0, 3)] = direct_duration;
        durations[at(0, 2)] = 5000.; // from -> quayB: keeps B-to-A's zero-wait bound from winning
        durations[at(1, 3)] = 5000.; // quayA -> to: same, for the egress side

        distances[at(0, 1)] = 53.;
        distances[at(2, 3)] = 89.;
        distances[at(0, 3)] = direct_distance;
        distances[at(0, 2)] = 997.; // trap: from -> quayB
        distances[at(1, 3)] = 991.; // trap: quayA -> to

        Arc::new(SimpleTransportCost::new(durations, distances).expect("valid matrix"))
    }

    /// A single A-to-B crossing (quays 1 and 2, matching `ferry_test_transport`'s quays) with one
    /// sailing and no B-to-A service - the empty direction lets a test's numbers describe the
    /// A-to-B leg unambiguously, since `best_path` tries both directions but B-to-A always finds
    /// no sailing regardless of road values.
    fn one_way_crossing(
        crossing_sec: Duration,
        boarding_buffer_sec: Duration,
        sailing_dep: Timestamp,
        sailing_arr: Timestamp,
    ) -> FerryIndex {
        let crossing = FerryCrossing::new(
            "aware".to_string(),
            1,
            2,
            crossing_sec,
            boarding_buffer_sec,
            vec![sailing(sailing_dep, sailing_arr)],
            vec![],
        );
        FerryIndex::new(vec![crossing])
    }

    fn test_route() -> Route {
        Route { actor: test_actor_with_profile(0), tour: Default::default() }
    }

    #[test]
    fn duration_prefers_inner_when_road_is_faster() {
        // direct road (500s) beats every ferry option (>=2100s even with zero wait), so duration
        // must equal the plain road duration - a mutant that always returns the ferry total would
        // report 2100 here instead.
        let inner = ferry_test_transport(500., 500.);
        let index = Arc::new(one_way_crossing(min(20.), 0., min(5.), min(25.)));
        let ferry_aware = FerryAwareTransportCost::new(inner.clone(), index);
        let route = test_route();

        let duration = ferry_aware.duration(&route, 0, 3, TravelTime::Departure(0.));

        assert_eq!(duration, inner.duration(&route, 0, 3, TravelTime::Departure(0.)));
        assert_eq!(duration, 500.);
    }

    #[test]
    fn duration_prefers_ferry_when_it_wins() {
        // ferry total: 300 approach + 0 wait (sailing departs exactly at quay arrival) + 1200
        // crossing + 600 egress = 2100s, far under the 5000s direct road leg - a wrapper that
        // never consults the ferry index would report 5000 here.
        let inner = ferry_test_transport(5000., 5000.);
        let index = Arc::new(one_way_crossing(min(20.), 0., min(5.), min(25.)));
        let ferry_aware = FerryAwareTransportCost::new(inner, index);
        let route = test_route();

        let duration = ferry_aware.duration(&route, 0, 3, TravelTime::Departure(0.));

        assert_eq!(duration, 2100.);
    }

    #[test]
    fn distance_sums_the_approach_and_egress_of_the_crossing_duration_chose() {
        // same winning ferry path as `duration_prefers_ferry_when_it_wins`; distance must equal
        // that path's own approach+egress (53+89), never the direct road distance (5000, a
        // ferry-blind mutant) and never the trap entries (997/991, a swapped-quay mutant).
        let inner = ferry_test_transport(5000., 5000.);
        let index = Arc::new(one_way_crossing(min(20.), 0., min(5.), min(25.)));
        let ferry_aware = FerryAwareTransportCost::new(inner, index);
        let route = test_route();

        let distance = ferry_aware.distance(&route, 0, 3, TravelTime::Departure(0.));

        assert_eq!(distance, 53. + 89.);
    }

    #[test]
    fn duration_approx_is_the_zero_wait_total_and_never_exceeds_duration() {
        // the sailing (900s) is well after the quay is reached (300s), forcing a 600s real wait;
        // duration_approx must ignore the timetable and report the zero-wait total (300 approach +
        // 1200 crossing + 600 egress = 2100), strictly less than the real, wait-inclusive duration
        // (2700) - a mutant that drops the crossing_sec term, or that reuses duration()'s
        // timetable lookup instead of a zero-wait bound, would report something other than these
        // exact numbers.
        let inner = ferry_test_transport(5000., 5000.);
        let index = Arc::new(one_way_crossing(min(20.), 0., min(15.), min(35.)));
        let ferry_aware = FerryAwareTransportCost::new(inner, index);
        let route = test_route();
        let profile = route.actor.vehicle.profile.clone();

        let duration_approx = ferry_aware.duration_approx(&profile, 0, 3);
        let duration = ferry_aware.duration(&route, 0, 3, TravelTime::Departure(0.));

        assert_eq!(duration_approx, 2100.);
        assert_eq!(duration, 2700.);
        assert!(duration_approx <= duration);
    }

    #[test]
    fn distance_approx_sums_the_approach_and_egress_of_the_crossing_duration_approx_chose() {
        let inner = ferry_test_transport(5000., 5000.);
        let index = Arc::new(one_way_crossing(min(20.), 0., min(5.), min(25.)));
        let ferry_aware = FerryAwareTransportCost::new(inner, index);
        let profile = Profile::default();

        let distance_approx = ferry_aware.distance_approx(&profile, 0, 3);

        assert_eq!(distance_approx, 53. + 89.);
    }

    #[test]
    fn empty_index_leaves_every_method_equal_to_inner() {
        let inner = ferry_test_transport(500., 400.);
        let index = Arc::new(FerryIndex::default());
        let ferry_aware = FerryAwareTransportCost::new(Arc::clone(&inner), index);
        let route = test_route();
        let profile = route.actor.vehicle.profile.clone();

        assert_eq!(
            ferry_aware.duration(&route, 0, 3, TravelTime::Departure(0.)),
            inner.duration(&route, 0, 3, TravelTime::Departure(0.))
        );
        assert_eq!(
            ferry_aware.distance(&route, 0, 3, TravelTime::Departure(0.)),
            inner.distance(&route, 0, 3, TravelTime::Departure(0.))
        );
        assert_eq!(ferry_aware.duration_approx(&profile, 0, 3), inner.duration_approx(&profile, 0, 3));
        assert_eq!(ferry_aware.distance_approx(&profile, 0, 3), inner.distance_approx(&profile, 0, 3));
        assert_eq!(ferry_aware.size(), inner.size());
    }

    #[test]
    fn ferry_transport_extra_property_round_trips() {
        let road: Arc<dyn TransportCost> = ferry_test_transport(0., 0.);
        let index = Arc::new(FerryIndex::default());
        let mut extras = Extras::default();

        extras.set_ferry_transport(Arc::new(FerryTransport { index: Arc::clone(&index), road: Arc::clone(&road) }));

        let stored = extras.get_ferry_transport().expect("ferry transport stored");
        assert!(Arc::ptr_eq(&stored.road, &road));
        assert!(Arc::ptr_eq(&stored.index, &index));
    }
}
