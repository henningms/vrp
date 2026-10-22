use super::*;
use crate::helpers::models::problem::{clock, rush_hour_offset_profile};

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
