use super::*;
use crate::format_time;
use crate::helpers::*;
use vrp_core::models::examples::create_example_problem;

parameterized_test! {can_check_load, (stop_loads, expected_result), {
    can_check_load_impl(stop_loads, expected_result);
}}

can_check_load! {
    case00: ( vec![1, 1, 3, 1, 2, 1, 0], Ok(())),

    case01: ( vec![1, 2, 3, 1, 2, 1, 0], Err(vec!["load mismatch at stop 1 in tour 'my_vehicle_1'".into()])),
    case02: ( vec![1, 1, 2, 1, 2, 1, 0], Err(vec!["load mismatch at stops 2, 3 in tour 'my_vehicle_1'".into()])),
    case03: ( vec![1, 1, 3, 2, 2, 1, 0], Err(vec!["load mismatch at stop 3 in tour 'my_vehicle_1'".into()])),
    case04: ( vec![1, 1, 3, 1, 1, 1, 0], Err(vec!["load mismatch at stop 4 in tour 'my_vehicle_1'".into()])),
    case05: ( vec![1, 1, 3, 1, 2, 2, 0], Err(vec!["load mismatch at stop 5 in tour 'my_vehicle_1'".into()])),

    case06_1: ( vec![10, 1, 3, 1, 2, 1, 0], Err(vec!["load exceeds capacity in tour 'my_vehicle_1'".into()])),
    case06_2: ( vec![1, 1, 30, 1, 2, 1, 0], Err(vec!["load exceeds capacity in tour 'my_vehicle_1'".into()])),
    case06_3: ( vec![1, 1, 3, 1, 20, 1, 0], Err(vec!["load exceeds capacity in tour 'my_vehicle_1'".into()])),
}

fn can_check_load_impl(stop_loads: Vec<i32>, expected_result: Result<(), Vec<GenericError>>) {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (1., 0.)),
                create_delivery_job("job2", (2., 0.)),
                create_delivery_job("job3", (3., 0.)),
                create_pickup_job("job4", (4., 0.)),
                create_pickup_delivery_job("job5", (1., 0.), (5., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    start: ShiftStart { earliest: format_time(0.), latest: None, location: (0., 0.).to_loc() },
                    end: Some(ShiftEnd { earliest: None, latest: format_time(1000.), location: (0., 0.).to_loc() }),
                    breaks: None,
                    reloads: Some(vec![VehicleReload {
                        location: (0., 0.).to_loc(),
                        duration: 2.0,
                        ..create_default_reload()
                    }]),
                    recharges: None,
                    required_stops: None,
                    via: None,
                }],
                capacity: Some(vec![5]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(0., 0.)
                        .load(vec![*stop_loads.first().unwrap()])
                        .build_departure(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(3., 5.)
                        .load(vec![*stop_loads.get(1).unwrap()])
                        .distance(1)
                        .activity(ActivityBuilder::delivery().job_id("job1").build())
                        .activity(ActivityBuilder::pickup().job_id("job5").tag("p1").build())
                        .build(),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(3., 5.)
                        .load(vec![*stop_loads.get(2).unwrap()])
                        .distance(1)
                        .build_single("reload", "reload"),
                    StopBuilder::default()
                        .coordinate((2., 0.))
                        .schedule_stamp(7., 8.)
                        .load(vec![*stop_loads.get(3).unwrap()])
                        .distance(3)
                        .activity(
                            ActivityBuilder::delivery().job_id("job2").coordinate((2., 0.)).time_stamp(8., 9.).build(),
                        )
                        .activity(
                            ActivityBuilder::delivery().job_id("job3").coordinate((3., 0.)).time_stamp(9., 10.).build(),
                        )
                        .build(),
                    StopBuilder::default()
                        .coordinate((4., 0.))
                        .schedule_stamp(11., 12.)
                        .load(vec![*stop_loads.get(4).unwrap()])
                        .distance(5)
                        .build_single("job4", "pickup"),
                    StopBuilder::default()
                        .coordinate((5., 0.))
                        .schedule_stamp(13., 14.)
                        .load(vec![*stop_loads.get(5).unwrap()])
                        .distance(6)
                        .build_single_tag("job5", "delivery", "d1"),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(19., 19.)
                        .load(vec![*stop_loads.get(6).unwrap()])
                        .distance(11)
                        .build_arrival(),
                ])
                .statistic(StatisticBuilder::default().driving(1).serving(1).build())
                .build(),
        )
        .build();
    let ctx = CheckerContext::new(create_example_problem(), problem, None, solution).unwrap();

    let result = check_vehicle_load(&ctx);

    assert_eq!(result, expected_result);
}

