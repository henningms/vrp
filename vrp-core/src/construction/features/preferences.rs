//! A job-vehicle preferences feature (soft constraint).
//!
//! This feature allows jobs to express preferences for vehicle attributes without making them
//! hard requirements. Unlike skills (which reject assignments), preferences add cost penalties
//! to guide the solver toward better matches.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/preferences_test.rs"]
mod preferences_test;

use super::*;
use std::collections::HashSet;

custom_dimension!(pub JobPreferences typeof JobPreferences);
custom_dimension!(pub VehicleAttributes typeof HashSet<String>);
custom_solution_state!(PreferencesFitness typeof Cost);

/// Job preferences for vehicle attributes (soft constraint).
///
/// Preferences express desired vehicle attributes without making them mandatory.
/// The solver will try to match preferences but can violate them if necessary.
///
/// # Semantics
/// - **preferred**: List of preferred attributes. Penalty if NONE are present.
/// - **acceptable**: Fallback attributes. Additional penalty if no preferred AND no acceptable.
/// - **avoid**: Attributes to avoid. Penalty for EACH attribute present.
///
/// # Example
/// ```
/// use vrp_core::construction::features::JobPreferences;
///
/// let prefs = JobPreferences::new(
///     Some(vec!["driver:alice".to_string(), "driver:bob".to_string()]),
///     Some(vec!["driver:charlie".to_string()]),
///     Some(vec!["shift:night".to_string()]),
/// );
/// // Job prefers Alice or Bob, accepts Charlie, wants to avoid night shift
/// ```
pub struct JobPreferences {
    /// List of preferred attributes. Penalty applied if NONE are present.
    pub preferred: Option<HashSet<String>>,

    /// List of acceptable attributes. Smaller penalty if none present and no preferred match.
    pub acceptable: Option<HashSet<String>>,

    /// List of attributes to avoid. Penalty applied for EACH attribute present.
    pub avoid: Option<HashSet<String>>,
}

impl JobPreferences {
    /// Creates a new instance of [`JobPreferences`].
    pub fn new(
        preferred: Option<Vec<String>>,
        acceptable: Option<Vec<String>>,
        avoid: Option<Vec<String>>,
    ) -> Self {
        let map: fn(Option<Vec<_>>) -> Option<HashSet<_>> =
            |attrs| attrs.and_then(|v| if v.is_empty() { None } else { Some(v.into_iter().collect()) });

        Self { preferred: map(preferred), acceptable: map(acceptable), avoid: map(avoid) }
    }

    /// Check if any preferred attribute matches the vehicle attributes.
    pub fn has_preferred_match(&self, vehicle_attrs: Option<&HashSet<String>>) -> bool {
        match (&self.preferred, vehicle_attrs) {
            (Some(preferred), Some(attrs)) => preferred.iter().any(|attr| attrs.contains(attr)),
            _ => false,
        }
    }

    /// Check if any acceptable attribute matches the vehicle attributes.
    pub fn has_acceptable_match(&self, vehicle_attrs: Option<&HashSet<String>>) -> bool {
        match (&self.acceptable, vehicle_attrs) {
            (Some(acceptable), Some(attrs)) => acceptable.iter().any(|attr| attrs.contains(attr)),
            _ => false,
        }
    }

    /// Count how many avoided attributes are present in the vehicle attributes.
    pub fn count_avoided(&self, vehicle_attrs: Option<&HashSet<String>>) -> usize {
        match (&self.avoid, vehicle_attrs) {
            (Some(avoid), Some(attrs)) => avoid.iter().filter(|attr| attrs.contains(*attr)).count(),
            _ => 0,
        }
    }
}

/// Configurable penalty structure for preferences.
///
/// These penalties control how strongly preferences influence routing decisions.
/// Higher penalties make the solver work harder to satisfy preferences.
///
/// # Tuning Guidelines
/// Compare penalties to your distance/time costs. If distance costs ~1.0 per km:
/// - Penalty of 10.0 = willing to drive ~10 km extra to honor preference
/// - Penalty of 100.0 = willing to drive ~100 km extra to honor preference
#[derive(Clone, Debug)]
pub struct PreferencePenalty {
    /// Penalty if none of the preferred attributes match.
    pub no_preferred_match: Cost,

    /// Penalty if no preferred AND no acceptable attributes match.
    pub no_acceptable_match: Cost,

