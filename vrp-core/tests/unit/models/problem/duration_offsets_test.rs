use super::*;
use crate::helpers::models::problem::{clock, rush_hour_offset_profile};
use rand::{Rng, SeedableRng, rngs::SmallRng};

#[test]
fn interpolates_the_offset_at_a_departure() {
    let profile = rush_hour_offset_profile();

    assert_eq!(profile.at(clock(6., 56.)), 240.);
    assert_eq!(profile.at(clock(7., 30.)), 480.);
    assert_eq!(profile.at(clock(9., 4.)), 240.);
    assert_eq!(profile.at(clock(9., 10.)), 0.);
}

#[test]
fn holds_the_first_and_the_last_offset_outside_the_points() {
    let profile = OffsetProfile::new(vec![(100., 30.), (200., 60.)]).unwrap();

    assert_eq!(profile.at(0.), 30.);
    assert_eq!(profile.at(100.), 30.);
    assert_eq!(profile.at(150.), 45.);
    assert_eq!(profile.at(200.), 60.);
    assert_eq!(profile.at(1_000_000.), 60.);
    assert_eq!(OffsetProfile::new(vec![(100., 45.)]).unwrap().at(0.), 45.);
}

/// The rush hour band on two consecutive dates, as eight points.
fn two_day_profile() -> OffsetProfile {
    let band = [(clock(6., 52.), 0.), (clock(7., 0.), 480.), (clock(9., 0.), 480.), (clock(9., 8.), 0.)];
    OffsetProfile::new(
        band.iter().copied().chain(band.iter().map(|&(time, offset)| (time + 86_400., offset))).collect(),
    )
    .unwrap()
}

#[test]
fn repeats_a_band_on_a_second_date() {
    let profile = two_day_profile();

    assert_eq!(profile.at(clock(12., 0.)), 0.);
    assert_eq!(profile.at(86_400. + clock(6., 56.)), 240.);
    assert_eq!(profile.at(86_400. + clock(7., 30.)), 480.);
}

parameterized_test! {refuses_an_invalid_profile, points, {
    assert!(OffsetProfile::new(points).is_err());
}}

refuses_an_invalid_profile! {
    case01_no_points: vec![],
    case02_unsorted_times: vec![(200., 0.), (100., 0.)],
    case03_repeated_time: vec![(100., 0.), (100., 10.)],
    case04_negative_offset: vec![(100., -1.)],
    case05_offset_not_finite: vec![(100., Float::NAN)],
    case06_time_not_finite: vec![(Float::INFINITY, 0.)],
    case07_falls_faster_than_time: vec![(clock(9., 0.), 480.048), (clock(9., 8.), 0.)],
}

#[test]
fn accepts_a_profile_which_falls_exactly_one_second_per_second() {
    assert!(OffsetProfile::new(vec![(clock(9., 0.), 480.), (clock(9., 8.), 0.)]).is_ok());
}

#[test]
fn names_both_points_when_a_profile_falls_faster_than_time() {
    let err = OffsetProfile::new(vec![(clock(9., 0.), 480.048), (clock(9., 8.), 0.)]).unwrap_err().to_string();

    assert!(err.contains("32400") && err.contains("32880"), "{err}");
}

fn band_with_ease_out(ease_out: Float) -> OffsetProfile {
    OffsetProfile::new(vec![
        (clock(6., 30.), 0.),
        (clock(7., 0.), 480.),
        (clock(9., 0.), 480.),
        (clock(9., 0.) + ease_out, 0.),
    ])
    .unwrap()
}

parameterized_test! {finds_the_latest_departure_which_arrives_in_time, (ease_out, deadline, expected), {
    let profile = band_with_ease_out(ease_out);

    let departure = profile.latest_departure(deadline - 600.);

    assert!((departure - expected).abs() < 1e-6, "{departure} != {expected}");
    assert!((departure + 600. + profile.at(departure) - deadline).abs() < 1e-6);
}}

