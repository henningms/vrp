use super::*;

fn sailing(dep: Timestamp, arr: Timestamp) -> FerrySailing {
    FerrySailing { dep, arr }
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
