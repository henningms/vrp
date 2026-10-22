use super::*;
use crate::construction::enablers::DynamicTransportCost;
use crate::helpers::models::problem::{clock, rush_hour_offset_profile, test_driver, test_vehicle};
use crate::helpers::models::solution::test_actor_with_profile;
use crate::models::Extras;
use crate::models::common::Profile;
use crate::models::problem::{
    Actor, FerryAwareTransportCost, FerryIndex, MatrixData, StopDurationTransportCost, StopDurations, Vehicle,
    create_matrix_transport_cost,
};
use crate::models::solution::Route;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::sync::Arc;

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

fn route() -> Route {
    Route { actor: test_actor_with_profile(0), tour: Default::default() }
}

/// A route whose vehicle profile scales durations.
fn scaled_route(scale: Float) -> Route {
    let actor = Actor {
        vehicle: Arc::new(Vehicle { profile: Profile::new(0, Some(scale)), ..test_vehicle(0) }),
        driver: Arc::new(test_driver()),
        detail: test_actor_with_profile(0).detail.clone(),
    };
    Route { actor: Arc::new(actor), tour: Default::default() }
}

/// Three locations; the cells named in `cells` take the rush hour offset.
fn wrap(durations: Vec<Float>, cells: Vec<u16>) -> DurationOffsetTransportCost {
    let distances = vec![0., 5000., 500., 5000., 0., 500., 500., 500., 0.];
    let inner = create_matrix_transport_cost(vec![MatrixData::new(0, None, durations, distances)]).unwrap();
    let offsets = DurationOffsets::new(3, vec![(0, cells, vec![rush_hour_offset_profile()])]).unwrap();

    DurationOffsetTransportCost::new(inner, Arc::new(offsets))
}

/// 0 → 1 takes 600 s and the rush hour offset, every other leg 60 s.
fn example_transport() -> DurationOffsetTransportCost {
    wrap(vec![0., 600., 60., 600., 0., 60., 60., 60., 0.], vec![0, 1, 0, 0, 0, 0, 0, 0, 0])
}

#[test]
fn adds_the_offset_at_the_departure_to_the_base() {
    let (transport, route) = (example_transport(), route());
    let at = |from, to, time| transport.duration(&route, from, to, TravelTime::Departure(time));

    assert_eq!(at(0, 1, clock(6., 56.)), 840.);
    assert_eq!(at(0, 1, clock(7., 30.)), 1080.);
    assert_eq!(at(0, 1, clock(9., 4.)), 840.);
    assert_eq!(at(0, 1, clock(9., 10.)), 600.);
    assert_eq!(at(1, 0, clock(7., 30.)), 600.);
    assert_eq!(at(0, 2, clock(7., 30.)), 60.);
}

#[test]
fn returns_the_duration_of_the_latest_departure_which_arrives_in_time() {
    let (transport, route) = (example_transport(), route());
    let by = |from, to, time| transport.duration(&route, from, to, TravelTime::Arrival(time));

    // leaving at 08:54 in the band arrives at 09:12; leaving at 09:02, as the arrival time alone suggests, is 6 min late
    assert_eq!(by(0, 1, clock(9., 12.)), 1080.);
    assert_eq!(by(0, 1, clock(9., 20.)), 600.);
    assert_eq!(by(1, 0, clock(9., 12.)), 600.);
}

#[test]
fn keeps_the_base_for_an_unbounded_arrival() {
    let (transport, route) = (example_transport(), route());
    let by = |time| transport.duration(&route, 0, 1, TravelTime::Arrival(time));

    // an unbounded latest departure would cancel against the arrival to a leg of no duration at all
    assert_eq!(by(Float::MAX), 600.);
    assert_eq!(by(Float::INFINITY), 600.);
}

#[test]
fn keeps_the_base_for_estimates_and_distances() {
    let (transport, route) = (example_transport(), route());

    assert_eq!(transport.duration_approx(&Profile::default(), 0, 1), 600.);
    assert_eq!(transport.distance(&route, 0, 1, TravelTime::Departure(clock(7., 30.))), 5000.);
    assert_eq!(transport.distance_approx(&Profile::default(), 0, 1), 5000.);
    assert_eq!(transport.size(), 3);
}

#[test]
fn adds_the_offset_after_the_profile_scale() {
    let transport = example_transport();
    let route = scaled_route(1.5);

    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Departure(clock(7., 30.))), 1380.);
    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Arrival(clock(9., 15.))), 1380.);
    assert_eq!(transport.duration_approx(&Profile::new(0, Some(1.5)), 0, 1), 900.);
}

#[test]
fn keeps_unreachable_cells_unreachable() {
    // both markers name the profile: -1 (no route) on 0 -> 1 and the 1e9 sentinel on 0 -> 2
    let transport = wrap(vec![0., -1., 1e9, 600., 0., 60., 60., 60., 0.], vec![0, 1, 1, 0, 0, 0, 0, 0, 0]);
    let route = route();

    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Departure(clock(7., 30.))), -1.);
    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Arrival(clock(9., 12.))), -1.);
    assert_eq!(transport.duration(&route, 0, 2, TravelTime::Departure(clock(7., 30.))), 1e9);
}

