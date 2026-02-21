use crate::format::feasibility::FeasibilityContext;
use crate::format::problem::*;
use crate::format::Location;
use crate::helpers::*;
use std::sync::Arc;
use std::time::Instant;
use vrp_core::construction::heuristics::InsertionContext;
use vrp_core::models::Problem as CoreProblem;
use vrp_core::solver::{Solver, VrpConfigBuilder};
use vrp_core::utils::{Environment, Parallelism};

/// Helper: build a minimal problem with given vehicles and jobs, plus a matching matrix.
fn build_problem_and_matrix(
    vehicles: Vec<VehicleType>,
    jobs: Vec<Job>,
) -> (Problem, Matrix) {
    let problem = Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet { vehicles, ..create_default_fleet() },
        objectives: None,
    };
    let matrix = create_matrix_from_problem(&problem);
    (problem, matrix)
}

/// Helper: build a minimal solution JSON for one vehicle with given stops served.
fn build_solution_json(
    vehicle_id: &str,
    type_id: &str,
    depot: (f64, f64),
    stops: Vec<(&str, &str, (f64, f64))>, // (job_id, activity_type, location)
    capacity: i32,
) -> String {
    let depot_loc = format!(r#"{{ "lat": {}, "lng": {} }}"#, depot.0, depot.1);

    let mut load = capacity;
    let mut stop_json_parts = Vec::new();

    // departure stop
    stop_json_parts.push(format!(
        r#"{{
            "location": {depot_loc},
            "time": {{ "arrival": "1970-01-01T00:00:00Z", "departure": "1970-01-01T00:00:00Z" }},
            "distance": 0,
            "load": [{load}],
            "activities": [{{ "jobId": "departure", "type": "departure" }}]
        }}"#
    ));

    // job stops
    for (job_id, activity_type, loc) in &stops {
        let loc_json = format!(r#"{{ "lat": {}, "lng": {} }}"#, loc.0, loc.1);
        // Simple load tracking for single-dim: delivery decreases, pickup increases
        match *activity_type {
            "delivery" => load -= 1,
            "pickup" => load += 1,
            _ => {}
        }
        stop_json_parts.push(format!(
            r#"{{
                "location": {loc_json},
                "time": {{ "arrival": "1970-01-01T00:00:01Z", "departure": "1970-01-01T00:00:02Z" }},
                "distance": 1,
                "load": [{load}],
                "activities": [{{ "jobId": "{job_id}", "type": "{activity_type}" }}]
            }}"#
        ));
    }

    // arrival stop
    stop_json_parts.push(format!(
        r#"{{
            "location": {depot_loc},
            "time": {{ "arrival": "1970-01-01T00:00:10Z", "departure": "1970-01-01T00:00:10Z" }},
            "distance": 2,
            "load": [{load}],
            "activities": [{{ "jobId": "arrival", "type": "arrival" }}]
        }}"#
    ));

    let stops_json = stop_json_parts.join(",\n");

    format!(
        r#"{{
            "statistic": {{ "cost": 0, "distance": 0, "duration": 0,
                "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }},
            "tours": [{{
                "vehicleId": "{vehicle_id}",
                "typeId": "{type_id}",
                "shiftIndex": 0,
                "stops": [{stops_json}],
                "statistic": {{ "cost": 0, "distance": 0, "duration": 0,
                    "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }}
            }}]
        }}"#
    )
}

#[test]
fn can_check_feasible_insertion_with_capacity() {
    // One vehicle with capacity 10, one job already assigned
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![10])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        10,
    );

    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Candidate: a delivery job with demand=1 — should fit (9 remaining capacity)
    let candidate = create_delivery_job("candidate1", (2.0, 0.0));
    let result = ctx.check_job(&candidate).expect("check_job failed");

    assert!(result.is_feasible);
    assert_eq!(result.vehicles.len(), 1);
    assert!(result.vehicles[0].is_feasible);
    assert!(result.vehicles[0].cost_delta.is_some());
    assert!(result.vehicles[0].violations.is_empty());
}

#[test]
fn can_detect_infeasible_capacity_constraint() {
    // One vehicle with capacity 1, one job already assigned (fills capacity)
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![1])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        1,
    );

    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Candidate: another delivery with demand=1 — should NOT fit
    let candidate = create_delivery_job("candidate1", (2.0, 0.0));
    let result = ctx.check_job(&candidate).expect("check_job failed");

    assert!(!result.is_feasible);
    assert_eq!(result.vehicles.len(), 1);
    assert!(!result.vehicles[0].is_feasible);
    assert!(result.vehicles[0].cost_delta.is_none());
    assert!(!result.vehicles[0].violations.is_empty());
    assert_eq!(result.vehicles[0].violations[0].code, "CAPACITY_CONSTRAINT");
}

