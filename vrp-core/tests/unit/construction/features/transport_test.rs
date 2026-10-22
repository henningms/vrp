use super::*;
use crate::helpers::models::problem::*;
use crate::helpers::models::solution::*;
use crate::models::common::*;
use crate::models::problem::{VehicleDetail, VehiclePlace};

const VIOLATION_CODE: ViolationCode = ViolationCode(1);
type VehicleData = (Location, Location, Timestamp, Timestamp);

fn create_detail(
    locations: (Option<Location>, Option<Location>),
    time: Option<(Timestamp, Timestamp)>,
) -> VehicleDetail {
    let (start_location, end_location) = locations;
    VehicleDetail {
        start: start_location.map(|location| VehiclePlace {
            location,
            time: time.map_or(Default::default(), |(start, _)| TimeInterval { earliest: Some(start), latest: None }),
        }),
        end: end_location.map(|location| VehiclePlace {
            location,
            time: time.map_or(Default::default(), |(_, end)| TimeInterval { earliest: None, latest: Some(end) }),
        }),
    }
}

mod timing {
    use super::*;
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::helpers::models::domain::test_random;
    use crate::models::solution::{Activity, Place, Registry};

    fn create_feature() -> Feature {
        TransportFeatureBuilder::new("transport")
            .set_violation_code(VIOLATION_CODE)
            .set_transport_cost(TestTransportCost::new_shared())
            .set_activity_cost(TestActivityCost::new_shared())
            .build_minimize_cost()
            .unwrap()
    }

    fn create_feature_and_route(vehicle_detail_data: VehicleData) -> (Feature, RouteContext) {
        let (location_start, location_end, time_start, time_end) = vehicle_detail_data;

        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicles(vec![
                TestVehicleBuilder::default()
                    .id("v1")
                    .details(vec![create_detail(
                        (Some(location_start), Some(location_end)),
                        Some((time_start, time_end)),
                    )])
                    .build(),
            ])
            .build();
        let route_ctx = RouteContextBuilder::default()
            .with_route(
                RouteBuilder::default()
                    .with_vehicle(&fleet, "v1")
                    .add_activity(ActivityBuilder::with_location(10).build())
                    .add_activity(ActivityBuilder::with_location(20).build())
                    .add_activity(ActivityBuilder::with_location(30).build())
                    .build(),
            )
            .build();

        let feature = create_feature();

        (feature, route_ctx)
    }

    parameterized_test! {can_properly_calculate_latest_arrival, (vehicle, activity, time), {
        can_properly_calculate_latest_arrival_impl(vehicle, activity, time);
    }}

    can_properly_calculate_latest_arrival! {
        case01: ((0, 0, 0., 100.), 3, 70.),
        case02: ((0, 0, 0., 100.), 2, 60.),
        case03: ((0, 0, 0., 100.), 1, 50.),

        case04: ((0, 0, 0., 60.), 3, 30.),
        case05: ((0, 0, 0., 60.), 2, 20.),
        case06: ((0, 0, 0., 60.), 1, 10.),

        case07: ((40, 40, 0., 100.), 3, 90.),
        case08: ((40, 40, 0., 100.), 1, 70.),
        case09: ((40, 40, 0., 100.), 2, 80.),
    }

    fn can_properly_calculate_latest_arrival_impl(vehicle_detail_data: VehicleData, activity_idx: usize, time: Float) {
        let (feature, mut route_ctx) = create_feature_and_route(vehicle_detail_data);
        feature.state.unwrap().accept_route_state(&mut route_ctx);

        let result = route_ctx.state().get_schedule_at(activity_idx).unwrap().latest_arrival;

        assert_eq!(result, time);
    }

    parameterized_test! {can_detect_activity_constraint_violation, (vehicle_detail_data, location, prev_index, next_index, expected), {
        can_detect_activity_constraint_violation_impl(vehicle_detail_data, location, prev_index, next_index, expected);
    }}

    can_detect_activity_constraint_violation! {
        case01: ((0, 0, 0., 100.), 50, 3, 4, None),
        case02: ((0, 0, 0., 100.), 1000, 3, 4, ConstraintViolation::skip(VIOLATION_CODE)),
        case03: ((0, 0, 0., 100.), 50, 2, 3, None),
        case04: ((0, 0, 0., 100.), 51, 2, 3, ConstraintViolation::skip(VIOLATION_CODE)),
        case05: ((0, 0, 0., 60.), 40, 3, 4, ConstraintViolation::skip(VIOLATION_CODE)),
        case06: ((0, 0, 0., 50.), 40, 3, 4, ConstraintViolation::fail(VIOLATION_CODE)),
        case07: ((0, 0, 0., 10.), 40, 3, 4, ConstraintViolation::fail(VIOLATION_CODE)),
        case08: ((0, 0, 60., 100.), 40, 3, 4, ConstraintViolation::fail(VIOLATION_CODE)),
        case09: ((0, 40, 0., 40.), 40, 1, 2, ConstraintViolation::skip(VIOLATION_CODE)),
        case10: ((0, 40, 0., 40.), 40, 3, 4, None),
    }

