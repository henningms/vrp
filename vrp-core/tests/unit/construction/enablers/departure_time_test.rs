use super::*;
use crate::helpers::models::problem::*;
use crate::helpers::models::solution::*;
use crate::models::common::*;
use crate::models::problem::*;
use crate::models::solution::{Activity, Route};
use rosomaxa::prelude::Float;
use std::sync::Arc;

parameterized_test! {can_advance_departure_time, (latest, optimize_whole_tour, tws, expected), {
    let tws = tws.into_iter().map(|(start, end)| TimeWindow::new(start, end)).collect::<Vec<_>>();
    can_advance_departure_time_impl(latest, optimize_whole_tour, tws, expected);
}}

can_advance_departure_time! {
    case01: (None, true, vec![(0., 100.), (25., 100.), (0., 100.)], Some(5.)),
    case02: (Some(3.), true, vec![(0., 100.), (25., 100.), (0., 100.)], Some(3.)),
    case03: (Some(7.), true, vec![(0., 100.), (25., 100.), (0., 100.)], Some(5.)),
    case04: (None, true, vec![(0., 100.), (10., 100.), (42., 100.)], Some(12.)),

    case05: (None, false, vec![(12., 100.), (0., 100.), (0., 100.)], Some(2.)),
    case06: (None, false, vec![(10., 100.), (0., 100.), (0., 100.)], None),
    case07: (None, false, vec![(0., 100.), (25., 100.), (0., 100.)], None),
}

fn can_advance_departure_time_impl(
    latest: Option<Float>,
    optimize_whole_tour: bool,
    tws: Vec<TimeWindow>,
    expected: Option<Float>,
) {
    if let [tw1, tw2, tw3] = tws.as_slice() {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicle(Vehicle {
                details: vec![VehicleDetail {
                    start: Some(VehiclePlace { location: 0, time: TimeInterval { earliest: Some(0.), latest } }),
                    ..test_vehicle_detail()
                }],
                ..test_vehicle_with_id("v1")
            })
            .build();
        let route_ctx = RouteContextBuilder::default()
            .with_route(
                RouteBuilder::default()
                    .with_vehicle(&fleet, "v1")
                    .add_activity(ActivityBuilder::with_location_and_tw(10, tw1.clone()).build())
                    .add_activity(ActivityBuilder::with_location_and_tw(20, tw2.clone()).build())
                    .add_activity(ActivityBuilder::with_location_and_tw(30, tw3.clone()).build())
                    .build(),
            )
            .build();

        let departure_time = try_advance_departure_time(&route_ctx, &TestTransportCost::default(), optimize_whole_tour);

        assert_eq!(departure_time, expected);
    } else {
        unreachable!()
    }
}

parameterized_test! {can_clamp_an_advanced_departure_to_the_latest_feasible_start, (window_start, optimize_whole_tour, has_offsets, expected), {
    can_clamp_an_advanced_departure_to_the_latest_feasible_start_impl(window_start, optimize_whole_tour, has_offsets, expected);
}}

can_clamp_an_advanced_departure_to_the_latest_feasible_start! {
    case01_whole_tour_without_offsets: (clock(7., 20.), true, false, Some(clock(7., 10.))),
    case02_whole_tour_with_offsets: (clock(7., 20.), true, true, Some(clock(7., 7.))),
    case03_first_leg_without_offsets: (clock(7., 20.), false, false, Some(clock(7., 10.))),
    case04_first_leg_with_offsets: (clock(7., 20.), false, true, Some(clock(7., 7.))),
    case05_clamp_not_binding: (clock(7., 0.), true, true, Some(clock(6., 50.))),
}

fn can_clamp_an_advanced_departure_to_the_latest_feasible_start_impl(
    window_start: Timestamp,
    optimize_whole_tour: bool,
    has_offsets: bool,
    expected: Option<Timestamp>,
) {
    let (transport, mut route_ctx) = create_route_to_one_stop(window_start, has_offsets);
    update_route_schedule(&mut route_ctx, &SimpleActivityCost::default(), transport.as_ref());

    let departure_time = try_advance_departure_time(&route_ctx, transport.as_ref(), optimize_whole_tour);

    assert_eq!(departure_time, expected);
}

parameterized_test! {can_advance_departure_time_under_offsets_without_a_bounded_latest_arrival, latest_arrival, {
    can_advance_departure_time_under_offsets_without_a_bounded_latest_arrival_impl(latest_arrival);
}}

can_advance_departure_time_under_offsets_without_a_bounded_latest_arrival! {
    case01_missing_schedule_state: None,
    case02_unbounded_latest_arrival: Some(Float::MAX),
}