#[test]
fn can_check_multi_vehicle_mixed_results() {
    // Two vehicles: one with capacity 1 (full), one with capacity 10 (has room)
    let vehicles = vec![
        VehicleType {
            type_id: "small".to_string(),
            vehicle_ids: vec!["small_1".to_string()],
            capacity: Some(vec![1]),
            ..create_default_vehicle("small")
        },
        VehicleType {
            type_id: "large".to_string(),
            vehicle_ids: vec!["large_1".to_string()],
            capacity: Some(vec![10]),
            ..create_default_vehicle("large")
        },
    ];

    let jobs = vec![
        create_delivery_job("job1", (1.0, 0.0)),
        create_delivery_job("job2", (2.0, 0.0)),
    ];

    let (problem, matrix) = build_problem_and_matrix(vehicles, jobs);

    // Small vehicle is full (1 delivery assigned), large has one delivery too
    let small_depot_loc = r#"{ "lat": 0.0, "lng": 0.0 }"#;
    let large_depot_loc = r#"{ "lat": 0.0, "lng": 0.0 }"#;

    let solution_json = format!(
        r#"{{
            "statistic": {{ "cost": 0, "distance": 0, "duration": 0,
                "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }},
            "tours": [
                {{
                    "vehicleId": "small_1",
                    "typeId": "small",
                    "shiftIndex": 0,
                    "stops": [
                        {{ "location": {small_depot_loc}, "time": {{ "arrival": "1970-01-01T00:00:00Z", "departure": "1970-01-01T00:00:00Z" }}, "distance": 0, "load": [1], "activities": [{{ "jobId": "departure", "type": "departure" }}] }},
                        {{ "location": {{ "lat": 1.0, "lng": 0.0 }}, "time": {{ "arrival": "1970-01-01T00:00:01Z", "departure": "1970-01-01T00:00:02Z" }}, "distance": 1, "load": [0], "activities": [{{ "jobId": "job1", "type": "delivery" }}] }},
                        {{ "location": {small_depot_loc}, "time": {{ "arrival": "1970-01-01T00:00:10Z", "departure": "1970-01-01T00:00:10Z" }}, "distance": 2, "load": [0], "activities": [{{ "jobId": "arrival", "type": "arrival" }}] }}
                    ],
                    "statistic": {{ "cost": 0, "distance": 0, "duration": 0, "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }}
                }},
                {{
                    "vehicleId": "large_1",
                    "typeId": "large",
                    "shiftIndex": 0,
                    "stops": [
                        {{ "location": {large_depot_loc}, "time": {{ "arrival": "1970-01-01T00:00:00Z", "departure": "1970-01-01T00:00:00Z" }}, "distance": 0, "load": [10], "activities": [{{ "jobId": "departure", "type": "departure" }}] }},
                        {{ "location": {{ "lat": 2.0, "lng": 0.0 }}, "time": {{ "arrival": "1970-01-01T00:00:01Z", "departure": "1970-01-01T00:00:02Z" }}, "distance": 1, "load": [9], "activities": [{{ "jobId": "job2", "type": "delivery" }}] }},
                        {{ "location": {large_depot_loc}, "time": {{ "arrival": "1970-01-01T00:00:10Z", "departure": "1970-01-01T00:00:10Z" }}, "distance": 2, "load": [9], "activities": [{{ "jobId": "arrival", "type": "arrival" }}] }}
                    ],
                    "statistic": {{ "cost": 0, "distance": 0, "duration": 0, "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }}
                }}
            ]
        }}"#
    );

    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    let candidate = create_delivery_job("candidate1", (3.0, 0.0));
    let result = ctx.check_job(&candidate).expect("check_job failed");

    // Should be feasible overall (fits in large)
    assert!(result.is_feasible);
    assert_eq!(result.vehicles.len(), 2);

    // Find results by vehicle id
    let small = result.vehicles.iter().find(|v| v.vehicle_id == "small_1").unwrap();
    let large = result.vehicles.iter().find(|v| v.vehicle_id == "large_1").unwrap();

    assert!(!small.is_feasible, "small vehicle should be infeasible");
    assert!(large.is_feasible, "large vehicle should be feasible");
}

#[test]
fn can_check_pickup_delivery_job_insertion() {
    // A vehicle with capacity 10, one existing delivery
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![10])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        10,
    );

    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Candidate: a pickup-delivery (multi) job
    let candidate = create_pickup_delivery_job("pd_candidate", (2.0, 0.0), (3.0, 0.0));
    let result = ctx.check_job(&candidate).expect("check_job failed");

    assert!(result.is_feasible);
    assert_eq!(result.vehicles.len(), 1);
    assert!(result.vehicles[0].is_feasible);
}

#[test]
fn can_detect_skills_constraint_violation() {
    // Vehicle with skills ["fragile"], existing job has NO skills requirement.
    // Candidate requires ["electronics"] which vehicle doesn't have.
    // The skills constraint is still active thanks to with_all_constraints_enabled().
    let vehicles = vec![VehicleType {
        skills: Some(vec!["fragile".to_string()]),
        ..create_default_vehicle("my_vehicle")
    }];

    // Existing job has no skills — verifies the constraint is active even when
    // the original problem wouldn't normally enable it.
    let jobs = vec![create_delivery_job("job1", (1.0, 0.0))];
    let (problem, matrix) = build_problem_and_matrix(vehicles, jobs);

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        10,
    );

    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Candidate requires skill "electronics" which vehicle doesn't have
    let candidate = create_delivery_job_with_skills(
        "candidate1",
        (2.0, 0.0),
        all_of_skills(vec!["electronics".to_string()]),
    );
    let result = ctx.check_job(&candidate).expect("check_job failed");

    assert!(!result.is_feasible);
    assert_eq!(result.vehicles.len(), 1);
    assert!(!result.vehicles[0].is_feasible);
    assert_eq!(result.vehicles[0].violations[0].code, "SKILL_CONSTRAINT");
}

#[test]
fn can_check_feasibility_at_scale_500_jobs_50_vehicles() {
    let num_vehicles: usize = 50;
    let jobs_per_vehicle: usize = 10; // 500 jobs total
    let total_jobs = num_vehicles * jobs_per_vehicle;
    let capacity = 100;

    // Use index-based locations: index 0 = depot, indices 1..=500 = job locations
    // Candidate will reuse an existing index for location lookup
    let total_locations = total_jobs + 1; // depot + jobs
    let matrix_size = total_locations * total_locations;

    // Generate vehicles: all share the same depot at index 0
    let vehicles: Vec<VehicleType> = (0..num_vehicles)
        .map(|i| {
            let id = format!("v{i}");
            VehicleType {
                type_id: id.clone(),
                vehicle_ids: vec![format!("{id}_1")],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: "1970-01-01T00:00:00Z".to_string(),
                        latest: None,
                        location: Location::Reference { index: 0 },
                    },
                    end: Some(ShiftEnd {
                        earliest: None,
                        latest: "1970-01-01T00:16:40Z".to_string(),
                        location: Location::Reference { index: 0 },
                    }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: None,
                    via: None,
                }],
                capacity: Some(vec![capacity]),
                capacity_configurations: None,
                skills: None,
                limits: None,
                lifo_tags: None,
            }
        })
        .collect();

    // Generate 500 jobs at indices 1..=500
    let jobs: Vec<Job> = (0..total_jobs)
        .map(|idx| create_delivery_job_with_index(&format!("job{idx}"), idx + 1))
        .collect();

    let problem = Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet { vehicles, ..create_default_fleet() },
        objectives: None,
    };

    let matrix = Matrix {
        profile: Some("car".to_string()),
        timestamp: None,
        travel_times: vec![10; matrix_size],
        distances: vec![10; matrix_size],
        error_codes: None,
    };

    // Build solution: each vehicle serves its 10 jobs
    let tours: Vec<String> = (0..num_vehicles)
        .map(|v| {
            let vid = format!("v{v}_1");
            let tid = format!("v{v}");
            let mut load = capacity;

            let mut stops = Vec::new();
            // departure at depot (index 0)
            stops.push(format!(
                r#"{{ "location": {{ "index": 0 }}, "time": {{ "arrival": "1970-01-01T00:00:00Z", "departure": "1970-01-01T00:00:00Z" }}, "distance": 0, "load": [{load}], "activities": [{{ "jobId": "departure", "type": "departure" }}] }}"#
            ));

            // job stops
            for j in 0..jobs_per_vehicle {
                let idx = v * jobs_per_vehicle + j;
                let loc_idx = idx + 1;
                load -= 1;
                stops.push(format!(
                    r#"{{ "location": {{ "index": {loc_idx} }}, "time": {{ "arrival": "1970-01-01T00:00:01Z", "departure": "1970-01-01T00:00:02Z" }}, "distance": 10, "load": [{load}], "activities": [{{ "jobId": "job{idx}", "type": "delivery" }}] }}"#
                ));
            }

            // arrival at depot
            stops.push(format!(
                r#"{{ "location": {{ "index": 0 }}, "time": {{ "arrival": "1970-01-01T00:00:10Z", "departure": "1970-01-01T00:00:10Z" }}, "distance": 20, "load": [{load}], "activities": [{{ "jobId": "arrival", "type": "arrival" }}] }}"#
            ));

            let stops_json = stops.join(",\n");
            format!(
                r#"{{ "vehicleId": "{vid}", "typeId": "{tid}", "shiftIndex": 0, "stops": [{stops_json}], "statistic": {{ "cost": 0, "distance": 0, "duration": 0, "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }} }}"#
            )
        })
        .collect();

    let tours_json = tours.join(",\n");
    let solution_json = format!(
        r#"{{ "statistic": {{ "cost": 0, "distance": 0, "duration": 0, "times": {{ "driving": 0, "serving": 0, "waiting": 0, "break": 0 }} }}, "tours": [{tours_json}] }}"#
    );

    // Build context (one-time cost)
    let ctx_start = Instant::now();
    let ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");
    let ctx_duration = ctx_start.elapsed();

    // Candidate at an existing index (reuses location 1 from the matrix)
    let candidate = create_delivery_job_with_index("candidate_new", 1);

    // Single check
    let check_start = Instant::now();
    let result = ctx.check_job(&candidate).expect("check_job failed");
    let check_duration = check_start.elapsed();

    // Batch of 10 checks (reusing same candidate to isolate check_job cost)
    let batch_start = Instant::now();
    for _ in 0..10 {
        ctx.check_job(&candidate).expect("batch check failed");
    }
    let batch_duration = batch_start.elapsed();

    eprintln!();
    eprintln!("=== Feasibility scale test: 500 jobs, 50 vehicles ===");
    eprintln!("Context build:    {:?}", ctx_duration);
    eprintln!("Single check_job: {:?}", check_duration);
    eprintln!("10x check_job:    {:?} ({:?}/check)", batch_duration, batch_duration / 10);
    eprintln!("=====================================================");
    eprintln!();

    // Verify correctness
    assert!(result.is_feasible);
    assert_eq!(result.vehicles.len(), num_vehicles);
    assert!(result.vehicles.iter().all(|v| v.is_feasible));
    assert!(check_duration.as_millis() < 5000, "check_job took too long: {:?}", check_duration);
}