finds_the_latest_departure_which_arrives_in_time! {
    case01_slow_ease_before_its_end: (1800., clock(9., 12.), clock(8., 54.)),
    case02_slow_ease_at_the_band_end: (1800., clock(9., 18.), clock(9., 0.)),
    case03_slow_ease_inside_it: (1800., clock(9., 30.), clock(9., 0.) + 720. * 1800. / 1320.), // 09:16:21.8
    case04_fast_ease: (720., clock(9., 12.), clock(8., 54.)),
    case05_ease_at_the_fifo_limit: (480., clock(9., 12.), clock(8., 54.)),
    case06_rising_edge: (1800., clock(7., 5.), clock(6., 30.) + 1500. * 1800. / 2280.), // 06:49:44.2
}

#[test]
fn resolves_a_stretch_of_equal_arrivals_to_its_latest_departure() {
    // a 780 s leg: every departure from 09:00 to 09:08 arrives at 09:21
    let profile = rush_hour_offset_profile();

    assert_eq!(profile.latest_departure(clock(9., 15.) - 780.), clock(8., 54.));
    assert_eq!(profile.latest_departure(clock(9., 21.) - 780.), clock(9., 8.));
    assert_eq!(profile.latest_departure(clock(9., 20.) + 59. - 780.), clock(8., 59.) + 59.);
}

#[test]
fn resolves_an_arrival_on_a_stretch_inside_the_points_to_its_latest_departure() {
    // the first date's stretch from 09:00 to 09:08 is followed by more points, so its repeated key is not the last
    let profile = two_day_profile();
    let target = clock(9., 8.);

    let departure = profile.latest_departure(target);

    assert_eq!(departure, clock(9., 8.));
    assert_eq!(departure + profile.at(departure), target);
}

#[test]
fn keeps_the_arrival_keys_sorted_when_rounding_would_invert_them() {
    // the middle segment falls exactly one second per second, yet 571.5 + 394.3 rounds below 126.7 + 839.1
    let profile = OffsetProfile::new(vec![(0., 0.), (126.7, 839.1), (571.5, 394.3), (1000., 394.3)]).unwrap();

    assert!(profile.keys.is_sorted(), "{:?}", profile.keys);
    assert_eq!(profile.latest_departure(126.7 + 839.1), 571.5);
}

#[test]
fn holds_the_first_and_the_last_offset_outside_the_points_for_an_arrival() {
    let profile = OffsetProfile::new(vec![(100., 30.), (200., 60.)]).unwrap();

    assert_eq!(profile.latest_departure(50.), 20.);
    assert_eq!(profile.latest_departure(195.), 150.);
    assert_eq!(profile.latest_departure(1000.), 940.);
    assert_eq!(OffsetProfile::new(vec![(100., 45.)]).unwrap().latest_departure(1000.), 955.);
}

#[test]
fn inverts_a_band_on_a_second_date() {
    let profile = two_day_profile();

    assert_eq!(profile.latest_departure(86_400. + clock(9., 15.) - 780.), 86_400. + clock(8., 54.));
    assert_eq!(profile.latest_departure(clock(12., 0.) - 780.), clock(12., 0.) - 780.);
}

#[test]
fn inverts_random_profiles_exactly() {
    let mut rng = SmallRng::seed_from_u64(20261022);

    for _ in 0..1000 {
        // 1 to 8 points 1..3600 s apart; the first offset in 0..900, each next one the previous plus a draw in
        // -gap..900, floored at 0, so it falls at most by the gap before it
        let count = rng.random_range(1..=8);
        let mut points: Vec<(Float, Float)> = vec![(rng.random_range(0.0..86_400.), rng.random_range(0.0..900.))];
        for _ in 1..count {
            let (time, offset) = points[points.len() - 1];
            let gap = rng.random_range(1.0..3600.);
            points.push((time + gap, (offset + rng.random_range(-gap..900.)).max(0.)));
        }
        let (first_time, last_time) = (points[0].0, points[points.len() - 1].0);
        let profile = OffsetProfile::new(points).unwrap();
        let base = rng.random_range(0.0..3000.0);
        let deadline = rng.random_range((first_time - 3600.)..(last_time + 7200.));

        let departure = profile.latest_departure(deadline - base);

        assert!((departure + base + profile.at(departure) - deadline).abs() < 1e-6);
        assert!(departure + 1. + base + profile.at(departure + 1.) > deadline);
    }
}