#[test]
#[ignore]
fn can_check_load_when_departure_has_other_activity() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_pickup_delivery_job("job1", (0., 0.), (1., 0.))], ..create_empty_plan() },
        fleet: Fleet { vehicles: vec![create_vehicle_with_capacity("my_vehicle", vec![2])], ..create_default_fleet() },
        ..create_empty_problem()
    };
    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(0., 1.)
                        .load(vec![1])
                        .distance(0)
                        .activity(ActivityBuilder::default().job_id("departure").activity_type("departure").build())
                        .activity(ActivityBuilder::pickup().job_id("job1").tag("p1").build())
                        .build(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(2., 3.)
                        .load(vec![0])
                        .distance(1)
                        .build_single_tag("job1", "delivery", "d1"),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(4., 4.)
                        .load(vec![0])
                        .distance(2)
                        .build_arrival(),
                ])
                .statistic(StatisticBuilder::default().driving(2).serving(2).build())
                .build(),
        )
        .build();
    let ctx = CheckerContext::new(create_example_problem(), problem, None, solution).unwrap();

    let result = check_vehicle_load(&ctx);

    assert_eq!(result, Ok(()));
}

#[test]
fn can_check_resource_consumption() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (1., 0.)),
                create_delivery_job("job2", (2., 0.)),
                create_delivery_job("job3", (3., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    reloads: Some(vec![VehicleReload {
                        location: (4., 0.).to_loc(),
                        resource_id: Some("resource_1".to_string()),
                        ..create_default_reload()
                    }]),
                    ..create_default_open_vehicle_shift()
                }],
                ..create_vehicle_with_capacity("my_vehicle", vec![2])
            }],
            resources: Some(vec![VehicleResource::Reload { id: "resource_1".to_string(), capacity: vec![1] }]),
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default().coordinate((0., 0.)).schedule_stamp(0., 0.).load(vec![1]).build_departure(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(1., 2.)
                        .load(vec![0])
                        .distance(1)
                        .build_single("job1", "delivery"),
                    StopBuilder::default()
                        .coordinate((4., 0.))
                        .schedule_stamp(5., 7.)
                        .load(vec![2])
                        .distance(4)
                        .build_single("reload", "reload"),
                    StopBuilder::default()
                        .coordinate((3., 0.))
                        .schedule_stamp(8., 9.)
                        .load(vec![1])
                        .distance(5)
                        .build_single("job3", "delivery"),
                    StopBuilder::default()
                        .coordinate((2., 0.))
                        .schedule_stamp(10., 11.)
                        .load(vec![0])
                        .distance(6)
                        .build_single("job2", "delivery"),
                ])
                .statistic(StatisticBuilder::default().driving(6).serving(5).build())
                .build(),
        )
        .build();
    let ctx = CheckerContext::new(create_example_problem(), problem, None, solution).unwrap();

    let result = check_resource_consumption(&ctx);

    assert_eq!(
        result,
        Err("consumed more resource 'resource_1' than available: [2, 0, 0, 0, 0, 0, 0, 0] vs [1, 0, 0, 0, 0, 0, 0, 0]"
            .into())
    );
}

fn create_layout_problem(vehicle: VehicleType) -> Problem {
    Problem {
        plan: create_empty_plan(),
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ..create_empty_problem()
    }
}

fn create_minibus_with_layouts() -> VehicleType {
    VehicleType {
        capacity: None,
        capacity_configurations: Some(vec![
            CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
            CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![4, 1] },
        ]),
        ..create_vehicle_with_capacity("minibus", vec![])
    }
}

fn layout_span(index: usize, name: &str, from_stop: usize, to_stop: usize) -> CapacityLayoutSpan {
    CapacityLayoutSpan { index, name: Some(name.to_string()), from_stop, to_stop }
}