    fn can_detect_activity_constraint_violation_impl(
        vehicle_detail_data: VehicleData,
        location: Location,
        prev_index: usize,
        next_index: usize,
        expected: Option<ConstraintViolation>,
    ) {
        let (feature, mut route_ctx) = create_feature_and_route(vehicle_detail_data);
        feature.state.unwrap().accept_route_state(&mut route_ctx);
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;

        let prev = route_ctx.route().tour.get(prev_index).unwrap();
        let target = ActivityBuilder::with_location(location).build();
        let next = route_ctx.route().tour.get(next_index);
        let activity_ctx = ActivityContext { index: prev_index, prev, target: &target, next };

        let result =
            feature.constraint.unwrap().evaluate(&MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx));

        assert_eq!(result, expected);
    }

    #[test]
    fn can_update_activity_schedule() {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicles(vec![TestVehicleBuilder::default().id("v1").build()])
            .build();
        let insertion_ctx = TestInsertionContextBuilder::default()
            .with_routes(vec![
                RouteContextBuilder::default()
                    .with_route(
                        RouteBuilder::default()
                            .with_vehicle(&fleet, "v1")
                            .add_activity(
                                ActivityBuilder::default()
                                    .place(Place {
                                        idx: 0,
                                        location: 10,
                                        duration: 5.,
                                        time: TimeWindow { start: 20., end: 30. },
                                    })
                                    .schedule(Schedule::new(10., 25.))
                                    .build(),
                            )
                            .add_activity(
                                ActivityBuilder::default()
                                    .place(Place {
                                        idx: 0,
                                        location: 20,
                                        duration: 10.,
                                        time: TimeWindow { start: 50., end: 100. },
                                    })
                                    .schedule(Schedule::new(35., 60.))
                                    .build(),
                            )
                            .build(),
                    )
                    .build(),
            ])
            .with_registry(Registry::new(&fleet, test_random()))
            .build();
        let mut solution_ctx = insertion_ctx.solution;

        create_feature().state.unwrap().accept_solution_state(&mut solution_ctx);

        let route_ctx = solution_ctx.routes.first().unwrap();
        assert_eq!(route_ctx.route().tour.get(1).unwrap().schedule, Schedule { arrival: 10., departure: 25. });
        assert_eq!(route_ctx.route().tour.get(2).unwrap().schedule, Schedule { arrival: 35., departure: 60. });
    }

    #[test]
    fn can_calculate_soft_activity_cost_for_empty_tour() {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver_with_costs(empty_costs()))
            .add_vehicles(vec![TestVehicleBuilder::default().id("v1").build()])
            .build();
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let route_ctx = RouteContextBuilder::default()
            .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").build())
            .build();
        let target = Box::new(Activity {
            place: Place { idx: 0, location: 5, duration: 1.0, time: DEFAULT_ACTIVITY_TIME_WINDOW },
            schedule: DEFAULT_ACTIVITY_SCHEDULE,
            job: None,
            commute: None,
        });
        let activity_ctx = ActivityContext {
            index: 0,
            prev: route_ctx.route().tour.get(0).unwrap(),
            target: &target,
            next: route_ctx.route().tour.get(1),
        };

        let result = create_feature().objective.unwrap().estimate(&MoveContext::activity(
            &solution_ctx,
            &route_ctx,
            &activity_ctx,
        ));

        assert_eq!(result, 21.0);
    }

    #[test]
    fn can_calculate_soft_activity_cost_for_non_empty_tour() {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver_with_costs(empty_costs()))
            .add_vehicles(vec![TestVehicleBuilder::default().id("v1").build()])
            .build();
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let route_ctx = RouteContextBuilder::default()
            .with_route(
                RouteBuilder::default()
                    .with_vehicle(&fleet, "v1")
                    .add_activity(
                        ActivityBuilder::default()
                            .place(Place {
                                idx: 0,
                                location: 10,
                                duration: 0.0,
                                time: DEFAULT_ACTIVITY_TIME_WINDOW.clone(),
                            })
                            .schedule(Schedule { arrival: 0.0, departure: 10.0 })
                            .build(),
                    )
                    .add_activity(
                        ActivityBuilder::default()
                            .place(Place {
                                idx: 0,
                                location: 20,
                                duration: 0.0,
                                time: TimeWindow { start: 40.0, end: 70.0 },
                            })
                            .build(),
                    )
                    .build(),
            )
            .build();
        let target = Box::new(Activity {
            place: Place { idx: 0, location: 30, duration: 10.0, time: DEFAULT_ACTIVITY_TIME_WINDOW },
            schedule: DEFAULT_ACTIVITY_SCHEDULE,
            job: None,
            commute: None,
        });
        let activity_ctx = ActivityContext {
            index: 0,
            prev: route_ctx.route().tour.get(1).unwrap(),
            target: &target,
            next: route_ctx.route().tour.get(2),
        };

        let result = create_feature().objective.unwrap().estimate(&MoveContext::activity(
            &solution_ctx,
            &route_ctx,
            &activity_ctx,
        ));

        assert_eq!(result, 30.0);
    }

    #[test]
    fn can_stop_with_time_route_constraint() {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicles(vec![TestVehicleBuilder::default().id("v1").build()])
            .build();
        let insertion_ctx = TestInsertionContextBuilder::default().build();
        let solution_ctx = insertion_ctx.solution;
        let route_ctx = RouteContextBuilder::default()
            .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").build())
            .build();
        let job = TestSingleBuilder::default().times(vec![TimeWindow::new(2000., 3000.)]).build_as_job_ref();

        let result =
            create_feature().constraint.unwrap().evaluate(&MoveContext::route(&solution_ctx, &route_ctx, &job));

        assert_eq!(result, ConstraintViolation::fail(VIOLATION_CODE));
    }
}

