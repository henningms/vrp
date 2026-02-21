//! Provides insertion feasibility checking against an existing solution.
//!
//! This module evaluates whether a candidate job can be feasibly inserted into the
//! current schedule without running the full solver, returning per-vehicle results
//! with cost deltas and constraint violation details.

#[cfg(test)]
#[path = "../../tests/unit/format/feasibility_test.rs"]
mod feasibility_test;

use crate::format::problem::job_reader::convert_api_job_to_core;
use crate::format::problem::{
    deserialize_matrix, deserialize_problem, get_problem_properties, map_to_problem_with_props, ApiProblem, Matrix,
    ProblemProperties,
};
use crate::format::solution::map_code_reason;
use crate::format::solution::read_init_solution;
use crate::format::{CoordIndexExtraProperty, CoreProblem, ShiftIndexDimension, VehicleTypeDimension};
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::sync::Arc;
use vrp_core::construction::heuristics::{
    apply_insertion_success, eval_job_insertion_in_route, BestResultSelector, EvaluationContext, InsertionPosition,
    InsertionResult, LegSelection, ResultSelector, UnassignmentInfo,
};
use vrp_core::models::problem::VehicleIdDimension;
use vrp_core::prelude::*;

type ApiJob = crate::format::problem::Job;

/// Information about a constraint violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintViolationInfo {
    /// A human-readable constraint code (e.g. "CAPACITY_CONSTRAINT").
    pub code: String,
    /// A human-readable description of the violation.
    pub description: String,
}

/// Per-vehicle feasibility result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VehicleFeasibility {
    /// The vehicle instance id (e.g. "my_vehicle_1").
    pub vehicle_id: String,
    /// The vehicle type id (e.g. "my_vehicle").
    pub type_id: String,
    /// The shift index of the vehicle.
    pub shift_index: usize,
    /// Whether the candidate job can be inserted into this vehicle's route.
    pub is_feasible: bool,
    /// The insertion cost delta if feasible.
    pub cost_delta: Option<Float>,
    /// Constraint violations if infeasible.
    pub violations: Vec<ConstraintViolationInfo>,
}

/// Top-level feasibility check result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeasibilityResult {
    /// Whether the candidate job can be inserted into at least one vehicle.
    pub is_feasible: bool,
    /// Per-vehicle feasibility details.
    pub vehicles: Vec<VehicleFeasibility>,
}

/// Pre-built context for performing repeated feasibility checks.
///
/// Reconstructs route state from a problem + solution pair once, then allows
/// checking multiple candidate jobs against the same solution state.
pub struct FeasibilityContext {
    problem: Arc<CoreProblem>,
    insertion_ctx: InsertionContext,
    api_problem: ApiProblem,
    properties: ProblemProperties,
}

impl FeasibilityContext {
    /// Creates a new feasibility context from API-level problem, matrices, and solution JSON.
    ///
    /// All constraint features (skills, compatibility, order, etc.) are force-enabled
    /// so that a candidate job introducing a new constraint type is still checked correctly.
    pub fn new(
        api_problem: ApiProblem,
        matrices: Vec<Matrix>,
        solution_json: &str,
    ) -> Result<Self, GenericError> {
        let properties = get_problem_properties(&api_problem, &matrices)
            .with_all_constraints_enabled();

        let coord_index = crate::format::CoordIndex::new(&api_problem);
        let core_problem: CoreProblem =
            map_to_problem_with_props(api_problem.clone(), matrices, coord_index, Some(properties.clone()))
                .map_err(|e: crate::format::MultiFormatError| e.to_string())?;
        let core_problem = Arc::new(core_problem);

        let random: Arc<dyn Random> = Arc::new(DefaultRandom::default());
        let environment = Arc::new(Environment::default());

        let solution = read_init_solution(
            BufReader::new(solution_json.as_bytes()),
            core_problem.clone(),
            random,
        )?;

        let insertion_ctx =
            InsertionContext::new_from_solution(core_problem.clone(), (solution, None), environment);

        Ok(Self { problem: core_problem, insertion_ctx, api_problem, properties })
    }