fn can_advance_departure_time_under_offsets_without_a_bounded_latest_arrival_impl(latest_arrival: Option<Timestamp>) {
    let (transport, mut route_ctx) = create_route_to_one_stop(clock(7., 20.), true);
    if let Some(latest_arrival) = latest_arrival {
        route_ctx.state_mut().set_schedule_states(vec![
            ActivityScheduleState::default(),
            ActivityScheduleState { latest_arrival, ..ActivityScheduleState::default() },
        ]);
    }

    let departure_time = try_advance_departure_time(&route_ctx, transport.as_ref(), false);

    assert_eq!(departure_time, Some(clock(7., 10.)));
}

parameterized_test! {can_clamp_an_advanced_departure_to_the_window_of_a_later_stop, optimize_whole_tour, {
    can_clamp_an_advanced_departure_to_the_window_of_a_later_stop_impl(optimize_whole_tour);
}}

can_clamp_an_advanced_departure_to_the_window_of_a_later_stop! {
    case01_whole_tour: true,
    case02_first_leg: false,
}

fn can_clamp_an_advanced_departure_to_the_window_of_a_later_stop_impl(optimize_whole_tour: bool) {
    // leaving at 07:10 reaches the first stop at 07:28, long before it closes, but the second one at 07:38
    let (transport, mut route_ctx) = create_route_to_two_stops(clock(7., 35.), clock(23., 0.));
    update_route_schedule(&mut route_ctx, &SimpleActivityCost::default(), transport.as_ref());

    let departure_time = try_advance_departure_time(&route_ctx, transport.as_ref(), optimize_whole_tour);

    assert_eq!(departure_time, Some(clock(7., 7.)));
}

#[test]
fn can_clamp_an_advanced_departure_to_a_whole_second_which_still_catches_the_sailing() {
    let (transport, mut route_ctx) = create_route_over_a_ferry();
    update_route_schedule(&mut route_ctx, &SimpleActivityCost::default(), transport.as_ref());

    let departure_time = try_advance_departure_time(&route_ctx, transport.as_ref(), true).expect("an advance");

    let arrival = departure_time + transport.duration(route_ctx.route(), 0, 3, TravelTime::Departure(departure_time));
    assert!(arrival <= clock(8., 0.), "leaving at {departure_time} misses the sailing and arrives at {arrival}");
    assert_eq!(departure_time, clock(6., 30.) + 52.);
}

#[test]
fn keeps_the_departure_when_leaving_at_the_clamped_start_would_arrive_late() {
    // the backward lookup on the second leg puts the latest start at 07:07:02, from which the first stop is reached in
    // time, at 07:25:02, but the second one at 07:35:02
    let (transport, mut route_ctx) = create_route_to_two_stops(clock(7., 35.), clock(23., 0.));
    let transport = ShortArrivalLookup(transport, (1, 2));
    let activity = SimpleActivityCost::default();
    update_route_schedule(&mut route_ctx, &activity, &transport);
    let schedules = get_schedules(&route_ctx);

    advance_departure_time(&mut route_ctx, &activity, &transport, true);

    assert_eq!(get_schedules(&route_ctx), schedules);
}

parameterized_test! {keeps_the_departure_when_the_first_leg_checks_out_but_a_later_arrival_is_late, case, {
    keeps_the_departure_when_the_first_leg_checks_out_but_a_later_arrival_is_late_impl(case);
}}

keeps_the_departure_when_the_first_leg_checks_out_but_a_later_arrival_is_late! {
    case01_at_a_later_stop: (clock(7., 35.), clock(23., 0.), (1, 2)),
    case02_at_the_tour_end: (clock(9., 0.), clock(7., 45.), (2, 0)),
}

fn keeps_the_departure_when_the_first_leg_checks_out_but_a_later_arrival_is_late_impl(
    (second_stop_end, shift_end, short_leg): (Timestamp, Timestamp, (Location, Location)),
) {
    let (transport, mut route_ctx) = create_route_to_two_stops(second_stop_end, shift_end);
    let transport = ShortArrivalLookup(transport, short_leg);
    let activity = SimpleActivityCost::default();
    update_route_schedule(&mut route_ctx, &activity, &transport);
    let start = try_advance_departure_time(&route_ctx, &transport, true).expect("an advance");
    let latest = route_ctx.state().get_schedule_at(1).expect("a schedule state").latest_arrival;
    let arrival = start + transport.duration(route_ctx.route(), 0, 1, TravelTime::Departure(start));
    assert!(arrival <= latest, "leaving at {start} reaches the first stop at {arrival}, after {latest}");

    advance_departure_time(&mut route_ctx, &activity, &transport, true);

    assert_eq!(route_ctx.route().tour.start().unwrap().schedule.departure, clock(6., 0.));
}

