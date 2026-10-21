use super::*;
use crate::helpers::models::solution::test_actor_with_profile;
use crate::models::problem::{MatrixData, create_matrix_transport_cost};

fn route() -> Route {
    Route { actor: test_actor_with_profile(0), tour: Default::default() }
}

fn wrap(matrices: Vec<MatrixData>, stops: Vec<(Location, Duration)>) -> StopDurationTransportCost {
    StopDurationTransportCost::new(create_matrix_transport_cost(matrices).unwrap(), Arc::new(StopDurations::new(stops)))
}

/// Three locations; the diagonal at location 1 is not zero, so a same-location leg shows whether a stop is added.
fn three_locations() -> Vec<MatrixData> {
    vec![MatrixData::new(
        0,
        None,
        vec![0., 10., 20., 10., 5., 10., 20., 10., 0.],
        vec![0., 100., 200., 100., 0., 100., 200., 100., 0.],
    )]
}

#[test]
fn keeps_the_largest_stop_duration_per_location() {
    let stop_durations = StopDurations::new(vec![(1, 30.), (1, 60.), (2, 10.), (1, 45.)]);

    assert_eq!(stop_durations.get(1), 60.);
    assert_eq!(stop_durations.get(2), 10.);
    assert_eq!(stop_durations.get(0), 0.);
    assert_eq!(stop_durations.get(99), 0.);
    assert!(!stop_durations.is_empty());
}

#[test]
fn ignores_stop_durations_which_are_not_positive() {
    let stop_durations = StopDurations::new(vec![(1, 0.), (2, -5.)]);

    assert!(stop_durations.is_empty());
    assert_eq!(stop_durations.get(2), 0.);
}

#[test]
fn charges_a_stop_duration_only_on_a_leg_between_different_locations() {
    let stop_durations = StopDurations::new(vec![(1, 60.)]);

    assert_eq!(stop_durations.on_leg(0, 1), 60.);
    assert_eq!(stop_durations.on_leg(1, 1), 0.);
    assert_eq!(stop_durations.on_leg(1, 0), 0.);
}

#[test]
fn adds_the_stop_duration_to_the_leg_into_its_location() {
    let transport = wrap(three_locations(), vec![(1, 30.)]);
    let route = route();
    let departure = TravelTime::Departure(0.);

    assert_eq!(transport.duration(&route, 0, 1, departure), 40.);
    assert_eq!(transport.duration_approx(&Profile::default(), 0, 1), 40.);
    assert_eq!(transport.duration(&route, 1, 1, departure), 5.);
    assert_eq!(transport.duration(&route, 1, 2, departure), 10.);
    assert_eq!(transport.distance(&route, 0, 1, departure), 100.);
    assert_eq!(transport.distance_approx(&Profile::default(), 0, 1), 100.);
    assert_eq!(transport.size(), 3);
}

#[test]
fn keeps_an_unreachable_leg_unreachable() {
    let unreachable = vec![MatrixData::new(0, None, vec![0., -1., -1., 0.], vec![0., -1., -1., 0.])];
    let transport = wrap(unreachable, vec![(1, 30.)]);

    assert_eq!(transport.duration(&route(), 0, 1, TravelTime::Departure(0.)), -1.);
    assert_eq!(transport.duration_approx(&Profile::default(), 0, 1), -1.);
}

#[test]
fn adds_the_stop_duration_after_the_profile_scale() {
    let transport = wrap(three_locations(), vec![(1, 30.)]);

    assert_eq!(transport.duration_approx(&Profile::new(0, Some(2.)), 0, 1), 50.);
}

#[test]
fn adds_the_stop_duration_to_interpolated_durations() {
    let transport = wrap(
        vec![
            MatrixData::new(0, Some(0.), vec![0., 10., 10., 0.], vec![0., 7., 7., 0.]),
            MatrixData::new(0, Some(100.), vec![0., 20., 20., 0.], vec![0., 7., 7., 0.]),
        ],
        vec![(1, 30.)],
    );
    let route = route();

    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Departure(50.)), 45.);
    // read at the pull-in time 80 - 30 = 50; reading at 80 would give 18 + 30
    assert_eq!(transport.duration(&route, 0, 1, TravelTime::Arrival(80.)), 45.);
    assert_eq!(transport.duration_approx(&Profile::default(), 0, 1), 40.);
    assert_eq!(transport.distance(&route, 0, 1, TravelTime::Departure(50.)), 7.);
}