    /// Checks whether the given candidate API job can be feasibly inserted.
    pub fn check_job(&self, candidate: &ApiJob) -> Result<FeasibilityResult, GenericError> {
        let coord_index = self
            .problem
            .extras
            .get_coord_index()
            .ok_or_else(|| GenericError::from("cannot get coord index"))?;

        let core_job =
            convert_api_job_to_core(candidate, &self.api_problem, &self.properties, &coord_index);

        let result_selector = BestResultSelector::default();
        let goal = &self.problem.goal;

        let mut vehicles = Vec::new();

        for route_ctx in &self.insertion_ctx.solution.routes {
            let actor = &route_ctx.route().actor;
            let vehicle_id = actor
                .vehicle
                .dimens
                .get_vehicle_id()
                .cloned()
                .unwrap_or_default();
            let type_id = actor
                .vehicle
                .dimens
                .get_vehicle_type()
                .cloned()
                .unwrap_or_default();
            let shift_index = actor
                .vehicle
                .dimens
                .get_shift_index()
                .copied()
                .unwrap_or_default();

            let eval_ctx = EvaluationContext {
                goal,
                job: &core_job,
                leg_selection: &LegSelection::Exhaustive,
                result_selector: &result_selector,
            };

            let result = eval_job_insertion_in_route(
                &self.insertion_ctx,
                &eval_ctx,
                route_ctx,
                InsertionPosition::Any,
                InsertionResult::make_failure(),
            );

            let vehicle_result = match result {
                InsertionResult::Success(success) => {
                    let cost_delta: Float = success.cost.iter().sum();
                    VehicleFeasibility {
                        vehicle_id,
                        type_id,
                        shift_index,
                        is_feasible: true,
                        cost_delta: Some(cost_delta),
                        violations: vec![],
                    }
                }
                InsertionResult::Failure(failure) => {
                    let (code, description) = map_code_reason(failure.constraint);
                    VehicleFeasibility {
                        vehicle_id,
                        type_id,
                        shift_index,
                        is_feasible: false,
                        cost_delta: None,
                        violations: vec![ConstraintViolationInfo {
                            code: code.to_string(),
                            description: description.to_string(),
                        }],
                    }
                }
            };

            vehicles.push(vehicle_result);
        }

        let is_feasible = vehicles.iter().any(|v| v.is_feasible);

        Ok(FeasibilityResult { is_feasible, vehicles })
    }

    /// Finds the best feasible insertion for the candidate job and applies it to the internal state.
    ///
    /// After this call the insertion context is updated as if the job had been assigned,
    /// so subsequent `check_job` / `accept_job` calls see the new state.
    pub fn accept_job(&mut self, candidate: &ApiJob) -> Result<VehicleFeasibility, GenericError> {
        let coord_index = self
            .problem
            .extras
            .get_coord_index()
            .ok_or_else(|| GenericError::from("cannot get coord index"))?;

        let core_job =
            convert_api_job_to_core(candidate, &self.api_problem, &self.properties, &coord_index);

        let result_selector = BestResultSelector::default();
        let goal = &self.problem.goal;

        let mut best = InsertionResult::make_failure();

        for route_ctx in &self.insertion_ctx.solution.routes {
            let eval_ctx = EvaluationContext {
                goal,
                job: &core_job,
                leg_selection: &LegSelection::Exhaustive,
                result_selector: &result_selector,
            };

            best = result_selector.select_insertion(
                &self.insertion_ctx,
                best,
                eval_job_insertion_in_route(
                    &self.insertion_ctx,
                    &eval_ctx,
                    route_ctx,
                    InsertionPosition::Any,
                    InsertionResult::make_failure(),
                ),
            );
        }

        match best {
            InsertionResult::Success(success) => {
                let actor = &success.actor;
                let vehicle_id = actor
                    .vehicle
                    .dimens
                    .get_vehicle_id()
                    .cloned()
                    .unwrap_or_default();
                let type_id = actor
                    .vehicle
                    .dimens
                    .get_vehicle_type()
                    .cloned()
                    .unwrap_or_default();
                let shift_index = actor
                    .vehicle
                    .dimens
                    .get_shift_index()
                    .copied()
                    .unwrap_or_default();
                let cost_delta: Float = success.cost.iter().sum();

                apply_insertion_success(&mut self.insertion_ctx, success);
                self.problem.goal.accept_solution_state(&mut self.insertion_ctx.solution);

                Ok(VehicleFeasibility {
                    vehicle_id,
                    type_id,
                    shift_index,
                    is_feasible: true,
                    cost_delta: Some(cost_delta),
                    violations: vec![],
                })
            }
            InsertionResult::Failure(_) => {
                Err("no feasible insertion found for candidate job".into())
            }
        }
    }

