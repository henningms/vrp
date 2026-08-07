use super::*;
use crate::helpers::*;
use vrp_core::models::examples::create_example_problem;

parameterized_test! {check_vehicles, (known_ids, tours, expected_result), {
    check_vehicles_impl(known_ids, tours, expected_result);
}}

check_vehicles! {
    case_01: (vec!["vehicle_1"], vec![("vehicle_1", 0)], Ok(())),
    case_02: (vec!["vehicle_1"], vec![("vehicle_2", 0)], Err(())),
    case_03: (vec!["vehicle_1"], vec![("vehicle_1", 0), ("vehicle_1", 1)], Ok(())),
    case_04: (vec!["vehicle_1"], vec![("vehicle_1", 0), ("vehicle_1", 0)], Err(())),
}

fn check_vehicles_impl(known_ids: Vec<&str>, tours: Vec<(&str, usize)>, expected_result: Result<(), ()>) {
    let problem = Problem {
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: known_ids.into_iter().map(|id| id.to_string()).collect(),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let solution = Solution {
        tours: tours
            .into_iter()
            .map(|(id, shift_index)| Tour {
                vehicle_id: id.to_string(),
                type_id: "my_vehicle".to_string(),
                shift_index,
                stops: vec![],
                statistic: Statistic::default(),
            })
            .collect(),
        ..SolutionBuilder::default().build()
    };
    let ctx = CheckerContext::new(create_example_problem(), problem, None, solution).unwrap();

    let result = check_vehicles(&ctx);

    assert_eq!(result.map_err(|_| ()), expected_result);
}

parameterized_test! {check_jobs, (jobs, tours, unassigned, expected_result), {
    check_jobs_impl(jobs, tours, unassigned, expected_result);
}}

check_jobs! {
    case_01: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![("my_vehicle_1", 0, vec![("job1", "pickup"), ("job1", "delivery")])],
        vec![],
        Ok(())
    ),
    case_02: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![
            ("my_vehicle_1", 0, vec![("job1", "pickup")]),
            ("my_vehicle_2", 0, vec![("job1", "delivery")])
        ],
        vec![],
        Err("job served in multiple tours: 'job1'".into())
    ),
    case_03: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![("my_vehicle_1", 0, vec![("job1", "pickup")])],
        vec![],
        Err("not all tasks served for 'job1', expected: 2, assigned: 1".into())
    ),
    case_04: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![("my_vehicle_1", 0, vec![("job1", "delivery"), ("job1", "pickup")])],
        vec![],
        Err("found pickup after delivery for 'job1'".into())
    ),
    case_05: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![],
        vec!["job1"],
        Ok(())
    ),
    case_06: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![],
        vec!["job1", "job1"],
        Err("duplicated job ids in the list of unassigned jobs".into())
    ),
    case_07: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![],
        vec!["job2"],
        Err("unknown job id in the list of unassigned jobs: 'job2'".into())
    ),
    case_08: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![],
        vec!["job1", "vehicle_break"],
        Ok(())
    ),
    case_09: (
        vec![("job1", vec!["pickup", "delivery"])],
        vec![("my_vehicle_1", 0, vec![("job1", "pickup"), ("job1", "delivery")])],
        vec!["job1"],
        Err("job present as assigned and unassigned: 'job1'".into())
    ),
     case_10: (
        vec![("job1", vec!["pickup"])],
        vec![("my_vehicle_1", 0, vec![("job1", "pickup")])],
        vec![],
        Ok(())
    ),
}