mod per_run_cost {
    use super::*;
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::models::problem::{Multi, VehiclePerRunCostDimension};
    use crate::models::solution::Activity;

    const PER_RUN: Cost = 100.;

    fn create_feature() -> Feature {
        TransportFeatureBuilder::new("transport")
            .set_violation_code(VIOLATION_CODE)
            .set_transport_cost(TestTransportCost::new_shared())
            .set_activity_cost(TestActivityCost::new_shared())
            .build_minimize_cost()
            .unwrap()
    }

    /// Creates pickup and delivery activities of one pickup-delivery job. The job is returned too:
    /// activities reach it through a weak link, so callers keep it alive.
    fn pudo(pickup_at: Location, delivery_at: Location) -> (Arc<Multi>, Activity, Activity) {
        let mut pickup = TestSingleBuilder::default();
        pickup.location(Some(pickup_at)).demand(Demand::pudo_pickup(1));
        let mut delivery = TestSingleBuilder::default();
        delivery.location(Some(delivery_at)).demand(Demand::pudo_delivery(1));
        let multi = Multi::new_shared(vec![pickup.build_shared(), delivery.build_shared()], Dimensions::default());

        let pickup = ActivityBuilder::with_location(pickup_at).job(Some(multi.jobs[0].clone())).build();
        let delivery = ActivityBuilder::with_location(delivery_at).job(Some(multi.jobs[1].clone())).build();

        (multi, pickup, delivery)
    }

    /// Builds a route with copies of the given activities and accepts the feature's route state.
    fn route_with(per_run: Option<Cost>, activities: &[&Activity]) -> RouteContext {
        let mut vehicle = TestVehicleBuilder::default();
        vehicle.id("v1");
        if let Some(per_run) = per_run {
            vehicle.dimens_mut().set_vehicle_per_run_cost(per_run);
        }
        let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle.build()).build();

        let mut builder = RouteBuilder::default();
        builder.with_vehicle(&fleet, "v1");
        activities.iter().for_each(|activity| {
            builder.add_activity(activity.deep_copy());
        });
        let mut route_ctx = RouteContextBuilder::default().with_route(builder.build()).build();
        create_feature().state.unwrap().accept_route_state(&mut route_ctx);