#[test]
fn advances_the_departure_when_the_whole_tour_stays_on_time() {
    let (transport, mut route_ctx) = create_route_to_two_stops(clock(7., 35.), clock(23., 0.));
    let activity = SimpleActivityCost::default();
    update_route_schedule(&mut route_ctx, &activity, transport.as_ref());

    advance_departure_time(&mut route_ctx, &activity, transport.as_ref(), true);

    assert_eq!(route_ctx.route().tour.start().unwrap().schedule.departure, clock(7., 7.));
    assert_eq!(route_ctx.route().tour.get(2).unwrap().schedule.arrival, clock(7., 35.));
}

fn get_schedules(route_ctx: &RouteContext) -> Vec<Schedule> {
    route_ctx.route().tour.all_activities().map(|activity| activity.schedule.clone()).collect()
}

/// Answers a lookup keyed by arrival two seconds short of the forward one, more than a backoff to whole seconds absorbs.
/// Only the leg it names is short.
struct ShortArrivalLookup(Arc<dyn TransportCost>, (Location, Location));

impl TransportCost for ShortArrivalLookup {
    fn duration_approx(&self, profile: &Profile, from: Location, to: Location) -> Duration {
        self.0.duration_approx(profile, from, to)
    }

    fn distance_approx(&self, profile: &Profile, from: Location, to: Location) -> Distance {
        self.0.distance_approx(profile, from, to)
    }

