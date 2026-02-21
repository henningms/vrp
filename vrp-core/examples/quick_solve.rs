//! This example demonstrates how to configure the solver for very fast execution on small problems.
//!
//! Key points:
//! - Use `with_max_time` and `with_max_generations` to limit solver runtime
//! - For small problems (< 50 jobs), even 10-20 generations produce good results
//! - The solver finds a valid initial solution quickly, then improves it
//!
//! Run with: `cargo run --example quick_solve`

#[path = "./common/routing.rs"]
mod common;
use crate::common::define_routing_data;

use std::sync::Arc;
use std::time::Instant;
use vrp_core::prelude::*;

fn define_problem(goal: GoalContext, transport: Arc<dyn TransportCost>) -> GenericResult<Problem> {
    // Create 6 delivery jobs - a typical small problem
    let jobs = (1..=6)
        .map(|idx| {
            SingleBuilder::default()
                .id(format!("job{idx}").as_str())
                .demand(Demand::delivery(1))
                .location(idx % 5)? // Use locations 1-4 cyclically
                .build_as_job()
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Create 2 vehicles with capacity for 4 jobs each
    let vehicles = (1..=2)
        .map(|idx| {
            VehicleBuilder::default()
                .id(format!("v{idx}").as_str())
                .add_detail(
                    VehicleDetailBuilder::default()
                        .set_start_location(0)
                        .set_end_location(0)
                        .build()?,
                )
                .capacity(SingleDimLoad::new(4))
                .build()
        })
        .collect::<Result<Vec<_>, _>>()?;

    ProblemBuilder::default()
        .add_jobs(jobs.into_iter())
        .add_vehicles(vehicles.into_iter())
        .with_goal(goal)
        .with_transport_cost(transport)
        .build()
}

fn define_goal(transport: Arc<dyn TransportCost>) -> GenericResult<GoalContext> {
    let minimize_unassigned = MinimizeUnassignedBuilder::new("min-unassigned").build()?;
    let capacity_feature = CapacityFeatureBuilder::<SingleDimLoad>::new("capacity").build()?;
    let transport_feature = TransportFeatureBuilder::new("min-distance")
        .set_transport_cost(transport)
        .set_time_constrained(false)
        .build_minimize_distance()?;

    GoalContextBuilder::with_features(&[minimize_unassigned, transport_feature, capacity_feature])?.build()
}

fn main() -> GenericResult<()> {
    let transport = Arc::new(define_routing_data()?);
    let goal = define_goal(transport.clone())?;
    let problem = Arc::new(define_problem(goal, transport)?);

    // ============================================================
    // QUICK SOLVE CONFIGURATION
    // ============================================================
    // For small problems, we can get good results very quickly by:
    // - Limiting max_time to 1 second (or less)
    // - Limiting max_generations to 10-50
    // The solver will stop when EITHER limit is reached.
    // ============================================================

    let start = Instant::now();

    let config = VrpConfigBuilder::new(problem.clone())
        .prebuild()?
        .with_max_time(Some(1)) // Stop after 1 second
        .with_max_generations(Some(20)) // Or after 20 generations
        .build()?;

    let solution = Solver::new(problem, config).solve()?;

    let elapsed = start.elapsed();

    // Print results
    println!("=== Quick Solve Results ===");
    println!("Solved in: {:?}", elapsed);
    println!("Total cost: {:.2}", solution.cost);
    println!("Routes: {}", solution.routes.len());
    println!("Unassigned jobs: {}", solution.unassigned.len());
    println!(
        "\nRoute details:\n{:?}",
        solution.get_locations().map(Iterator::collect::<Vec<_>>).collect::<Vec<_>>()
    );

    // Verify we got a valid solution quickly
    assert!(elapsed.as_millis() < 1000, "Should complete in under 1 second");
    assert!(solution.unassigned.is_empty(), "All jobs should be assigned");

    println!("\nSuccess! Got a valid solution in under 1 second.");

    Ok(())
}
