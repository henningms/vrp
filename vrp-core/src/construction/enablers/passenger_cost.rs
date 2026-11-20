//! Provides the way to charge a vehicle's per-passenger cost.
//!
//! A vehicle with a per-passenger cost pays it for every passenger of every job its tour serves, once per
//! job: a multi job counts once, whatever the number of its activities. A job carries the passengers set
//! by its `JobPassengers` dimension, and a job without it, e.g. a break or a reload, carries none.

#[cfg(test)]
#[path = "../../../tests/unit/construction/enablers/passenger_cost_test.rs"]
mod passenger_cost_test;

use crate::construction::heuristics::RouteContext;
use crate::models::common::Cost;
use crate::models::problem::{Job, JobPassengersDimension, Vehicle, VehiclePerPassengerCostDimension};
use crate::models::solution::Tour;

/// Counts the passengers of the jobs in the tour.
pub fn count_tour_passengers(tour: &Tour) -> usize {
    tour.jobs().filter_map(|job| job.dimens().get_job_passengers()).sum()
}

/// Returns the route's passenger fees: its vehicle's per-passenger cost for every passenger of the
/// tour's jobs, or `None` when the vehicle has no per-passenger cost.
pub fn get_route_passenger_cost(route_ctx: &RouteContext) -> Option<Cost> {
    let per_passenger = *route_ctx.route().actor.vehicle.dimens.get_vehicle_per_passenger_cost()?;

    Some(per_passenger * count_tour_passengers(&route_ctx.route().tour) as Cost)
}

/// Returns the passenger fees the vehicle charges for the job, or `None` when the vehicle has no
/// per-passenger cost or the job carries no passengers.
pub fn get_job_passenger_cost(vehicle: &Vehicle, job: &Job) -> Option<Cost> {
    let per_passenger = *vehicle.dimens.get_vehicle_per_passenger_cost()?;
    let passengers = *job.dimens().get_job_passengers()?;

    Some(per_passenger * passengers as Cost)
}
