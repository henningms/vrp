use crate::format::problem::*;
use crate::format_time;
use crate::helpers::*;

#[test]
fn handles_empty_required_stops_array() {
    // Empty array should be treated as if the field is not present
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (5., 0.))],
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
                    required_stops: Some(vec![]), // Empty array
                    via: None,
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    // Should solve successfully
    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];

    // Should deliver the job without any required stops
    let job_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 1, "Job should be delivered");

    // No required stops should be present
    let required_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .count();

    assert_eq!(required_count, 0, "No required stops should be present");
}

#[test]
fn handles_empty_via_array() {
    // Empty via array should work without issues
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (5., 0.))],
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
                    required_stops: None,
                    via: Some(vec![]), // Empty array
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.tours.len(), 1);

    let job_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 1, "Job should be delivered");
}

#[test]
fn handles_required_stops_without_tags() {
    // Required stops without tags should still work (matched by location/time)
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (5., 0.))],
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
                    required_stops: Some(vec![JobPlace {
                        location: (3., 0.).to_loc(),
                        duration: 1.,
                        times: None,
                        tag: None, // No tag
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

    assert_eq!(solution.tours.len(), 1);

    // Should have at least one required stop activity
    let required_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .count();

    assert!(required_count > 0, "At least one required stop should be present");
}

#[test]
fn handles_via_stops_with_tight_time_windows() {
    // Via stops with very tight time windows that might not be feasible
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (10., 0.)),
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
                    required_stops: None,
                    via: Some(vec![JobPlace {
                        location: (5., 0.).to_loc(),
                        duration: 1.,
                        times: Some(vec![vec![
                            format_time(1.), // Very tight window
                            format_time(2.),
                        ]]),
                        tag: Some("via_tight".to_string()),
                        requested_time: None,
                    }]),
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    // Should still solve - via stops are optional
    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(!solution.tours.is_empty(), "Should have a tour");

    // Job should be delivered even if via stop can't be visited
    let job_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 1, "Job should be delivered");
}

#[test]
fn handles_conflicting_required_and_via_stops() {
    // Test where required and via stops might conflict
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
                            duration: 1.,
                            times: None,
                            tag: Some("req1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (7., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req2".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: Some(vec![
                        JobPlace {
                            location: (5., 0.).to_loc(), // Between required stops
                            duration: 1.,
                            times: None,
                            tag: Some("via1".to_string()),
                            requested_time: None,
                        },
                    ]),
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.tours.len(), 1);

    // Required stops must be present
    let req_tags: Vec<String> = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .filter_map(|a| a.job_tag.clone())
        .collect();

    assert!(req_tags.contains(&"req1".to_string()), "req1 should be present");
    assert!(req_tags.contains(&"req2".to_string()), "req2 should be present");

    // Job should be delivered
    let job_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 1, "Job should be delivered");
}

#[test]
fn handles_many_required_stops() {
    // Test with several required stops to verify sequence handling
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (12., 0.))],
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
                    required_stops: Some(
                        (1..=5)
                            .map(|i| JobPlace {
                                location: (i as f64 * 2., 0.).to_loc(),
                                duration: 1.,
                                times: None,
                                tag: Some(format!("req{}", i)),
                                requested_time: None,
                            })
                            .collect(),
                    ),
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

    assert_eq!(solution.tours.len(), 1);

    // All required stops should be present
    let req_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .count();

    assert_eq!(req_count, 5, "All 5 required stops should be present");

    // Verify they're in order
    let req_tags: Vec<String> = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .filter_map(|a| a.job_tag.clone())
        .collect();

    let expected: Vec<String> = (1..=5).map(|i| format!("req{}", i)).collect();
    assert_eq!(req_tags, expected, "Required stops should be in order");
}

#[test]
fn handles_via_stops_far_from_route() {
    // Via stops very far from optimal route should be skipped
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (5., 0.)),
                create_delivery_job("job2", (10., 0.)),
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
                    required_stops: None,
                    via: Some(vec![
                        JobPlace {
                            location: (100., 100.).to_loc(), // Very far
                            duration: 1.,
                            times: None,
                            tag: Some("via_far".to_string()),
                            requested_time: None,
                        },
                    ]),
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.tours.len(), 1);

    // Jobs should be delivered
    let job_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 2, "Both jobs should be delivered");

    // Far via stop should likely be skipped (optional)
    let via_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "via")
        .count();

    // Via is optional, so it's OK if it's not visited
    println!("Via stops visited: {}", via_count);
}

#[test]
fn handles_single_required_stop() {
    // Edge case: single required stop
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
                    required_stops: Some(vec![JobPlace {
                        location: (5., 0.).to_loc(),
                        duration: 1.,
                        times: None,
                        tag: Some("single_req".to_string()),
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

    assert_eq!(solution.tours.len(), 1);

    // Single required stop should be present
    let req_count = solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .count();

    assert_eq!(req_count, 1, "Single required stop should be present");
}
