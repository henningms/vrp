//! This example demonstrates the passenger preferences feature.
//!
//! A ride-sharing scenario where passengers have preferences for drivers and vehicle types,
//! but these preferences are soft constraints (can be violated with a cost penalty).
//!
//! Key points:
//! - How to define job preferences (preferred/acceptable/avoid)
//! - How to define vehicle attributes
//! - How preferences influence route assignment
//! - How to tune preference penalties

#[path = "./common/routing.rs"]
mod common;
use crate::common::define_routing_data;

use std::collections::HashSet;
use std::sync::Arc;
use vrp_core::prelude::*;
use vrp_core::construction::features::{
    JobPreferences, JobPreferencesDimension, PreferencePenalty, VehicleAttributesDimension, create_preferences_feature,
};
use vrp_core::models::problem::{JobIdDimension, VehicleIdDimension};

/// Defines a ride-sharing problem with passenger preferences.
///
/// Scenario:
/// - 4 passengers with different preferences
/// - 2 drivers: Alice (SUV) and Bob (Sedan)
/// - Passengers prefer certain drivers/vehicles
fn define_problem(goal: GoalContext, transport: Arc<dyn TransportCost>) -> GenericResult<Problem> {
    // Create 4 passenger jobs with different preferences
    let passengers = vec![
        // Passenger 1: Strongly prefers Alice
        SingleBuilder::default()
            .id("passenger1")
            .demand(Demand::delivery(1))
            .location(1)?
            .dimension(|dimens| {
                dimens.set_job_preferences(JobPreferences::new(
                    Some(vec!["driver:alice".to_string()]), // Preferred: Alice
                    None,                                    // Acceptable: none specified
                    None,                                    // Avoid: none
                ));
            })
            .build_as_job()?,
        // Passenger 2: Prefers Alice or Bob, but avoids night shift
        SingleBuilder::default()
            .id("passenger2")
            .demand(Demand::delivery(1))
            .location(2)?
            .dimension(|dimens| {
                dimens.set_job_preferences(JobPreferences::new(
                    Some(vec!["driver:alice".to_string(), "driver:bob".to_string()]),
                    None,
                    Some(vec!["shift:night".to_string()]),
                ));
            })
            .build_as_job()?,
        // Passenger 3: Prefers SUV, accepts sedan, avoids old vehicles
        SingleBuilder::default()
            .id("passenger3")
            .demand(Demand::delivery(1))
            .location(3)?
            .dimension(|dimens| {
                dimens.set_job_preferences(JobPreferences::new(
                    Some(vec!["vehicle:suv".to_string()]),
                    Some(vec!["vehicle:sedan".to_string()]),
                    Some(vec!["vehicle:old".to_string()]),
                ));
            })
            .build_as_job()?,
        // Passenger 4: No preferences (flexible)
        SingleBuilder::default()
            .id("passenger4")
            .demand(Demand::delivery(1))
            .location(4)?
            .build_as_job()?,
    ];

    // Create 2 drivers with different attributes
    let drivers = vec![
        // Alice with SUV
        VehicleBuilder::default()
            .id("alice")
            .add_detail(
                VehicleDetailBuilder::default()
                    .set_start_location(0)
                    .set_end_location(0)
                    .build()?,
            )
            .dimension(|dimens| {
                dimens.set_vehicle_attributes(
                    vec!["driver:alice".to_string(), "vehicle:suv".to_string(), "shift:day".to_string()]
                        .into_iter()
                        .collect::<HashSet<_>>(),
                );
            })
            .capacity(SingleDimLoad::new(2))
            .build()?,
        // Bob with Sedan
        VehicleBuilder::default()
            .id("bob")
            .add_detail(
                VehicleDetailBuilder::default()
                    .set_start_location(0)
                    .set_end_location(0)
                    .build()?,
            )
            .dimension(|dimens| {
                dimens.set_vehicle_attributes(
                    vec!["driver:bob".to_string(), "vehicle:sedan".to_string(), "shift:day".to_string()]
                        .into_iter()
                        .collect::<HashSet<_>>(),
                );
            })
            .capacity(SingleDimLoad::new(2))
            .build()?,
    ];

    ProblemBuilder::default()
        .add_jobs(passengers.into_iter())
        .add_vehicles(drivers.into_iter())
        .with_goal(goal)
        .with_transport_cost(transport)
        .build()
}