#[allow(clippy::type_complexity)]
fn check_jobs_impl(
    jobs: Vec<(&str, Vec<&str>)>,
    tours: Vec<(&str, usize, Vec<(&str, &str)>)>,
    unassigned: Vec<&str>,
    expected_result: Result<(), GenericError>,
) {
    let create_tasks = |tgt: &str, tasks: &Vec<&str>| {
        (1..)
            .zip(tasks.iter())
            .filter(|(_, t)| **t == tgt)
            .map(|(idx, _)| JobTask {
                places: vec![JobPlace {
                    location: Location::Coordinate { lat: 0.0, lng: 0.0 },
                    duration: 0.0,
                    times: None,
                    tag: Some(format!("{tgt}{idx}")),
                    requested_time: None,
                }],
                demand: if tgt != "service" { Some(vec![1]) } else { None },
                named_demand: None,
                order: None,
            })
            .collect()
    };

    let create_stop = |stop: (&str, &str)| StopBuilder::default().coordinate((0., 0.)).build_single(stop.0, stop.1);

    let problem = Problem {
        plan: Plan {
            jobs: jobs
                .into_iter()
                .map(|(id, tasks)| Job {
                    pickups: Some(create_tasks("pickup", &tasks)),
                    deliveries: Some(create_tasks("delivery", &tasks)),
                    replacements: Some(create_tasks("replacement", &tasks)),
                    services: Some(create_tasks("service", &tasks)),
                    ..create_job(id)
                })
                .collect(),
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let solution = Solution {
        tours: tours
            .into_iter()
            .map(|(id, shift_index, stops)| Tour {
                vehicle_id: id.to_string(),
                type_id: "my_vehicle".to_string(),
                shift_index,
                stops: stops.into_iter().map(create_stop).collect(),
                statistic: Statistic::default(),
            })
            .collect(),
        unassigned: Some(
            unassigned.into_iter().map(|job| UnassignedJob { job_id: job.to_string(), reasons: vec![] }).collect(),
        ),
        ..SolutionBuilder::default().build()
    };
    let ctx = CheckerContext::new(create_example_problem(), problem, None, solution).unwrap();

    let result = check_jobs_presence(&ctx);

    assert_eq!(result, expected_result);
}

fn create_constraint_job(
    id: &str,
    pickup_tags: &[&str],
    delivery_tags: &[&str],
    solo_riding: bool,
    fixed_order: bool,
) -> Job {
    let create_tasks =
        |tags: &[&str]| tags.iter().map(|tag| create_task((0., 0.), Some((*tag).to_string()))).collect::<Vec<_>>();

    Job {
        pickups: Some(create_tasks(pickup_tags)),
        deliveries: Some(create_tasks(delivery_tags)),
        solo_riding: Some(solo_riding),
        fixed_order: Some(fixed_order),
        ..create_job(id)
    }
}

fn create_constraint_context(jobs: Vec<Job>, activities: Vec<(&str, &str, &str)>) -> CheckerContext {
    let problem =
        Problem { plan: Plan { jobs, ..create_empty_plan() }, fleet: create_default_fleet(), ..create_empty_problem() };
    let solution = if activities.is_empty() {
        SolutionBuilder::default().build()
    } else {
        let stops = activities
            .into_iter()
            .map(|(job_id, activity_type, tag)| {
                StopBuilder::default().coordinate((0., 0.)).build_single_tag(job_id, activity_type, tag)
            })
            .collect();
        SolutionBuilder::default().tour(TourBuilder::default().stops(stops).build()).build()
    };

    CheckerContext::new(create_example_problem(), problem, None, solution).unwrap()
}

#[test]
fn can_check_fixed_companion_order() {
    let ctx = create_constraint_context(
        vec![create_constraint_job("job1", &["p0", "p1"], &["d0"], false, true)],
        vec![("job1", "pickup", "p0"), ("job1", "pickup", "p1"), ("job1", "delivery", "d0")],
    );

    assert_eq!(check_fixed_order(&ctx), Ok(()));
}

#[test]
fn can_detect_fixed_companion_order_violation() {
    let ctx = create_constraint_context(
        vec![create_constraint_job("job1", &["p0", "p1"], &["d0"], false, true)],
        vec![("job1", "pickup", "p1"), ("job1", "pickup", "p0"), ("job1", "delivery", "d0")],
    );

    let error = check_fixed_order(&ctx).unwrap_err().to_string();

    assert!(error.contains("fixed order is not respected for job 'job1'"));
    assert!(error.contains("activity 0"));
}

#[test]
fn can_reorder_pickups_when_fixed_order_is_disabled() {
    let ctx = create_constraint_context(
        vec![create_constraint_job("job1", &["p0", "p1"], &["d0"], false, false)],
        vec![("job1", "pickup", "p1"), ("job1", "pickup", "p0"), ("job1", "delivery", "d0")],
    );

    assert_eq!(check_fixed_order(&ctx), Ok(()));
}

#[test]
fn can_skip_fixed_order_check_for_unassigned_job() {
    let ctx =
        create_constraint_context(vec![create_constraint_job("job1", &["p0", "p1"], &["d0"], false, true)], vec![]);

    assert_eq!(check_fixed_order(&ctx), Ok(()));
}

#[test]
fn can_check_non_overlapping_solo_riding() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("solo", &["sp"], &["sd"], true, false),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("solo", "pickup", "sp"),
            ("solo", "delivery", "sd"),
            ("other", "pickup", "op"),
            ("other", "delivery", "od"),
        ],
    );

    assert_eq!(check_solo_riding(&ctx), Ok(()));
}

