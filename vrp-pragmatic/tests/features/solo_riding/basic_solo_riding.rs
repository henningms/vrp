use crate::format::problem::*;
use crate::format::solution::{UnassignedJobDetail, UnassignedJobReason};
use crate::helpers::*;

#[test]
fn can_unassign_job_due_to_solo_riding() {
    let regular_problem = Problem {
        plan: Plan {
            jobs: vec![
                create_pickup_delivery_job("solo", (1., 0.), (9., 0.)),
                create_pickup_delivery_job("job2", (8., 0.), (9., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType { capacity: Some(vec![2]), ..create_default_vehicle_type() }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };

    let mut solo_job = create_pickup_delivery_job("solo", (1., 0.), (9., 0.));
    solo_job.solo_riding = Some(true);
    let solo_riding_problem = Problem {
        plan: Plan {
            jobs: vec![solo_job, create_pickup_delivery_job("job2", (8., 0.), (9., 0.))],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType { capacity: Some(vec![2]), ..create_default_vehicle_type() }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };

    let regular_solution = solve_with_metaheuristic_and_iterations(
        regular_problem.clone(),
        Some(vec![create_matrix_from_problem(&regular_problem)]),
        500,
    );
    let solo_riding_solution = solve_with_metaheuristic_and_iterations(
        solo_riding_problem.clone(),
        Some(vec![create_matrix_from_problem(&solo_riding_problem)]),
        500,
    );

    assert!(regular_solution.unassigned.is_none());
    assert_eq!(solo_riding_solution.unassigned.as_ref().map_or(0, |u| u.len()), 1);

    let reasons = solo_riding_solution
        .unassigned
        .iter()
        .flatten()
        .flat_map(|job| job.reasons.iter().cloned())
        .collect::<Vec<_>>();

    assert_eq!(
        reasons,
        vec![UnassignedJobReason {
            code: "SOLO_RIDING_CONSTRAINT".to_string(),
            description: "cannot be assigned due to solo riding constraint".to_string(),
            details: Some(vec![UnassignedJobDetail { vehicle_id: "my_vehicle_1".to_string(), shift_index: 0 }])
        }]
    );
}