        route_ctx
    }

    fn fitness_of(per_run: Option<Cost>, activities: &[&Activity]) -> Cost {
        let insertion_ctx =
            TestInsertionContextBuilder::default().with_routes(vec![route_with(per_run, activities)]).build();
        create_feature().objective.unwrap().fitness(&insertion_ctx)
    }

    /// Returns the fitness part charged for runs: fitness with the per-run cost minus fitness without it.
    fn run_fees_of(activities: &[&Activity]) -> Cost {
        fitness_of(Some(PER_RUN), activities) - fitness_of(None, activities)
    }

    /// Estimates inserting `target` after the tour activity at `leg_index`.
    fn estimate(per_run: Option<Cost>, activities: &[&Activity], target: &Activity, leg_index: usize) -> Cost {
        let route_ctx = route_with(per_run, activities);
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext {
            index: leg_index,
            prev: tour.get(leg_index).unwrap(),
            target,
            next: tour.get(leg_index + 1),
        };

        create_feature().objective.unwrap().estimate(&MoveContext::activity(&solution_ctx, &route_ctx, &activity_ctx))
    }

    /// Returns the estimate part charged for runs.
    fn run_fee_estimate(activities: &[&Activity], target: &Activity, leg_index: usize) -> Cost {
        estimate(Some(PER_RUN), activities, target, leg_index) - estimate(None, activities, target, leg_index)
    }

    #[test]
    fn fitness_without_per_run_cost_is_distance_and_duration_cost() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(15, 20);
        let route_ctx = route_with(None, &[&ap, &ad, &bp, &bd]);
        let distance = *route_ctx.state().get_total_distance().unwrap();
        let duration = *route_ctx.state().get_total_duration().unwrap();

        // vehicle and driver both cost 1 per distance and time unit, no fixed cost
        assert_eq!(fitness_of(None, &[&ap, &ad, &bp, &bd]), 2. * distance + 2. * duration);
    }

    #[test]
    fn empty_tour_has_no_run_fee() {
        assert_eq!(run_fees_of(&[]), 0.);
    }

    #[test]
    fn one_run_is_charged_once() {
        let (_a, ap, ad) = pudo(5, 10);

        assert_eq!(run_fees_of(&[&ap, &ad]), PER_RUN);
    }

    #[test]
    fn back_to_back_runs_are_charged_separately() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(15, 20);

        assert_eq!(run_fees_of(&[&ap, &ad, &bp, &bd]), 2. * PER_RUN);
    }

    #[test]
    fn riders_on_board_together_share_one_run() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(6, 11);

        assert_eq!(run_fees_of(&[&ap, &bp, &ad, &bd]), PER_RUN);
    }

    #[test]
    fn handover_at_one_stop_costs_two_runs_in_either_order() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(10, 20);

        assert_eq!(run_fees_of(&[&ap, &ad, &bp, &bd]), 2. * PER_RUN);
        assert_eq!(run_fees_of(&[&ap, &bp, &ad, &bd]), 2. * PER_RUN);
        assert_eq!(fitness_of(Some(PER_RUN), &[&ap, &ad, &bp, &bd]), fitness_of(Some(PER_RUN), &[&ap, &bp, &ad, &bd]));
    }

    #[test]
    fn rider_on_board_across_handover_shares_one_run() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(10, 20);
        let (_c, cp, cd) = pudo(6, 15);

        assert_eq!(run_fees_of(&[&ap, &cp, &ad, &bp, &cd, &bd]), PER_RUN);
    }

    #[test]
    fn estimate_matches_fitness_change_after_removal_refreshes_route_state() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(6, 15);
        let (_c, cp, cd) = pudo(10, 20);
        let (d, dp, dd) = pudo(12, 14);
        let feature = create_feature();
        let state = feature.state.as_ref().unwrap();
        let objective = feature.objective.as_ref().unwrap();

        // removing d turns the stops at 10 into one stop where a hands over to c
        let mut insertion_ctx = TestInsertionContextBuilder::default()
            .with_routes(vec![route_with(Some(PER_RUN), &[&ap, &ad, &dp, &dd, &cp, &cd])])
            .build();
        assert_eq!(run_fees_of(&[&ap, &ad, &dp, &dd, &cp, &cd]), 3. * PER_RUN);
        insertion_ctx.solution.routes[0].route_mut().tour.remove(&Job::Multi(d.clone()));
        state.accept_solution_state(&mut insertion_ctx.solution);
        assert_eq!(objective.fitness(&insertion_ctx), fitness_of(Some(PER_RUN), &[&ap, &ad, &cp, &cd]));

        // b is picked up between a's pickup and the handover, then delivered after c's pickup
        let route_ctx = &insertion_ctx.solution.routes[0];
        let pickup_estimate = estimate_run_fee(route_ctx, &bp, 1);
        let mut shadow = route_ctx.deep_copy();
        shadow.route_mut().tour.insert_at(bp.deep_copy(), 2);
        state.accept_route_state(&mut shadow);
        let delivery_estimate = estimate_run_fee(&shadow, &bd, 4);

        let fee_change = run_fees_of(&[&ap, &bp, &ad, &cp, &bd, &cd]) - run_fees_of(&[&ap, &ad, &cp, &cd]);
        assert_eq!(fee_change, -PER_RUN);
        assert_eq!(pickup_estimate + delivery_estimate, fee_change);
    }

    /// Estimates the change in run fees of inserting `target` after the tour activity at `leg_index`.
    fn estimate_run_fee(route_ctx: &RouteContext, target: &Activity, leg_index: usize) -> Cost {
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext {
            index: leg_index,
            prev: tour.get(leg_index).unwrap(),
            target,
            next: tour.get(leg_index + 1),
        };

        estimate_route_run_cost(route_ctx, &activity_ctx).unwrap_or_default()
    }

    #[test]
    fn single_jobs_open_no_run() {
        let service = ActivityBuilder::with_location(5).job(Some(TestSingleBuilder::default().build_shared())).build();

        assert_eq!(run_fees_of(&[&service]), 0.);
    }

    #[test]
    fn estimate_without_per_run_cost_is_unchanged() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(15, 20);
        let route_ctx = route_with(None, &[&ap, &ad, &bp]);
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext { index: 3, prev: tour.get(3).unwrap(), target: &bd, next: tour.get(4) };

        assert_eq!(estimate_route_run_cost(&route_ctx, &activity_ctx), None);
        // the leg cost of [bp -> bd -> end] replacing [bp -> end]: twice (5 + 20 - 15) for distance and duration
        assert_eq!(estimate(None, &[&ap, &ad, &bp], &bd, 3), 4. * (5. + 20. - 15.));
    }

    #[test]
    fn estimate_charges_job_that_opens_a_new_run() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(15, 20);
        // [start, ap, ad, bp, end]: b rides alone after a is delivered.
        assert_eq!(run_fee_estimate(&[&ap, &ad, &bp], &bd, 3), PER_RUN);
    }

    #[test]
    fn estimate_is_zero_for_job_that_joins_a_run() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(6, 11);
        // [start, ap, bp, ad, end]: b is on board with a.
        assert_eq!(run_fee_estimate(&[&ap, &bp, &ad], &bd, 3), 0.);
    }

    #[test]
    fn estimate_refunds_job_that_merges_two_runs() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_c, cp, cd) = pudo(15, 20);
        let (_b, bp, bd) = pudo(6, 16);
        // [start, ap, bp, ad, cp, cd, end]: delivering b after cp keeps the vehicle loaded between both runs.
        assert_eq!(run_fee_estimate(&[&ap, &bp, &ad, &cp, &cd], &bd, 4), -PER_RUN);
    }

    #[test]
    fn estimate_is_zero_for_first_activity_of_job() {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, _) = pudo(15, 20);

        assert_eq!(run_fee_estimate(&[&ap, &ad], &bp, 2), 0.);
    }

    parameterized_test! {estimate_matches_run_fee_change_after_insertion, (before, shadow, leg_index, after), {
        estimate_matches_run_fee_change_after_insertion_impl(before, shadow, leg_index, after);
    }}

    estimate_matches_run_fee_change_after_insertion! {
        case01_open: (vec!["ap", "ad"], vec!["ap", "ad", "bp"], 3, vec!["ap", "ad", "bp", "bd"]),
        case02_join: (vec!["ap", "ad"], vec!["ap", "bp", "ad"], 3, vec!["ap", "bp", "ad", "bd"]),
        case03_merge: (vec!["ap", "ad", "cp", "cd"], vec!["ap", "bp", "ad", "cp", "cd"], 4, vec!["ap", "bp", "ad", "cp", "bd", "cd"]),
        case04_empty: (vec![], vec!["bp"], 1, vec!["bp", "bd"]),
        case05_right_before_pickup_at_its_stop: (vec!["dp", "dd"], vec!["bp", "dp", "dd"], 1, vec!["bp", "bd", "dp", "dd"]),
        case06_right_after_pickup_at_its_stop: (vec!["dp", "dd"], vec!["bp", "dp", "dd"], 2, vec!["bp", "dp", "bd", "dd"]),
        case07_right_before_pickup_elsewhere: (vec!["cp", "cd"], vec!["bp", "cp", "cd"], 1, vec!["bp", "bd", "cp", "cd"]),
    }

    fn estimate_matches_run_fee_change_after_insertion_impl(
        before: Vec<&str>,
        shadow: Vec<&str>,
        leg_index: usize,
        after: Vec<&str>,
    ) {
        let (_a, ap, ad) = pudo(5, 10);
        let (_b, bp, bd) = pudo(6, 16);
        let (_c, cp, cd) = pudo(15, 20);
        let (_d, dp, dd) = pudo(16, 25);
        let by_name = |names: Vec<&str>| {
            names
                .into_iter()
                .map(|name| match name {
                    "ap" => &ap,
                    "ad" => &ad,
                    "bp" => &bp,
                    "bd" => &bd,
                    "cp" => &cp,
                    "cd" => &cd,
                    "dp" => &dp,
                    "dd" => &dd,
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>()
        };

        let delta = run_fee_estimate(&by_name(shadow), &bd, leg_index);

        assert_eq!(run_fees_of(&by_name(after)) - run_fees_of(&by_name(before)), delta);
    }
}

mod stop_durations {
    use super::*;
    use crate::construction::enablers::TotalDurationTourState;
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::models::problem::{StopDurationTransportCost, StopDurations};
    use crate::models::solution::Activity;
    use rosomaxa::prelude::Float;
    use std::sync::Arc;

    fn create_feature() -> Feature {
        let stop_durations = Arc::new(StopDurations::new(vec![(10, 30.), (20, 40.)]));
        TransportFeatureBuilder::new("transport")
            .set_violation_code(VIOLATION_CODE)
            .set_transport_cost(Arc::new(StopDurationTransportCost::new(
                TestTransportCost::new_shared(),
                stop_durations,
            )))
            .set_activity_cost(TestActivityCost::new_shared())
            .build_minimize_duration()
            .unwrap()
    }

    fn activity(location: Location, time: (Timestamp, Timestamp)) -> Activity {
        ActivityBuilder::with_location_tw_and_duration(location, TimeWindow::new(time.0, time.1), 5.).build()
    }

    fn job(location: Location) -> Activity {
        activity(location, (0., 1000.))
    }

    /// A scheduled route of a vehicle starting at `start` at time 0 and ending at 0.
    fn create_route(feature: &Feature, start: Location, activities: Vec<Activity>) -> RouteContext {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicles(vec![
                TestVehicleBuilder::default()
                    .id("v1")
                    .details(vec![create_detail((Some(start), Some(0)), Some((0., 1000.)))])
                    .build(),
            ])
            .build();
        let mut route_ctx = RouteContextBuilder::default()
            .with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").add_activities(activities).build())
            .build();
        feature.state.as_ref().unwrap().accept_route_state(&mut route_ctx);

        route_ctx
    }

    /// Arrival and departure of every activity after the start, the end included.
    fn schedules(route_ctx: &RouteContext) -> Vec<(Timestamp, Timestamp)> {
        route_ctx.route().tour.all_activities().skip(1).map(|a| (a.schedule.arrival, a.schedule.departure)).collect()
    }

    fn total_duration(route_ctx: &RouteContext) -> Float {
        *route_ctx.state().get_total_duration().unwrap()
    }

    /// Estimates inserting `target` after the tour activity at `index`.
    fn estimate(feature: &Feature, route_ctx: &RouteContext, target: &Activity, index: usize) -> Float {
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext { index, prev: tour.get(index).unwrap(), target, next: tour.get(index + 1) };

        feature.objective.as_ref().unwrap().estimate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
    }

    /// Evaluates the time constraint for inserting `target` after the tour activity at `index`.
    fn evaluate(
        feature: &Feature,
        route_ctx: &RouteContext,
        target: &Activity,
        index: usize,
    ) -> Option<ConstraintViolation> {
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext { index, prev: tour.get(index).unwrap(), target, next: tour.get(index + 1) };

        feature.constraint.as_ref().unwrap().evaluate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
    }

    /// Inserts `target` after the tour activity at `index` and updates the schedule.
    fn insert(feature: &Feature, route_ctx: &mut RouteContext, target: Activity, index: usize) {
        route_ctx.route_mut().tour.insert_at(target, index + 1);
        feature.state.as_ref().unwrap().accept_route_state(route_ctx);
    }

    #[test]
    fn charges_a_stop_duration_once_per_arrival_and_again_on_a_revisit() {
        let feature = create_feature();

        let route_ctx = create_route(&feature, 0, vec![job(10), job(10), job(20), job(10)]);

        assert_eq!(schedules(&route_ctx), vec![(40., 45.), (45., 50.), (100., 105.), (145., 150.), (160., 160.)]);
    }

    #[test]
    fn charges_none_from_the_start_at_the_same_location_or_into_an_end_without_one() {
        let feature = create_feature();

        let route_ctx = create_route(&feature, 10, vec![job(10), job(20)]);

        assert_eq!(schedules(&route_ctx), vec![(0., 5.), (55., 60.), (80., 80.)]);
    }

    #[test]
    fn inserting_at_the_previous_location_adds_only_its_own_duration() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, 0, vec![job(10)]);
        assert_eq!(total_duration(&route_ctx), 55.);

        assert_eq!(estimate(&feature, &route_ctx, &job(10), 1), 0.);
        insert(&feature, &mut route_ctx, job(10), 1);

        assert_eq!(schedules(&route_ctx), vec![(40., 45.), (45., 50.), (60., 60.)]);
        assert_eq!(total_duration(&route_ctx), 60.);
    }

    #[test]
    fn inserting_another_location_between_two_activities_at_one_location_makes_the_second_pay() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, 0, vec![job(10), job(10)]);
        assert_eq!(total_duration(&route_ctx), 60.);

        let delta = estimate(&feature, &route_ctx, &job(20), 1);
        insert(&feature, &mut route_ctx, job(20), 1);

        assert_eq!(delta, 90.);
        assert_eq!(schedules(&route_ctx), vec![(40., 45.), (95., 100.), (140., 145.), (155., 155.)]);
        // the estimate covers travel and stop durations; the inserted job's own 5 completes the change
        assert_eq!(total_duration(&route_ctx) - 60., delta + 5.);
    }

    #[test]
    fn inserting_at_the_next_location_removes_its_stop_duration() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, 0, vec![job(20), job(10)]);
        assert_eq!(schedules(&route_ctx), vec![(60., 65.), (105., 110.), (120., 120.)]);

        assert_eq!(estimate(&feature, &route_ctx, &job(10), 1), 0.);
        insert(&feature, &mut route_ctx, job(10), 1);

        assert_eq!(schedules(&route_ctx), vec![(60., 65.), (105., 110.), (110., 115.), (125., 125.)]);
    }

    #[test]
    fn rejects_a_target_whose_window_closes_within_its_stop_duration() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, 0, vec![job(10)]);

        // the vehicle leaves 10 at 45 and is ready at 20 at 45 + 10 + 40 = 95
        assert_eq!(
            evaluate(&feature, &route_ctx, &activity(20, (0., 94.)), 1),
            ConstraintViolation::skip(VIOLATION_CODE)
        );
        assert_eq!(evaluate(&feature, &route_ctx, &activity(20, (0., 95.)), 1), None);

        insert(&feature, &mut route_ctx, activity(20, (0., 95.)), 1);
        assert_eq!(route_ctx.route().tour.get(2).unwrap().schedule.arrival, 95.);
    }

    #[test]
    fn rejects_a_target_which_pushes_the_next_activity_past_its_window() {
        let feature = create_feature();

        // a job at 10 first makes the job at 20 ready at 0 + 10 + 30 + 5 + 10 + 40 = 95
        let route_ctx = create_route(&feature, 0, vec![activity(20, (0., 94.))]);
        assert_eq!(evaluate(&feature, &route_ctx, &job(10), 0), ConstraintViolation::skip(VIOLATION_CODE));

        let mut route_ctx = create_route(&feature, 0, vec![activity(20, (0., 95.))]);
        assert_eq!(evaluate(&feature, &route_ctx, &job(10), 0), None);

        insert(&feature, &mut route_ctx, job(10), 0);
        assert_eq!(route_ctx.route().tour.get(2).unwrap().schedule.arrival, 95.);
    }
}

