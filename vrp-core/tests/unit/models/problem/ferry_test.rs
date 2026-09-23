use super::*;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::collections::HashMap;

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

#[test]
fn best_arrival_picks_the_latest_sailing_that_still_makes_the_deadline_and_round_trips() {
    let index = FerryIndex::new(vec![half_hourly_crossing()]);

    // 14:00 deadline: ten minutes of slack past the 13:30 sailing's 13:50 arrival. A zero-slack
    // deadline (e.g. exactly 13:50) would make the round trip hold by accident, since there'd be
    // only one departure consistent with catching that sailing at all.
    let arrival = min(840.);
    let path = best_arrival(&index, &zero_road, 0, 3, arrival).expect("reachable");

    assert_eq!(path.sailing_dep, min(810.));
    assert_eq!(path.depart_at, min(800.)); // 13:20 - mirrors the 12:50 departure boundary example
    assert_eq!(path.total_duration, min(40.)); // arrival - depart_at, includes the ten minutes of slack

    let departure_path = best_departure(&index, &zero_road, 0, 3, path.depart_at).expect("reachable");

    // the round trip criterion is the same sailing, not equal totals: best_departure's total is
    // the journey's own length, which legitimately differs from best_arrival's arrival-anchored
    // total by exactly the slack to the deadline (40 min vs 30 min here).
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