    /// Penalty per avoided attribute that is present.
    pub per_avoided_present: Cost,
}

impl Default for PreferencePenalty {
    fn default() -> Self {
        Self {
            no_preferred_match: 100.0,   // High penalty for missing preferred
            no_acceptable_match: 30.0,   // Lower additional penalty for missing acceptable
            per_avoided_present: 75.0,   // High penalty per unwanted attribute
        }
    }
}

/// Creates a preferences feature as soft constraint (objective).
///
/// # Arguments
/// - `name`: Unique name for the feature
/// - `penalty`: Penalty configuration (use Default for reasonable defaults)
///
/// # Example
/// ```
/// use vrp_core::construction::features::{create_preferences_feature, PreferencePenalty};
///
/// let feature = create_preferences_feature(
///     "preferences",
///     PreferencePenalty::default(),
/// ).unwrap();
/// ```
pub fn create_preferences_feature(name: &str, penalty: PreferencePenalty) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name(name)
        .with_objective(PreferencesObjective { penalty: penalty.clone() })
        .with_state(PreferencesState { penalty })
        .build()
}

struct PreferencesObjective {
    penalty: PreferencePenalty,
}

impl FeatureObjective for PreferencesObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        // Get cached solution-level fitness if available
        solution
            .solution
            .state
            .get_preferences_fitness()
            .copied()
            .unwrap_or_else(|| calculate_solution_fitness(&self.penalty, &solution.solution))
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => calculate_job_penalty(&self.penalty, job, route_ctx),
            MoveContext::Activity { .. } => 0.0,
        }
    }
}

struct PreferencesState {
    penalty: PreferencePenalty,
}

impl FeatureState for PreferencesState {
    fn accept_insertion(&self, _solution_ctx: &mut SolutionContext, _route_index: usize, _job: &Job) {
        // Performance note: We don't cache route-level penalties here.
        // This is a deliberate trade-off:
        // - Simpler state management (no cache invalidation needed)
        // - Lower memory usage
        // - Preference calculation is lightweight (HashSet lookups)
        // - Solution-level cache (in accept_solution_state) handles most cases
        //
        // If profiling shows this is a bottleneck for large problems (1000+ jobs),
        // consider adding route-level caching similar to the transport feature.
    }

    fn accept_route_state(&self, _route_ctx: &mut RouteContext) {
        // See comment in accept_insertion for caching design rationale
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        let total_penalty = calculate_solution_fitness(&self.penalty, solution_ctx);
        solution_ctx.state.set_preferences_fitness(total_penalty);
    }
}

/// Calculate penalty for assigning a job to a route.
fn calculate_job_penalty(penalty_config: &PreferencePenalty, job: &Job, route_ctx: &RouteContext) -> Cost {
    let preferences = match job.dimens().get_job_preferences() {
        Some(prefs) => prefs,
        None => return 0.0, // No preferences = no penalty
    };

    let vehicle_attrs = route_ctx.route().actor.vehicle.dimens.get_vehicle_attributes();
    let mut total_penalty = 0.0;

    // Check preferred attributes
    let has_preferred = preferences.has_preferred_match(vehicle_attrs);
    let has_acceptable = preferences.has_acceptable_match(vehicle_attrs);

    if preferences.preferred.is_some() && !has_preferred {
        // None of the preferred attributes match
        total_penalty += penalty_config.no_preferred_match;

        // If also no acceptable match, add additional penalty
        if preferences.acceptable.is_some() && !has_acceptable {
            total_penalty += penalty_config.no_acceptable_match;
        }
    }

    // Check avoided attributes (penalize each one present)
    let avoided_count = preferences.count_avoided(vehicle_attrs);
    total_penalty += (avoided_count as Cost) * penalty_config.per_avoided_present;

    total_penalty
}

/// Calculate total penalty for all jobs in a route.
fn calculate_route_penalty(penalty_config: &PreferencePenalty, route_ctx: &RouteContext) -> Cost {
    route_ctx.route().tour.jobs().map(|job| calculate_job_penalty(penalty_config, job, route_ctx)).sum()
}

/// Calculate total penalty across entire solution.
fn calculate_solution_fitness(penalty_config: &PreferencePenalty, solution_ctx: &SolutionContext) -> Cost {
    solution_ctx.routes.iter().map(|route_ctx| calculate_route_penalty(penalty_config, route_ctx)).sum()
}