mod duration_offsets {
    use super::*;
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::models::problem::{
        DurationOffsetTransportCost, DurationOffsets, MatrixData, OffsetProfile, SimpleActivityCost,
        create_matrix_transport_cost,
    };
    use crate::models::solution::Activity;
    use std::sync::Arc;

    /// Locations: 0 depot, 1 A, 2 B, 3 X. Every leg takes 60 s except A -> B (row 1, column 2): 600 s, plus 480 s for
    /// departures from 07:00 to 09:00, eased in from 06:30 and out by 09:12.
    fn create_feature() -> Feature {
        let mut durations = vec![60.; 16];
        (0..4).for_each(|idx| durations[idx * 4 + idx] = 0.);
        durations[4 + 2] = 600.;
        let mut cells = vec![0_u16; 16];
        cells[4 + 2] = 1;

        let inner = create_matrix_transport_cost(vec![MatrixData::new(0, None, durations, vec![0.; 16])]).unwrap();
        let profile = OffsetProfile::new(vec![
            (clock(6., 30.), 0.),
            (clock(7., 0.), 480.),
            (clock(9., 0.), 480.),
            (clock(9., 12.), 0.),
        ])
        .unwrap();
        let offsets = DurationOffsets::new(4, vec![(0, cells, vec![profile])]).unwrap();

        TransportFeatureBuilder::new("transport")
            .set_violation_code(VIOLATION_CODE)
            .set_transport_cost(Arc::new(DurationOffsetTransportCost::new(inner, Arc::new(offsets))))
            .set_activity_cost(Arc::new(SimpleActivityCost::default()))
            .build_minimize_cost()
            .unwrap()
    }

