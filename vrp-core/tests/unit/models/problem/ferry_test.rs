use super::*;
use rand::{Rng, SeedableRng, rngs::SmallRng};

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

    // a 13:20 deadline is exactly the 13:00 sailing's arrival (20 minute crossing): it still
    // qualifies (boundary inclusive), and no later sailing does.
    let arrival = min(800.);
    let path = best_arrival(&index, &zero_road, 0, 3, arrival).expect("reachable");

    assert_eq!(path.sailing_dep, min(780.));
    assert_eq!(path.arrive_quay_at, min(770.)); // mirrors the 12:50 departure boundary example

    let implied_departure = arrival - path.total_duration;
    let departure_path = best_departure(&index, &zero_road, 0, 3, implied_departure).expect("reachable");

    assert_eq!(departure_path.crossing_idx, path.crossing_idx);
    assert_eq!(departure_path.direction, path.direction);
    assert_eq!(departure_path.sailing_dep, path.sailing_dep);
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

/// Sorted-by-construction sailing list: departures strictly increase, so no explicit sort is
/// needed to satisfy `FerryCrossing::new`'s expectations.
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

#[test]
fn best_departure_is_fifo_leaving_later_never_arrives_earlier() {
    // fixed seed: a failure must be reproducible, not a one-off flake.
    let mut rng = SmallRng::seed_from_u64(20260923);

    for _ in 0..300 {
        let crossing_count = rng.gen_range(1..=2);
        let crossings = (0..crossing_count)
            .map(|i| {
                let crossing_sec = rng.gen_range(60.0..7200.0);
                let boarding_buffer_sec = rng.gen_range(0.0..1800.0);
                let a_to_b_count = rng.gen_range(0..=4);
                let b_to_a_count = rng.gen_range(0..=4);
                let a_to_b = random_sorted_sailings(&mut rng, a_to_b_count);
                let b_to_a = random_sorted_sailings(&mut rng, b_to_a_count);
                FerryCrossing::new(
                    format!("c{i}"),
                    2 * i + 1,
                    2 * i + 2,
                    crossing_sec,
                    boarding_buffer_sec,
                    a_to_b,
                    b_to_a,
                )
            })
            .collect::<Vec<_>>();
        let index = FerryIndex::new(crossings);

        // a single positive road duration shared by every approach/egress leg in this case:
        // enough to exercise the sailing-selection arithmetic without needing a per-location map.
        let road_duration = rng.gen_range(1.0..500.0);
        let road = move |_from: Location, _to: Location| -> Duration { road_duration };

        let t1 = rng.gen_range(0.0..5000.0);
        let t2 = t1 + rng.gen_range(0.0..3000.0);

        let arrival_time = |t: Timestamp| match best_departure(&index, &road, 100, 200, t) {
            Some(path) => t + path.total_duration,
            None => Duration::INFINITY,
        };

        assert!(arrival_time(t1) <= arrival_time(t2) + 1e-6, "leaving later must not arrive earlier: t1={t1} t2={t2}");
    }
}