#[test]
fn charges_no_offset_outside_the_matrix_or_for_another_profile() {
    let offsets =
        DurationOffsets::new(3, vec![(0, vec![0, 1, 0, 0, 0, 0, 0, 0, 0], vec![rush_hour_offset_profile()])]).unwrap();

    assert!(offsets.get(0, 0, 1).is_some());
    assert!(offsets.get(0, 1, 0).is_none());
    assert!(offsets.get(0, 0, 3).is_none());
    assert!(offsets.get(0, 3, 1).is_none());
    assert!(offsets.get(1, 0, 1).is_none());
}

#[test]
fn refuses_cells_which_do_not_match_the_matrix() {
    let table = |cells: Vec<u16>| (0, cells, vec![rush_hour_offset_profile()]);

    // all zero but one cell short: the shape is checked before all-zero tables are dropped
    assert!(DurationOffsets::new(3, vec![table(vec![0; 8])]).is_err());
    assert!(DurationOffsets::new(3, vec![table(vec![0, 2, 0, 0, 0, 0, 0, 0, 0])]).is_err());
    assert!(DurationOffsets::new(3, vec![table(vec![0, 1, 0, 0, 0, 0, 0, 0, 0]), table(vec![1; 9])]).is_err());
}

#[test]
fn ignores_tables_without_a_cell_which_names_a_profile() {
    let zero = |index| (index, vec![0; 9], vec![rush_hour_offset_profile()]);

    // two all-zero tables for one profile index are not a duplicate: neither applies
    assert!(DurationOffsets::new(3, vec![zero(0), zero(0)]).unwrap().is_empty());
}

#[test]
fn keeps_the_cells_it_was_given_without_copying() {
    let cells: Vec<u16> = vec![0, 1, 0, 0, 0, 0, 0, 0, 0];
    let pointer = cells.as_ptr();

    let offsets = DurationOffsets::new(3, vec![(0, cells, vec![rush_hour_offset_profile()])]).unwrap();

    assert_eq!(offsets.cells(0).unwrap().as_ptr(), pointer);
}

#[test]
fn reports_duration_offsets_through_every_wrapper() {
    let offsets: Arc<dyn TransportCost> = Arc::new(example_transport());
    let plain = create_matrix_transport_cost(vec![MatrixData::new(0, None, vec![0.; 9], vec![0.; 9])]).unwrap();

    assert!(offsets.has_duration_offsets());
    assert!(!plain.has_duration_offsets());
    assert!(
        StopDurationTransportCost::new(offsets.clone(), Arc::new(StopDurations::new(vec![(1, 30.)])))
            .has_duration_offsets()
    );
    assert!(FerryAwareTransportCost::new(offsets.clone(), Arc::new(FerryIndex::default())).has_duration_offsets());
    assert!(DynamicTransportCost::new(Default::default(), offsets).unwrap().has_duration_offsets());
}

#[test]
fn reports_no_duration_offsets_through_wrappers_around_a_plain_matrix() {
    let plain = create_matrix_transport_cost(vec![MatrixData::new(0, None, vec![0.; 9], vec![0.; 9])]).unwrap();

    assert!(
        !StopDurationTransportCost::new(plain.clone(), Arc::new(StopDurations::new(vec![(1, 30.)])))
            .has_duration_offsets()
    );
    assert!(!FerryAwareTransportCost::new(plain.clone(), Arc::new(FerryIndex::default())).has_duration_offsets());
    assert!(!DynamicTransportCost::new(Default::default(), plain).unwrap().has_duration_offsets());
}

#[test]
fn adds_the_offset_on_the_road_and_the_stop_duration_after_it() {
    let transport =
        StopDurationTransportCost::new(Arc::new(example_transport()), Arc::new(StopDurations::new(vec![(1, 30.)])));
    let route = route();

    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Departure(clock(6., 56.))), 870.);
    // pulls in at 09:15 after leaving at 08:57 in the band, then stops for 30 s
    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Arrival(clock(9., 15.) + 30.)), 1110.);
    assert_eq!(transport.duration_approx(&Profile::default(), 0, 1), 630.);
}

#[test]
fn keeps_duration_offsets_in_extras() {
    let offsets = Arc::new(
        DurationOffsets::new(3, vec![(0, vec![0, 1, 0, 0, 0, 0, 0, 0, 0], vec![rush_hour_offset_profile()])]).unwrap(),
    );
    let mut extras = Extras::default();

    extras.set_duration_offsets(offsets.clone());

    assert!(Arc::ptr_eq(&extras.get_duration_offsets().unwrap(), &offsets));
}