    fn activity(location: Location, start: Timestamp, end: Timestamp) -> Activity {
        ActivityBuilder::with_location_tw_and_duration(location, TimeWindow::new(start, end), 0.).build()
    }

    /// Leaves the depot at 06:00 for A (open all day), then B (closing at `b_end`); the vehicle returns by 23:00.
    fn create_route(feature: &Feature, b_end: Timestamp) -> RouteContext {
        let fleet = FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicles(vec![
                TestVehicleBuilder::default()
                    .id("v1")
                    .details(vec![create_detail((Some(0), Some(0)), Some((clock(6., 0.), clock(23., 0.))))])
                    .build(),
            ])
            .build();
        let mut route_ctx = RouteContextBuilder::default()
            .with_route(
                RouteBuilder::default()
                    .with_vehicle(&fleet, "v1")
                    .add_activities(vec![activity(1, 0., clock(23., 0.)), activity(2, 0., b_end)])
                    .build(),
            )
            .build();
        route_ctx.route_mut().tour.get_mut(0).unwrap().schedule.departure = clock(6., 0.);
        feature.state.as_ref().unwrap().accept_route_state(&mut route_ctx);

        route_ctx
    }

    /// Evaluates inserting X (location 3, open from `x_open` for an hour) between the depot and A.
    fn evaluate_x_before_a(
        feature: &Feature,
        route_ctx: &RouteContext,
        x_open: Timestamp,
    ) -> Option<ConstraintViolation> {
        let solution_ctx = TestInsertionContextBuilder::default().build().solution;
        let target = activity(3, x_open, x_open + 3600.);
        let tour = &route_ctx.route().tour;
        let activity_ctx = ActivityContext { index: 0, prev: tour.get(0).unwrap(), target: &target, next: tour.get(1) };

        feature.constraint.as_ref().unwrap().evaluate(&MoveContext::activity(&solution_ctx, route_ctx, &activity_ctx))
    }