/// Defines the optimization goal with preferences as soft constraint.
fn define_goal(transport: Arc<dyn TransportCost>) -> GenericResult<GoalContext> {
    // Configure features
    let minimize_unassigned = MinimizeUnassignedBuilder::new("min-unassigned").build()?;
    let capacity_feature = CapacityFeatureBuilder::<SingleDimLoad>::new("capacity").build()?;
    let transport_feature = TransportFeatureBuilder::new("min-distance")
        .set_transport_cost(transport)
        .set_time_constrained(false)
        .build_minimize_distance()?;

    // Add preference feature with default penalties
    let preferences_feature = create_preferences_feature("preferences", PreferencePenalty::default())?;

    // Configure goal: minimize unassigned, then preference violations, then distance
    GoalContextBuilder::with_features(&[minimize_unassigned, preferences_feature, transport_feature, capacity_feature])?
        .build()
}

fn main() -> GenericResult<()> {
    println!("=== Passenger Preferences Example ===\n");

    // Setup
    let transport = Arc::new(define_routing_data()?);
    let goal = define_goal(transport.clone())?;
    let problem = Arc::new(define_problem(goal, transport)?);

    println!("Problem setup:");
    println!("  - 4 passengers with different preferences");
    println!("  - 2 drivers: Alice (SUV), Bob (Sedan)");
    println!("\nPassenger preferences:");
    println!("  - Passenger 1: Prefers Alice");
    println!("  - Passenger 2: Prefers Alice or Bob, avoids night shift");
    println!("  - Passenger 3: Prefers SUV, accepts Sedan, avoids old vehicles");
    println!("  - Passenger 4: No preferences (flexible)\n");

    // Solve
    let config =
        VrpConfigBuilder::new(problem.clone()).prebuild()?.with_max_time(Some(5)).with_max_generations(Some(10)).build()?;

    println!("Solving...\n");
    let solution = Solver::new(problem.clone(), config).solve()?;

    // Analyze solution
    println!("=== Solution ===\n");
    println!("Total cost: {:.2}", solution.cost);
    println!("Routes: {}\n", solution.routes.len());

    for route in &solution.routes {
        let driver = route.actor.vehicle.dimens.get_vehicle_id().unwrap();
        let attrs = route.actor.vehicle.dimens.get_vehicle_attributes().unwrap();

        println!("Driver {} (attributes: {:?}):", driver, attrs);

        for job in route.tour.jobs() {
            let job_id = job.dimens().get_job_id().unwrap();
            let preferences = job.dimens().get_job_preferences();

            match preferences {
                Some(prefs) => {
                    let has_preferred = prefs.has_preferred_match(Some(attrs));
                    let has_acceptable = prefs.has_acceptable_match(Some(attrs));
                    let avoided_count = prefs.count_avoided(Some(attrs));

                    let status = if has_preferred {
                        "✓ PREFERRED MATCH"
                    } else if has_acceptable {
                        "~ ACCEPTABLE MATCH"
                    } else if prefs.preferred.is_some() {
                        "✗ NO PREFERRED MATCH"
                    } else {
                        "  (no preference)"
                    };

                    let avoid_msg = if avoided_count > 0 {
                        format!(" [WARNING: {} avoided attributes present]", avoided_count)
                    } else {
                        String::new()
                    };

                    println!("  {} {} {}{}", if has_preferred { "✓" } else { " " }, job_id, status, avoid_msg);
                }
                None => {
                    println!("    {} (no preferences)", job_id);
                }
            }
        }
        println!();
    }

    // Summary
    let total_passengers = solution.routes.iter().map(|r| r.tour.job_count()).sum::<usize>();
    println!("Summary:");
    println!("  - Total passengers served: {}", total_passengers);
    println!("  - All jobs assigned: {}", solution.unassigned.is_empty());

    Ok(())
}
