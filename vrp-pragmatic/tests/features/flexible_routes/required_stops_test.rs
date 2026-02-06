use crate::format::problem::*;
use crate::format_time;
use crate::helpers::*;

#[test]
fn can_enforce_required_stops_order() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (1., 0.)),
                create_delivery_job("job2", (9., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                costs: create_default_vehicle_costs(),
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: format_time(0.),
                        latest: None,
                        location: (0., 0.).to_loc(),
                    },
                    end: Some(ShiftEnd {
                        earliest: None,
                        latest: format_time(1000.),
                        location: (0., 0.).to_loc(),
                    }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: Some(vec![
                        JobPlace {
                            location: (3., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (5., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req2".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (7., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req3".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: None,
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // Verify that required stops are present and in order
    let tour = &solution.tours[0];

    // Verify tags are in correct order
    let req_tags: Vec<String> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|activity| activity.activity_type == "required")
        .filter_map(|activity| activity.job_tag.clone())
        .collect();

    // All required stops should be present
    assert_eq!(req_tags.len(), 3, "Expected exactly 3 required stops");

    // Verify they appear in the correct order
    assert_eq!(req_tags, vec!["req1", "req2", "req3"], "Required stops must be in order");

    // Verify all jobs are served
    let job_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 2, "Both delivery jobs should be served");
}

#[test]
fn can_handle_required_stops_with_time_windows() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (10., 0.))],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                costs: create_default_vehicle_costs(),
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: format_time(0.),
                        latest: None,
                        location: (0., 0.).to_loc(),
                    },
                    end: Some(ShiftEnd {
                        earliest: None,
                        latest: format_time(1000.),
                        location: (0., 0.).to_loc(),
                    }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: Some(vec![
                        JobPlace {
                            location: (3., 0.).to_loc(),
                            duration: 2.,
                            times: Some(vec![vec![format_time(3.), format_time(10.)]]),
                            tag: Some("req_early".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (6., 0.).to_loc(),
                            duration: 2.,
                            times: Some(vec![vec![format_time(10.), format_time(20.)]]),
                            tag: Some("req_late".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: None,
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // Verify required stops are present
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];

    // Count required stop activities
    let req_activities: Vec<_> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .collect();

    // At least one required stop should be visited
    assert!(
        !req_activities.is_empty(),
        "At least one required stop should be present"
    );

    // Verify the job is served
    let job_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 1, "Delivery job should be served");
}

#[test]
fn required_stops_work_with_multiple_vehicle_ids() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (5., 0.)),
                create_delivery_job("job2", (5., 5.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: vec!["vehicle_1".to_string(), "vehicle_2".to_string()],
                costs: create_default_vehicle_costs(),
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: format_time(0.),
                        latest: None,
                        location: (0., 0.).to_loc(),
                    },
                    end: Some(ShiftEnd {
                        earliest: None,
                        latest: format_time(1000.),
                        location: (0., 0.).to_loc(),
                    }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: Some(vec![JobPlace {
                        location: (2., 0.).to_loc(),
                        duration: 1.,
                        times: None,
                        tag: Some("req".to_string()),
                        requested_time: None,
                    }]),
                    via: None,
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // Both vehicles should visit their required stop
    assert_eq!(solution.tours.len(), 2);

    for tour in &solution.tours {
        let has_required = tour
            .stops
            .iter()
            .flat_map(|stop| stop.activities().iter())
            .any(|a| a.activity_type == "required");

        assert!(has_required, "Each vehicle must visit its required stop");
    }
}
