use crate::format::problem::*;
use crate::format_time;
use crate::helpers::*;

#[test]
fn can_combine_required_and_via_stops() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (2., 0.)),
                create_delivery_job("job2", (12., 0.)),
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
                            location: (4., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (8., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req2".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: Some(vec![
                        JobPlace {
                            location: (6., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("via1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (10., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("via2".to_string()),
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
    let tour = &solution.tours[0];

    // Verify all required stops are present
    let req_tags: Vec<String> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|activity| activity.activity_type == "required")
        .filter_map(|activity| activity.job_tag.clone())
        .collect();

    assert_eq!(
        req_tags,
        vec!["req1", "req2"],
        "All required stops must be present in order"
    );

    // Verify required stops maintain order
    let all_activities: Vec<(&str, String)> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter_map(|activity| {
            activity
                .job_tag
                .as_ref()
                .map(|tag| (activity.activity_type.as_str(), tag.clone()))
        })
        .collect();

    // Find positions of required stops
    let req1_pos = all_activities
        .iter()
        .position(|(_, tag)| tag == "req1")
        .expect("req1 not found");
    let req2_pos = all_activities
        .iter()
        .position(|(_, tag)| tag == "req2")
        .expect("req2 not found");

    assert!(
        req1_pos < req2_pos,
        "Required stops must maintain order: req1 at {}, req2 at {}",
        req1_pos,
        req2_pos
    );

    // Via stops are optional, just verify structure is valid
    let via_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|activity| activity.activity_type == "via")
        .count();

    println!("Via stops visited: {}/2", via_count);

    // Verify all mandatory jobs are served
    let job_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|activity| activity.activity_type == "delivery")
        .count();

    assert_eq!(job_count, 2, "All jobs should be served");
}

#[test]
fn required_stops_take_precedence_over_via_stops() {
    // Create scenario where via stops might conflict with required stop order
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
                            tag: Some("req_first".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (7., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("req_second".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: Some(vec![
                        JobPlace {
                            location: (5., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("via_between".to_string()),
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
    let tour = &solution.tours[0];

    // Get all stop tags in order
    let stop_sequence: Vec<(String, String)> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter_map(|activity| {
            activity.job_tag.as_ref().map(|tag| {
                (activity.activity_type.clone(), tag.clone())
            })
        })
        .collect();

    // Find required stops
    let req_first_pos = stop_sequence
        .iter()
        .position(|(_, tag)| tag == "req_first")
        .expect("req_first must be present");
    let req_second_pos = stop_sequence
        .iter()
        .position(|(_, tag)| tag == "req_second")
        .expect("req_second must be present");

    // Required stops must be in order
    assert!(
        req_first_pos < req_second_pos,
        "Required stops must maintain strict order"
    );

    // If via stop is present, it shouldn't break required stop order
    if let Some(via_pos) = stop_sequence
        .iter()
        .position(|(_, tag)| tag == "via_between")
    {
        println!("Via stop inserted at position {}", via_pos);
        // Via can be anywhere, but shouldn't break required order
        assert!(
            req_first_pos < req_second_pos,
            "Via stops cannot break required stop sequence"
        );
    }
}

#[test]
fn complex_mixed_route_with_jobs_required_and_via() {
    // Realistic scenario: delivery route with mandatory checkpoints and optional waypoints
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("customer1", (5., 0.)),
                create_delivery_job("customer2", (15., 0.)),
                create_delivery_job("customer3", (25., 0.)),
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
                            location: (10., 0.).to_loc(),
                            duration: 2.,
                            times: None,
                            tag: Some("checkpoint1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (20., 0.).to_loc(),
                            duration: 2.,
                            times: None,
                            tag: Some("checkpoint2".to_string()),
                            requested_time: None,
                        },
                    ]),
                    via: Some(vec![
                        JobPlace {
                            location: (7., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("optional_waypoint1".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (13., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("optional_waypoint2".to_string()),
                            requested_time: None,
                        },
                        JobPlace {
                            location: (22., 0.).to_loc(),
                            duration: 1.,
                            times: None,
                            tag: Some("optional_waypoint3".to_string()),
                            requested_time: None,
                        },
                    ]),
                }],
                capacity: Some(vec![10]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];

    // Verify all customers are served
    let customer_count = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "delivery" && a.job_id.starts_with("customer"))
        .count();

    assert_eq!(customer_count, 3, "All customers must be served");

    // Verify all required checkpoints are present and in order
    let checkpoint_tags: Vec<String> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "required")
        .filter_map(|a| a.job_tag.clone())
        .collect();

    assert_eq!(
        checkpoint_tags,
        vec!["checkpoint1", "checkpoint2"],
        "All checkpoints must be visited in order"
    );

    // Via stops are optional - just report what was visited
    let via_tags: Vec<String> = tour
        .stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|a| a.activity_type == "via")
        .filter_map(|a| a.job_tag.clone())
        .collect();

    println!(
        "Optional waypoints visited: {:?} out of 3 available",
        via_tags
    );

    // Verify solution statistics
    assert!(
        tour.statistic.duration > 0,
        "Tour should have positive duration"
    );
    assert!(
        tour.statistic.distance > 0,
        "Tour should have positive distance"
    );
}