fn create_layout_tour() -> Tour {
    TourBuilder::default()
        .vehicle_id("minibus_1")
        .type_id("minibus")
        .stops(vec![
            StopBuilder::default().load(vec![0, 0]).layout(1, Some("one_wheelchair")).build_departure(),
            StopBuilder::default().load(vec![0, 1]).layout(1, Some("one_wheelchair")).build_single("job1", "pickup"),
            StopBuilder::default().load(vec![0, 0]).layout(1, Some("one_wheelchair")).build_single("job1", "delivery"),
            StopBuilder::default().load(vec![6, 0]).layout(0, Some("all_seated")).build_single("job2", "pickup"),
            StopBuilder::default().load(vec![0, 0]).layout(0, Some("all_seated")).build_single("job2", "delivery"),
            StopBuilder::default().load(vec![0, 0]).layout(0, Some("all_seated")).build_arrival(),
        ])
        .capacity_layouts(vec![layout_span(1, "one_wheelchair", 0, 2), layout_span(0, "all_seated", 3, 5)])
        .build()
}

fn check_layout_tour(vehicle: VehicleType, tour: Tour) -> GenericResult<()> {
    let solution = SolutionBuilder::default().tour(tour).build();
    let ctx = CheckerContext::new(create_example_problem(), create_layout_problem(vehicle), None, solution).unwrap();

    check_capacity_layouts(&ctx)
}

const BOTH_OR_NEITHER: &str =
    "capacity layouts in tour 'minibus_1' must be set on the tour and on every stop, or not at all";
const SPANS_MISMATCH: &str = "capacity layout spans do not match the stops' layouts in tour 'minibus_1'";

parameterized_test! {can_check_capacity_layouts, (modify, expected), {
    can_check_capacity_layouts_impl(modify, expected);
}}

can_check_capacity_layouts! {
    case01_valid: (|_: &mut Tour| {}, Ok(())),
    case02_no_layouts: (|tour: &mut Tour| {
        tour.capacity_layouts = None;
        tour.stops.iter_mut().for_each(|stop| *stop.layout_mut() = None);
    }, Ok(())),
    case03_stop_without_layout: (|tour: &mut Tour| *tour.stops[1].layout_mut() = None, Err(BOTH_OR_NEITHER.into())),
    case04_no_spans: (|tour: &mut Tour| tour.capacity_layouts = None, Err(BOTH_OR_NEITHER.into())),
    case05_spans_without_stop_layouts: (|tour: &mut Tour| {
        tour.stops.iter_mut().for_each(|stop| *stop.layout_mut() = None);
    }, Err(BOTH_OR_NEITHER.into())),
    case06_index_out_of_range: (
        |tour: &mut Tour| *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 5, name: None }),
        Err("capacity layout 5 at stop 3 in tour 'minibus_1' is not in capacityConfigurations".into()),
    ),
    case07_name_mismatch: (
        |tour: &mut Tour| {
            *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 0, name: Some("one_wheelchair".to_string()) })
        },
        Err("capacity layout 0 at stop 3 in tour 'minibus_1' has another name than in capacityConfigurations".into()),
    ),
    case08_load_does_not_fit: (
        |tour: &mut Tour| {
            *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 1, name: Some("one_wheelchair".to_string()) })
        },
        Err("load does not fit capacity layout 1 at stop 3 in tour 'minibus_1'".into()),
    ),
    case09_span_gap: (
        |tour: &mut Tour| {
            tour.capacity_layouts =
                Some(vec![layout_span(1, "one_wheelchair", 0, 1), layout_span(0, "all_seated", 3, 5)])
        },
        Err(SPANS_MISMATCH.into()),
    ),
    case10_span_not_maximal: (
        |tour: &mut Tour| {
            tour.capacity_layouts = Some(vec![
                layout_span(1, "one_wheelchair", 0, 2),
                layout_span(0, "all_seated", 3, 4),
                layout_span(0, "all_seated", 5, 5),
            ])
        },
        Err(SPANS_MISMATCH.into()),
    ),
}

fn can_check_capacity_layouts_impl(modify: impl FnOnce(&mut Tour), expected: GenericResult<()>) {
    let mut tour = create_layout_tour();
    modify(&mut tour);

    let result = check_layout_tour(create_minibus_with_layouts(), tour);

    assert_eq!(result, expected);
}

#[test]
fn can_check_capacity_layouts_of_vehicle_type_without_configurations() {
    let result = check_layout_tour(create_vehicle_with_capacity("minibus", vec![8, 1]), create_layout_tour());

    assert_eq!(
        result,
        Err("tour 'minibus_1' has capacity layouts, but its vehicle type 'minibus' has no capacityConfigurations".into())
    );
}