#[test]
fn can_detect_solo_pickup_while_another_job_is_onboard() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("solo", &["sp"], &["sd"], true, false),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("other", "pickup", "op"),
            ("solo", "pickup", "sp"),
            ("solo", "delivery", "sd"),
            ("other", "delivery", "od"),
        ],
    );

    let error = check_solo_riding(&ctx).unwrap_err().to_string();

    assert!(error.contains("solo job 'solo' is picked up while another job is onboard"));
}

#[test]
fn can_detect_other_pickup_while_solo_job_is_onboard() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("solo", &["sp"], &["sd"], true, false),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("solo", "pickup", "sp"),
            ("other", "pickup", "op"),
            ("other", "delivery", "od"),
            ("solo", "delivery", "sd"),
        ],
    );

    let error = check_solo_riding(&ctx).unwrap_err().to_string();

    assert!(error.contains("job 'other' is picked up while solo job 'solo' is onboard"));
}

#[test]
fn can_finish_solo_companion_job_with_unequal_activity_counts() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("solo", &["sp0", "sp1"], &["sd0"], true, true),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("solo", "pickup", "sp0"),
            ("solo", "pickup", "sp1"),
            ("solo", "delivery", "sd0"),
            ("other", "pickup", "op"),
            ("other", "delivery", "od"),
        ],
    );

    assert_eq!(check_solo_riding(&ctx), Ok(()));
}

#[test]
fn can_detect_overlap_with_solo_companion_job() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("solo", &["sp0", "sp1"], &["sd0"], true, true),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("solo", "pickup", "sp0"),
            ("solo", "pickup", "sp1"),
            ("other", "pickup", "op"),
            ("solo", "delivery", "sd0"),
            ("other", "delivery", "od"),
        ],
    );

    assert!(check_solo_riding(&ctx).is_err());
}

#[test]
fn assignment_check_includes_fixed_order_and_solo_riding() {
    let ctx = create_constraint_context(
        vec![
            create_constraint_job("fixed", &["fp0", "fp1"], &["fd0"], false, true),
            create_constraint_job("solo", &["sp"], &["sd"], true, false),
            create_constraint_job("other", &["op"], &["od"], false, false),
        ],
        vec![
            ("fixed", "pickup", "fp1"),
            ("fixed", "pickup", "fp0"),
            ("fixed", "delivery", "fd0"),
            ("solo", "pickup", "sp"),
            ("other", "pickup", "op"),
            ("other", "delivery", "od"),
            ("solo", "delivery", "sd"),
        ],
    );

    let errors = check_assignment(&ctx).unwrap_err().into_iter().map(|error| error.to_string()).collect::<Vec<_>>();

    assert!(errors.iter().any(|error| error.contains("fixed order is not respected for job 'fixed'")));
    assert!(errors.iter().any(|error| error.contains("job 'other' is picked up while solo job 'solo' is onboard")));
}