    /// Serializes the current solution state to a JSON string.
    ///
    /// Builds a domain `Solution` from the internal `InsertionContext` and converts it
    /// to the pragmatic API format. Useful for persisting state or rebuilding the context later.
    pub fn to_solution_json(&self) -> Result<String, GenericError> {
        use crate::format::solution::{create_solution, PragmaticOutputType, serialize_solution};
        use std::io::BufWriter;

        let cost = self.insertion_ctx.get_total_cost().unwrap_or(0.);

        let routes: Vec<vrp_core::models::solution::Route> = self
            .insertion_ctx
            .solution
            .routes
            .iter()
            .map(|rc| rc.route().deep_copy())
            .collect();

        let registry = self.insertion_ctx.solution.registry.resources().deep_copy();

        let unassigned: Vec<(vrp_core::prelude::Job, UnassignmentInfo)> = self
            .insertion_ctx
            .solution
            .unassigned
            .iter()
            .chain(
                self.insertion_ctx
                    .solution
                    .required
                    .iter()
                    .map(|job| (job, &UnassignmentInfo::Unknown)),
            )
            .map(|(job, info)| (job.clone(), info.clone()))
            .collect();

        let domain_solution = vrp_core::models::Solution {
            cost,
            registry,
            routes,
            unassigned,
            telemetry: None,
        };

        let api_solution =
            create_solution(&self.problem, &domain_solution, &PragmaticOutputType::OnlyPragmatic);

        let mut writer = BufWriter::new(Vec::new());
        serialize_solution(&api_solution, &mut writer)
            .map_err(|e| GenericError::from(e.to_string()))?;

        String::from_utf8(writer.into_inner().map_err(|e| GenericError::from(e.to_string()))?)
            .map_err(|e| GenericError::from(e.to_string()))
    }
}

/// Convenience function: check insertion feasibility from JSON strings.
///
/// Parses problem, matrices, and solution from JSON, creates a `FeasibilityContext`,
/// evaluates the candidate job, and returns the result as a JSON string.
pub fn check_insertion_feasibility(
    problem_json: &str,
    matrices_json: Vec<&str>,
    solution_json: &str,
    candidate_job_json: &str,
) -> Result<String, GenericError> {
    let api_problem: crate::format::problem::Problem =
        deserialize_problem(BufReader::new(problem_json.as_bytes()))
            .map_err(|e: crate::format::MultiFormatError| e.to_string())?;

    let matrices: Vec<Matrix> = matrices_json
        .into_iter()
        .map(|m| {
            deserialize_matrix(BufReader::new(m.as_bytes()))
                .map_err(|e: crate::format::MultiFormatError| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let candidate: ApiJob =
        serde_json::from_str(candidate_job_json).map_err(|e: serde_json::Error| e.to_string())?;

    let ctx = FeasibilityContext::new(api_problem, matrices, solution_json)?;
    let result = ctx.check_job(&candidate)?;

    serde_json::to_string_pretty(&result).map_err(|e: serde_json::Error| e.to_string().into())
}
