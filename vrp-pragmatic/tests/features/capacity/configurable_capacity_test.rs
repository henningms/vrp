use crate::format::problem::*;
use crate::helpers::*;

/// Test that a vehicle with configurable capacity can serve jobs that fit
/// different configurations.
///
/// Scenario: Wheelchair-accessible minibus with configurations:
/// - Config 1: [8 seated, 0 wheelchairs] - all seated capacity
/// - Config 2: [4 seated, 1 wheelchair] - one wheelchair spot
///
/// Jobs:
/// - 2 seated passengers (demand: [2, 0])
/// - 1 wheelchair passenger (demand: [0, 1])
///
/// The solver should be able to serve all jobs using config 2.
#[test]
fn can_serve_jobs_with_configurable_capacity() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                // 2 seated passengers picked up at location (1,0)
                Job {
                    pickups: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (1., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: Some("p1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![2, 0]),
                        named_demand: None,
                        order: None,
                    }]),
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (5., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: Some("d1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![2, 0]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("seated_passengers")
                },
                // 1 wheelchair passenger picked up at location (2,0)
                Job {
                    pickups: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (2., 0.).to_loc(),
                            duration: 120.,
                            times: None,
                            tag: Some("p1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![0, 1]),
                        named_demand: None,
                        order: None,
                    }]),
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (6., 0.).to_loc(),
                            duration: 120.,
                            times: None,
                            tag: Some("d1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![0, 1]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("wheelchair_passenger")
                },
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                type_id: "accessible_minibus".to_string(),
                vehicle_ids: vec!["bus_1".to_string()],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![create_default_vehicle_shift()],
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
                    CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![4, 1] },
                ]),
                skills: None,
                limits: None,
                lifo_tags: None,
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // All jobs should be served
    assert!(solution.unassigned.is_none(), "Expected all jobs to be assigned");
    assert_eq!(solution.tours.len(), 1, "Expected exactly one tour");
}

/// Test that configurable capacity correctly rejects jobs that don't fit
/// any configuration.
///
/// Scenario: Vehicle with configurations:
/// - Config 1: [4, 0] - 4 seated, 0 wheelchairs
/// - Config 2: [2, 1] - 2 seated, 1 wheelchair
///
/// Job demanding [3, 1] cannot fit either configuration.
#[test]
fn can_reject_jobs_exceeding_all_configurations() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                // Job that needs 3 seated AND 1 wheelchair - impossible
                Job {
                    pickups: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (1., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: Some("p1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![3, 1]),
                        named_demand: None,
                        order: None,
                    }]),
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (5., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: Some("d1".to_string()),
                            requested_time: None,
                        }],
                        demand: Some(vec![3, 1]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("impossible_job")
                },
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                type_id: "small_bus".to_string(),
                vehicle_ids: vec!["bus_1".to_string()],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![create_default_vehicle_shift()],
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![4, 0] },
                    CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![2, 1] },
                ]),
                skills: None,
                limits: None,
                lifo_tags: None,
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // The job should be unassigned because it exceeds all configurations
    assert!(solution.unassigned.is_some(), "Expected job to be unassigned");
    assert_eq!(solution.unassigned.as_ref().unwrap().len(), 1);
}

/// Test that named demand works correctly with capacity dimensions.
///
/// Uses namedDemand: {"wheelchair": 1} instead of positional demand: [0, 1]
#[test]
fn can_use_named_demand_with_capacity_dimensions() {
    let mut named_demand_pickup = std::collections::HashMap::new();
    named_demand_pickup.insert("wheelchair".to_string(), 1);

    let mut named_demand_delivery = std::collections::HashMap::new();
    named_demand_delivery.insert("wheelchair".to_string(), 1);

    let problem = Problem {
        plan: Plan {
            jobs: vec![Job {
                pickups: Some(vec![JobTask {
                    places: vec![JobPlace {
                        location: (1., 0.).to_loc(),
                        duration: 120.,
                        times: None,
                        tag: Some("p1".to_string()),
                        requested_time: None,
                    }],
                    demand: None,
                    named_demand: Some(named_demand_pickup),
                    order: None,
                }]),
                deliveries: Some(vec![JobTask {
                    places: vec![JobPlace {
                        location: (5., 0.).to_loc(),
                        duration: 120.,
                        times: None,
                        tag: Some("d1".to_string()),
                        requested_time: None,
                    }],
                    demand: None,
                    named_demand: Some(named_demand_delivery),
                    order: None,
                }]),
                ..create_job("wheelchair_user")
            }],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                type_id: "accessible_vehicle".to_string(),
                vehicle_ids: vec!["v1".to_string()],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![create_default_vehicle_shift()],
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("with_wheelchair".to_string()), capacities: vec![4, 1] },
                ]),
                skills: None,
                limits: None,
                lifo_tags: None,
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // Job should be served
    assert!(solution.unassigned.is_none(), "Expected job to be assigned");
    assert_eq!(solution.tours.len(), 1);
}

/// Test multiple jobs with different accessibility needs using configurable capacity.
///
/// Scenario: 3 accessibility features (seated, wheelchair, stroller)
/// Vehicle configurations:
/// - [10, 0, 0] - all seated
/// - [6, 1, 0] - one wheelchair
/// - [6, 0, 2] - two strollers
/// - [4, 1, 1] - mixed accessibility
///
/// Jobs:
/// - 2 seated passengers
/// - 1 wheelchair user
/// - 1 stroller user
///
/// Expected: All jobs assigned using configuration [4, 1, 1]
#[test]
fn can_handle_multiple_accessibility_features() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                // Seated passengers
                Job {
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (1., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: None,
                            requested_time: None,
                        }],
                        demand: Some(vec![2, 0, 0]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("seated")
                },
                // Wheelchair user
                Job {
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (2., 0.).to_loc(),
                            duration: 120.,
                            times: None,
                            tag: None,
                            requested_time: None,
                        }],
                        demand: Some(vec![0, 1, 0]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("wheelchair")
                },
                // Stroller user
                Job {
                    deliveries: Some(vec![JobTask {
                        places: vec![JobPlace {
                            location: (3., 0.).to_loc(),
                            duration: 60.,
                            times: None,
                            tag: None,
                            requested_time: None,
                        }],
                        demand: Some(vec![0, 0, 1]),
                        named_demand: None,
                        order: None,
                    }]),
                    ..create_job("stroller")
                },
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                type_id: "accessible_bus".to_string(),
                vehicle_ids: vec!["bus_1".to_string()],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![create_default_vehicle_shift()],
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![10, 0, 0] },
                    CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![6, 1, 0] },
                    CapacityConfiguration { name: Some("two_strollers".to_string()), capacities: vec![6, 0, 2] },
                    CapacityConfiguration {
                        name: Some("mixed".to_string()),
                        capacities: vec![4, 1, 1],
                    },
                ]),
                skills: None,
                limits: None,
                lifo_tags: None,
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string(), "stroller".to_string()]),
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    // All jobs should be assigned
    assert!(solution.unassigned.is_none(), "Expected all jobs to be assigned");
    assert_eq!(solution.tours.len(), 1);
}
