use crate::format::problem::*;
use crate::helpers::*;

#[test]
fn can_assign_solo_and_other_jobs_sequentially() {
    // Solo riding only forbids OVERLAP — once the solo job is delivered, the vehicle
    // is free to take additional jobs. With no time-window pressure, both jobs fit.
    let mut solo_job = create_pickup_delivery_job("solo", (1., 0.), (9., 0.));
    solo_job.solo_riding = Some(true);
    let problem = Problem {
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

    let solution = solve_with_metaheuristic_and_iterations(
        problem.clone(),
        Some(vec![create_matrix_from_problem(&problem)]),
        500,
    );

    assert!(solution.unassigned.is_none(), "solo and other job should both be assigned sequentially");
}

#[test]
fn can_unassign_job_due_to_solo_riding() {
    // Time windows force the only feasible joint plan to be interleaved (both onboard
    // simultaneously). Solo riding then blocks that plan, leaving one job unassigned.
    let solo_pickup_window = vec![(0, 5)];
    let solo_delivery_window = vec![(9, 15)];
    let job2_pickup_window = vec![(5, 8)];
    let job2_delivery_window = vec![(9, 13)];

    let regular_problem = Problem {
        plan: Plan {
            jobs: vec![
                create_pickup_delivery_job_with_params(
                    "solo",
                    vec![1],
                    ((1., 0.), 0., solo_pickup_window.clone()),
                    ((9., 0.), 0., solo_delivery_window.clone()),
                ),
                create_pickup_delivery_job_with_params(
                    "job2",
                    vec![1],
                    ((8., 0.), 0., job2_pickup_window.clone()),
                    ((9., 0.), 0., job2_delivery_window.clone()),
                ),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType { capacity: Some(vec![2]), ..create_default_vehicle_type() }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };

    let mut solo_job = create_pickup_delivery_job_with_params(
        "solo",
        vec![1],
        ((1., 0.), 0., solo_pickup_window),
        ((9., 0.), 0., solo_delivery_window),
    );
    solo_job.solo_riding = Some(true);
    let solo_riding_problem = Problem {
        plan: Plan {
            jobs: vec![
                solo_job,
                create_pickup_delivery_job_with_params(
                    "job2",
                    vec![1],
                    ((8., 0.), 0., job2_pickup_window),
                    ((9., 0.), 0., job2_delivery_window),
                ),
            ],
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

    // Without solo riding, the only feasible plan is interleaved and both jobs fit.
    assert!(regular_solution.unassigned.is_none(), "regular problem should fit both jobs interleaved");
    // Solo riding blocks the interleaved plan, leaving the second job unassigned. The most-frequent
    // failure code on the route may be reported as TIME_WINDOW (the sequential alternative), but
    // the unassignment itself is caused by solo riding — proven by the regular problem fitting both.
    assert_eq!(
        solo_riding_solution.unassigned.as_ref().map_or(0, |u| u.len()),
        1,
        "solo riding should force one job to be unassigned"
    );
}
