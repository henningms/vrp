use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

#[test]
fn can_enforce_capacity_in_tenth_and_sixteenth_dimensions() {
    for (dimensions, layouts) in [(10, false), (16, false), (10, true), (16, true)] {
        let mut demand = vec![0; dimensions];
        demand[0] = 1;
        demand[dimensions - 1] = 1;
        let mut too_large = demand.clone();
        too_large[dimensions - 1] = 2;
        let problem = Problem {
            plan: Plan {
                jobs: vec![
                    create_delivery_job_with_demand("fits", (1., 0.), demand.clone()),
                    create_delivery_job_with_demand("too_large", (2., 0.), too_large),
                ],
                ..create_empty_plan()
            },
            fleet: Fleet {
                vehicles: vec![VehicleType {
                    shifts: vec![create_default_open_vehicle_shift()],
                    capacity: (!layouts).then_some(demand.clone()),
                    capacity_configurations: layouts.then_some(vec![CapacityConfiguration {
                        name: Some("layout".to_string()),
                        capacities: demand,
                    }]),
                    ..create_default_vehicle_type()
                }],
                ..create_default_fleet()
            },
            ..create_empty_problem()
        };
        let matrix = create_matrix_from_problem(&problem);
        let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));
        assert_eq!(solution.tours.len(), 1);
        assert!(
            solution.tours[0].stops.iter().flat_map(|stop| stop.activities()).any(|activity| activity.job_id == "fits")
        );
        let unassigned = solution.unassigned.expect("the high-dimension overflow must be refused");
        assert_eq!(unassigned.len(), 1);
        assert_eq!(unassigned[0].job_id, "too_large");
        assert_eq!(unassigned[0].reasons[0].code, "CAPACITY_CONSTRAINT");
    }
}

#[test]
fn can_use_two_dimensions() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job_with_demand("job1", (1., 0.), vec![0, 1]),
                create_delivery_job_with_demand("job2", (2., 0.), vec![1, 0]),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![create_default_open_vehicle_shift()],
                capacity: Some(vec![1, 1]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(
        solution,
        SolutionBuilder::default()
            .tour(
                TourBuilder::default()
                    .stops(vec![
                        StopBuilder::default()
                            .coordinate((0., 0.))
                            .schedule_stamp(0., 0.)
                            .load(vec![1, 1])
                            .build_departure(),
                        StopBuilder::default()
                            .coordinate((1., 0.))
                            .schedule_stamp(1., 2.)
                            .load(vec![1, 0])
                            .distance(1)
                            .build_single("job1", "delivery"),
                        StopBuilder::default()
                            .coordinate((2., 0.))
                            .schedule_stamp(3., 4.)
                            .load(vec![0, 0])
                            .distance(2)
                            .build_single("job2", "delivery"),
                    ])
                    .statistic(StatisticBuilder::default().driving(2).serving(2).build())
                    .build()
            )
            .build()
    );
}

#[test]
fn can_unassign_due_to_dimension_mismatch() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job_with_demand("job1", (1., 0.), vec![0, 1])], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![create_default_open_vehicle_shift()],
                capacity: Some(vec![1]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(
        solution,
        SolutionBuilder::default()
            .unassigned(Some(vec![UnassignedJob {
                job_id: "job1".to_string(),
                reasons: vec![UnassignedJobReason {
                    code: "CAPACITY_CONSTRAINT".to_string(),
                    description: "does not fit into any vehicle due to capacity".to_string(),
                    details: None,
                }]
            }]))
            .build()
    );
}