    /// Inserts that X and updates the schedule.
    fn insert_x_before_a(feature: &Feature, route_ctx: &mut RouteContext, x_open: Timestamp) {
        route_ctx.route_mut().tour.insert_at(activity(3, x_open, x_open + 3600.), 1);
        feature.state.as_ref().unwrap().accept_route_state(route_ctx);
    }

    #[test]
    fn stores_the_latest_departure_which_arrives_in_time_as_the_latest_arrival() {
        let route_ctx = create_route(&create_feature(), clock(9., 12.));

        assert_eq!(route_ctx.state().get_schedule_at(1).unwrap().latest_arrival, clock(8., 54.));
    }

    #[test]
    fn refuses_an_insertion_which_would_arrive_late_after_the_band() {
        let feature = create_feature();
        let route_ctx = create_route(&feature, clock(9., 12.));

        let verdict = evaluate_x_before_a(&feature, &route_ctx, clock(9., 1.));

        assert_eq!(verdict.map(|violation| violation.code), Some(VIOLATION_CODE));
    }

    #[test]
    fn accepts_an_insertion_which_arrives_in_time_and_stays_on_time() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, clock(9., 12.));

        assert_eq!(evaluate_x_before_a(&feature, &route_ctx, clock(8., 52.)), None);
        insert_x_before_a(&feature, &mut route_ctx, clock(8., 52.));

        let tour = &route_ctx.route().tour;
        assert_eq!(tour.get(2).unwrap().schedule.departure, clock(8., 53.));
        assert_eq!(tour.get(3).unwrap().schedule.arrival, clock(9., 11.));
    }

    #[test]
    fn accepts_a_departure_on_the_rising_edge_which_arrives_in_time() {
        let feature = create_feature();
        let mut route_ctx = create_route(&feature, clock(7., 5.));

        assert_eq!(evaluate_x_before_a(&feature, &route_ctx, clock(6., 48.)), None);
        insert_x_before_a(&feature, &mut route_ctx, clock(6., 48.));

        let tour = &route_ctx.route().tour;
        assert_eq!(tour.get(2).unwrap().schedule.departure, clock(6., 49.));
        assert_eq!(tour.get(3).unwrap().schedule.arrival, clock(7., 4.) + 4.);
    }
}