    fn duration(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Duration {
        let duration = self.0.duration(route, from, to, travel_time);

        match travel_time {
            TravelTime::Arrival(_) if (from, to) == self.1 => duration - 2.,
            _ => duration,
        }
    }

    fn distance(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Distance {
        self.0.distance(route, from, to, travel_time)
    }

    fn size(&self) -> usize {
        self.0.size()
    }

    fn has_duration_offsets(&self) -> bool {
        true
    }
}

/// Leaves the depot at 06:00 for one stop ten minutes away which closes at 07:25. With offsets, the leg to the stop
/// takes eight minutes longer from 07:00.
fn create_route_to_one_stop(window_start: Timestamp, has_offsets: bool) -> (Arc<dyn TransportCost>, RouteContext) {
    let matrix = vec![0., 600., 600., 0.];
    let mut transport = create_matrix_transport_cost(vec![MatrixData::new(0, None, matrix.clone(), matrix)]).unwrap();
    if has_offsets {
        let offsets = DurationOffsets::new(2, vec![(0, vec![0, 1, 0, 0], vec![rush_hour_offset_profile()])]).unwrap();
        transport = Arc::new(DurationOffsetTransportCost::new(transport, Arc::new(offsets)));
    }

    (transport, create_route_from_the_depot(vec![stop(1, window_start, clock(7., 25.))], clock(23., 0.)))
}

/// Leaves the depot at 06:00 for a stop ten minutes away which opens at 07:20 and closes at 09:00, then for a second
/// stop ten minutes on which opens at 07:30 and closes at `second_stop_end`, and is back ten minutes later, by
/// `shift_end`. The leg to the first stop takes eight minutes longer from 07:00.
fn create_route_to_two_stops(
    second_stop_end: Timestamp,
    shift_end: Timestamp,
) -> (Arc<dyn TransportCost>, RouteContext) {
    let matrix = vec![0., 600., 600., 600., 0., 600., 600., 600., 0.];
    let transport = create_matrix_transport_cost(vec![MatrixData::new(0, None, matrix.clone(), matrix)]).unwrap();
    let cells = vec![0, 1, 0, 0, 0, 0, 0, 0, 0];
    let offsets = DurationOffsets::new(3, vec![(0, cells, vec![rush_hour_offset_profile()])]).unwrap();
    let transport: Arc<dyn TransportCost> = Arc::new(DurationOffsetTransportCost::new(transport, Arc::new(offsets)));
    let stops = vec![stop(1, clock(7., 20.), clock(9., 0.)), stop(2, clock(7., 30.), second_stop_end)];

    (transport, create_route_from_the_depot(stops, shift_end))
}

/// Leaves the depot (0) at 06:00 for one stop (3) open from 07:40 to 08:00, over a crossing from quay A (1) to quay B
/// (2) with one sailing, at 06:36:07: 300 s to the quay, 1200 s on the water and 600 s on, while the direct road takes
/// 100,000 s. From 06:30 the drive to the quay takes up to 480 s longer, so the latest start which still catches the
/// sailing is 06:30:52.89.
fn create_route_over_a_ferry() -> (Arc<dyn TransportCost>, RouteContext) {
    let mut durations = vec![0.; 16];
    durations[1] = 300.;
    durations[2 * 4 + 3] = 600.;
    durations[3] = 100_000.;
    let road = create_matrix_transport_cost(vec![MatrixData::new(0, None, durations.clone(), durations)]).unwrap();
    let profile = OffsetProfile::new(vec![
        (clock(6., 30.), 0.),
        (clock(7., 0.), 480.),
        (clock(9., 0.), 480.),
        (clock(9., 12.), 0.),
    ])
    .unwrap();
    let mut cells = vec![0; 16];
    cells[1] = 1;
    let offsets = DurationOffsets::new(4, vec![(0, cells, vec![profile])]).unwrap();
    let sailing = FerrySailing { dep: clock(6., 36.) + 7., arr: clock(6., 56.) + 7. };
    let crossing = FerryCrossing::new("crossing".to_string(), 1, 2, 1200., 0., vec![sailing], vec![]);
    let transport: Arc<dyn TransportCost> = Arc::new(FerryAwareTransportCost::new(
        Arc::new(DurationOffsetTransportCost::new(road, Arc::new(offsets))),
        Arc::new(FerryIndex::new(vec![crossing])),
    ));

    (transport, create_route_from_the_depot(vec![stop(3, clock(7., 40.), clock(8., 0.))], clock(23., 0.)))
}

fn stop(location: Location, start: Timestamp, end: Timestamp) -> Activity {
    ActivityBuilder::with_location_tw_and_duration(location, TimeWindow::new(start, end), 0.).build()
}

/// Leaves the depot, location 0, at 06:00 for the stops and returns there by `shift_end`.
fn create_route_from_the_depot(stops: Vec<Activity>, shift_end: Timestamp) -> RouteContext {
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(Vehicle {
            details: vec![VehicleDetail {
                start: Some(VehiclePlace {
                    location: 0,
                    time: TimeInterval { earliest: Some(clock(6., 0.)), latest: None },
                }),
                end: Some(VehiclePlace { location: 0, time: TimeInterval { earliest: None, latest: Some(shift_end) } }),
            }],
            ..test_vehicle_with_id("v1")
        })
        .build();
    let mut route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").add_activities(stops).build())
        .build();
    route_ctx.route_mut().tour.get_mut(0).unwrap().schedule.departure = clock(6., 0.);

    route_ctx
}

parameterized_test! {can_recede_departure_time, (earliest, start_departure, latest_first_arrival, tw, duration_limit, expected), {
    can_recede_departure_time_impl(earliest, start_departure, latest_first_arrival, TimeWindow::new(tw.0, tw.1), duration_limit, expected);
}}

can_recede_departure_time! {
    case01: (Some(0.), 0., 10., (10., 20.), None, None),
    case02: (Some(0.), 5., 10., (10., 20.), None, None),
    case03: (Some(5.), 10., 15., (10., 20.), None, Some(5.)),
    case04: (Some(5.), 10., 20., (10., 20.), None, Some(5.)),
    case05: (None, 10., 50., (10., 20.), None, Some(0.)),
    case06: (Some(5.), 10., 11., (10., 20.), None, Some(9.)),

    case07: (Some(0.), 10., 20., (10., 20.), Some((20., 30.)), Some(0.)),
    case08: (Some(0.), 10., 20., (10., 20.), Some((20., 25.)), Some(5.)),
    case09: (Some(0.), 10., 20., (10., 20.), Some((20., 20.)), None),
}

fn can_recede_departure_time_impl(
    earliest: Option<Float>,
    start_departure: Float,
    latest_first_arrival: Float,
    tw: TimeWindow,
    total_duration_limit: Option<(Float, Float)>,
    expected: Option<Float>,
) {
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(Vehicle {
            details: vec![VehicleDetail {
                start: Some(VehiclePlace { location: 0, time: TimeInterval { earliest, latest: None } }),
                ..test_vehicle_detail()
            }],
            ..test_vehicle_with_id("v1")
        })
        .build();
    let mut route_ctx = RouteContextBuilder::default()
        .with_route(
            RouteBuilder::default()
                .with_vehicle(&fleet, "v1")
                .add_activity(ActivityBuilder::with_location_and_tw(10, tw).build())
                .build(),
        )
        .build();
    let (route, state) = route_ctx.as_mut();
    route.tour.get_mut(0).unwrap().schedule.departure = start_departure;
    state.set_schedule_states(vec![
        ActivityScheduleState::default(),
        ActivityScheduleState { latest_arrival: latest_first_arrival, ..ActivityScheduleState::default() },
    ]);

    if let Some((total, limit)) = total_duration_limit {
        state.set_total_duration(total);
        state.set_limit_duration(limit);
    }

    let departure_time = try_recede_departure_time(&route_ctx);

    assert_eq!(departure_time, expected);
}