#[test]
fn can_accept_job_and_update_state() {
    // One vehicle with capacity 10, one job already assigned
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![10])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        10,
    );

    let mut ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Accept a new job — should succeed
    let candidate = create_delivery_job("candidate1", (2.0, 0.0));
    let result = ctx.accept_job(&candidate).expect("accept_job failed");

    assert!(result.is_feasible);
    assert_eq!(result.vehicle_id, "my_vehicle_1");
    assert!(result.cost_delta.is_some());

    // Now check another job against the updated state — should still fit (8 remaining)
    let candidate2 = create_delivery_job("candidate2", (3.0, 0.0));
    let check = ctx.check_job(&candidate2).expect("check_job failed");
    assert!(check.is_feasible);
}

#[test]
fn can_reject_infeasible_accept() {
    // One vehicle with capacity 1, already full
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![1])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        1,
    );

    let mut ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Try to accept — should fail
    let candidate = create_delivery_job("candidate1", (2.0, 0.0));
    let result = ctx.accept_job(&candidate);
    assert!(result.is_err());
}

#[test]
fn can_serialize_solution_after_accept() {
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![10])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        10,
    );

    let mut ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Accept a job
    let candidate = create_delivery_job("candidate1", (2.0, 0.0));
    ctx.accept_job(&candidate).expect("accept_job failed");

    // Serialize the solution
    let json = ctx.to_solution_json().expect("to_solution_json failed");

    // The JSON should contain the new job
    assert!(json.contains("candidate1"), "serialized solution should contain the accepted job");
    // And also the original job
    assert!(json.contains("job1"), "serialized solution should contain the original job");
}

