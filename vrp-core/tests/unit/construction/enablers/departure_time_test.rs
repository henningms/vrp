use super::*;
use crate::helpers::models::problem::*;
use crate::helpers::models::solution::*;
use crate::models::common::*;
use crate::models::problem::*;
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

/// Leaves the depot at 06:00 for one stop ten minutes away which closes at 07:25. With offsets, the leg to the stop
/// takes eight minutes longer from 07:00.
fn create_route_to_one_stop(window_start: Timestamp, has_offsets: bool) -> (Arc<dyn TransportCost>, RouteContext) {
    let matrix = vec![0., 600., 600., 0.];
    let mut transport = create_matrix_transport_cost(vec![MatrixData::new(0, None, matrix.clone(), matrix)]).unwrap();
    if has_offsets {
        let offsets = DurationOffsets::new(2, vec![(0, vec![0, 1, 0, 0], vec![rush_hour_offset_profile()])]).unwrap();
        transport = Arc::new(DurationOffsetTransportCost::new(transport, Arc::new(offsets)));
    }
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(Vehicle {
            details: vec![VehicleDetail {
                start: Some(VehiclePlace {
                    location: 0,
                    time: TimeInterval { earliest: Some(clock(6., 0.)), latest: None },
                }),
                end: Some(VehiclePlace {
                    location: 0,
                    time: TimeInterval { earliest: None, latest: Some(clock(23., 0.)) },
                }),
            }],
            ..test_vehicle_with_id("v1")
        })
        .build();
    let stop =
        ActivityBuilder::with_location_tw_and_duration(1, TimeWindow::new(window_start, clock(7., 25.)), 0.).build();
    let mut route_ctx = RouteContextBuilder::default()
        .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").add_activity(stop).build())
        .build();
    route_ctx.route_mut().tour.get_mut(0).unwrap().schedule.departure = clock(6., 0.);

    (transport, route_ctx)
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