#[test]
fn can_detect_time_window_violation() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job_with_times("job1", (1., 0.), vec![(1, 2)], 1.)],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default().coordinate((0., 0.)).schedule_stamp(2., 2.).load(vec![1]).build_departure(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(3., 4.)
                        .load(vec![0])
                        .distance(1)
                        .build_single("job1", "delivery"),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(5., 5.)
                        .load(vec![0])
                        .distance(2)
                        .build_arrival(),
                ])
                .statistic(StatisticBuilder::default().driving(2).serving(1).build())
                .build(),
        )
        .build();
    let core_problem = Arc::new(problem.clone().read_pragmatic().unwrap());
    let ctx = CheckerContext::new(core_problem, problem, None, solution).unwrap();

    let result = check_assignment(&ctx);

    assert_eq!(result, Err(vec!["cannot match activities to jobs: job1:<no tag>".into()]));
}

#[test]
fn can_detect_job_duration_violation() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job_with_times("job1", (1., 0.), vec![(5, 10)], 1.)],
            ..create_empty_plan()
        },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };
    let solution = SolutionBuilder::default()
        .tour(
            TourBuilder::default()
                .stops(vec![
                    StopBuilder::default().coordinate((0., 0.)).schedule_stamp(2., 2.).load(vec![1]).build_departure(),
                    StopBuilder::default()
                        .coordinate((1., 0.))
                        .schedule_stamp(5., 7.)
                        .load(vec![0])
                        .distance(1)
                        .build_single("job1", "delivery"),
                    StopBuilder::default()
                        .coordinate((0., 0.))
                        .schedule_stamp(8., 8.)
                        .load(vec![0])
                        .distance(2)
                        .build_arrival(),
                ])
                .statistic(StatisticBuilder::default().driving(2).serving(2).waiting(2).build())
                .build(),
        )
        .build();
    let core_problem = Arc::new(problem.clone().read_pragmatic().unwrap());
    let ctx = CheckerContext::new(core_problem, problem, None, solution).unwrap();

    let result = check_assignment(&ctx);

    assert_eq!(result, Err(vec!["cannot match activities to jobs: job1:<no tag>".into()]));
}

#[test]
fn can_detect_group_violations() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job_with_group("job1", (1., 0.), "group1"),
                create_delivery_job_with_group("job2", (1., 0.), "group1"),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: vec!["v1".to_string(), "v2".to_string()],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };

    let create_tour = |vehicle_id: &str, job_id: &str| {
        TourBuilder::default()
            .vehicle_id(vehicle_id)
            .stops(vec![
                StopBuilder::default().coordinate((0., 0.)).schedule_stamp(0., 0.).load(vec![1]).build_departure(),
                StopBuilder::default()
                    .coordinate((1., 0.))
                    .schedule_stamp(1., 2.)
                    .load(vec![0])
                    .distance(1)
                    .build_single(job_id, "delivery"),
                StopBuilder::default()
                    .coordinate((0., 0.))
                    .schedule_stamp(3., 3.)
                    .load(vec![0])
                    .distance(2)
                    .build_arrival(),
            ])
            .statistic(StatisticBuilder::default().driving(2).serving(2).waiting(2).build())
            .build()
    };
    let solution = SolutionBuilder::default().tour(create_tour("v1", "job1")).tour(create_tour("v2", "job2")).build();
    let core_problem = Arc::new(problem.clone().read_pragmatic().unwrap());
    let ctx = CheckerContext::new(core_problem, problem, None, solution).unwrap();

    let result = check_groups(&ctx);

    assert_eq!(result, Err("job groups are not respected: 'group1'".into()));
}