#[test]
fn can_accept_multiple_jobs_sequentially() {
    // One vehicle with capacity 3, one job already assigned (2 remaining)
    let (problem, matrix) = build_problem_and_matrix(
        vec![create_vehicle_with_capacity("my_vehicle", vec![3])],
        vec![create_delivery_job("job1", (1.0, 0.0))],
    );

    let solution_json = build_solution_json(
        "my_vehicle_1",
        "my_vehicle",
        (0.0, 0.0),
        vec![("job1", "delivery", (1.0, 0.0))],
        3,
    );

    let mut ctx = FeasibilityContext::new(problem, vec![matrix], &solution_json)
        .expect("cannot build context");

    // Accept first — should succeed (1 remaining)
    let c1 = create_delivery_job("c1", (2.0, 0.0));
    ctx.accept_job(&c1).expect("first accept should succeed");

    // Accept second — should succeed (0 remaining)
    let c2 = create_delivery_job("c2", (3.0, 0.0));
    ctx.accept_job(&c2).expect("second accept should succeed");

    // Accept third — should fail (no capacity)
    let c3 = create_delivery_job("c3", (4.0, 0.0));
    let result = ctx.accept_job(&c3);
    assert!(result.is_err(), "third accept should fail due to capacity");
}

/// Compares feasibility check vs solver re-optimization at scale.
///
/// 1. Builds a 500-job / 50-vehicle problem, solves from scratch to get a baseline
/// 2. Adds one new job → feasibility check (fast path)
/// 3. Adds the same job → solver with initial solution, 500 generations (slow path)
/// 4. Reports timing and solution quality for both
#[test]
fn compare_feasibility_vs_solver_500_jobs_50_vehicles() {
    let num_vehicles: usize = 50;
    let jobs_per_vehicle: usize = 10; // 500 total
    let total_jobs = num_vehicles * jobs_per_vehicle;
    let capacity = 100;
    let total_locations = total_jobs + 1; // depot (0) + 500 job locations (1..=500)
    let matrix_size = total_locations * total_locations;

    // --- Build the base problem (500 jobs) ---
    let vehicles: Vec<VehicleType> = (0..num_vehicles)
        .map(|i| {
            let id = format!("v{i}");
            VehicleType {
                type_id: id.clone(),
                vehicle_ids: vec![format!("{id}_1")],
                profile: create_default_vehicle_profile(),
                costs: create_default_vehicle_costs(),
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: "1970-01-01T00:00:00Z".to_string(),
                        latest: None,
                        location: Location::Reference { index: 0 },
                    },
                    end: Some(ShiftEnd {
                        earliest: None,
                        latest: "1970-01-01T00:16:40Z".to_string(),
                        location: Location::Reference { index: 0 },
                    }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: None,
                    via: None,
                }],
                capacity: Some(vec![capacity]),
                capacity_configurations: None,
                skills: None,
                limits: None,
                lifo_tags: None,
            }
        })
        .collect();

    let base_jobs: Vec<Job> = (0..total_jobs)
        .map(|idx| create_delivery_job_with_index(&format!("job{idx}"), idx + 1))
        .collect();

    let base_matrix = Matrix {
        profile: Some("car".to_string()),
        timestamp: None,
        travel_times: vec![10; matrix_size],
        distances: vec![10; matrix_size],
        error_codes: None,
    };

    // --- Solve the base problem to get a good initial solution ---
    let base_problem = Problem {
        plan: Plan { jobs: base_jobs.clone(), ..create_empty_plan() },
        fleet: Fleet { vehicles: vehicles.clone(), ..create_default_fleet() },
        objectives: None,
    };

    eprintln!();
    eprintln!("=== Step 1: Solving base problem (500 jobs, 50 vehicles, 200 gen) ===");

    let base_solve_start = Instant::now();
    let base_solution = solve_with_metaheuristic_and_iterations(
        base_problem.clone(),
        Some(vec![base_matrix.clone()]),
        200,
    );
    let base_solve_duration = base_solve_start.elapsed();

    let base_cost = base_solution.statistic.cost;
    let base_unassigned = base_solution.unassigned.as_ref().map_or(0, |u| u.len());
    let base_routes = base_solution.tours.len();

    eprintln!("Base solve:       {:?}", base_solve_duration);
    eprintln!("Base cost:        {:.2}", base_cost);
    eprintln!("Base routes:      {}", base_routes);
    eprintln!("Base unassigned:  {}", base_unassigned);

    // --- Feasibility check: can the new job fit into the existing solution? ---
    eprintln!();
    eprintln!("=== Step 2: Feasibility check for new job ===");

    // Serialize the base solution to JSON for the feasibility context
    let mut writer = std::io::BufWriter::new(Vec::new());
    crate::format::solution::serialize_solution(&base_solution, &mut writer).unwrap();
    let solution_json = String::from_utf8(writer.into_inner().unwrap()).unwrap();

    let candidate = create_delivery_job_with_index("new_job", 1); // reuse existing location

    let feas_ctx_start = Instant::now();
    let feas_ctx = FeasibilityContext::new(base_problem.clone(), vec![base_matrix.clone()], &solution_json)
        .expect("cannot build feasibility context");
    let feas_ctx_duration = feas_ctx_start.elapsed();

    let feas_check_start = Instant::now();
    let feas_result = feas_ctx.check_job(&candidate).expect("feasibility check failed");
    let feas_check_duration = feas_check_start.elapsed();

    let feasible_count = feas_result.vehicles.iter().filter(|v| v.is_feasible).count();
    let best_cost_delta = feas_result
        .vehicles
        .iter()
        .filter_map(|v| v.cost_delta)
        .min_by(|a, b| a.partial_cmp(b).unwrap());

    eprintln!("Context build:    {:?}", feas_ctx_duration);
    eprintln!("check_job:        {:?}", feas_check_duration);
    eprintln!("Is feasible:      {}", feas_result.is_feasible);
    eprintln!("Feasible vehicles: {}/{}", feasible_count, feas_result.vehicles.len());
    eprintln!("Best cost delta:  {:?}", best_cost_delta);

    // --- Solver with initial solution: add new job and re-optimize ---
    eprintln!();
    eprintln!("=== Step 3: Solver re-optimization (501 jobs, initial solution, 500 gen) ===");

    // Build new problem with the extra job
    let mut new_jobs = base_jobs;
    new_jobs.push(candidate);

    let new_problem = Problem {
        plan: Plan { jobs: new_jobs, ..create_empty_plan() },
        fleet: Fleet { vehicles, ..create_default_fleet() },
        objectives: None,
    };

    let solver_start = Instant::now();

    let environment = Arc::new(Environment {
        parallelism: Parallelism::new_with_cpus(4),
        ..Environment::default()
    });

    // Build core problem with the new job included
    let core_problem: Arc<CoreProblem> = Arc::new(
        (new_problem.clone(), vec![base_matrix.clone()])
            .read_pragmatic()
            .expect("cannot read new problem"),
    );

    // Read the base solution as an initial solution for the new problem.
    // Jobs not in the solution (our new job) go into the "required" pool.
    let init_solution = crate::format::solution::read_init_solution(
        std::io::BufReader::new(solution_json.as_bytes()),
        core_problem.clone(),
        environment.random.clone(),
    )
    .expect("cannot read init solution");

    let init_ctx = InsertionContext::new_from_solution(
        core_problem.clone(),
        (init_solution, None),
        environment.clone(),
    );

    let config = VrpConfigBuilder::new(core_problem.clone())
        .set_environment(environment)
        .prebuild()
        .expect("cannot prebuild")
        .with_init_solutions(vec![init_ctx], None)
        .with_max_generations(Some(500))
        .build()
        .expect("cannot build config");

    let solver_solution = Solver::new(core_problem.clone(), config)
        .solve()
        .expect("solver failed");
    let solver_duration = solver_start.elapsed();

    let solver_cost = solver_solution.cost;
    let solver_unassigned = solver_solution.unassigned.len();
    let solver_routes = solver_solution.routes.len();

    eprintln!("Solver total:     {:?}", solver_duration);
    eprintln!("Solver cost:      {:.2}", solver_cost);
    eprintln!("Solver routes:    {}", solver_routes);
    eprintln!("Solver unassigned: {}", solver_unassigned);

    // --- Summary ---
    eprintln!();
    eprintln!("=== Summary ===");
    eprintln!("                      Feasibility     Solver (500 gen)");
    eprintln!("Time:                 {:>12?}     {:>12?}", feas_check_duration, solver_duration);
    eprintln!("New job placed:       {:>12}     {:>12}", feas_result.is_feasible, solver_unassigned == 0);
    eprintln!("Cost estimate:        {:>12.2}     {:>12.2}", base_cost + best_cost_delta.unwrap_or(0.), solver_cost);
    eprintln!("=================");
    eprintln!();

    // Basic assertions
    assert!(feas_result.is_feasible, "new job should be feasible");
    assert_eq!(solver_unassigned, 0, "solver should assign all jobs");
}
