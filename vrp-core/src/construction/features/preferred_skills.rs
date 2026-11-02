//! A job-vehicle preferred skills feature: a soft counterpart of the skills feature.
//!
//! A job lists groups of preferred skills. A group is met when the vehicle serving the job has at least
//! one skill of the group, and every unmet group of an assigned job costs one unit. Unassigned jobs cost
//! nothing, so the objective must rank below the one which minimizes unassigned jobs.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/preferred_skills_test.rs"]
mod preferred_skills_test;

use super::*;
use std::collections::HashSet;

custom_dimension!(pub JobPreferredSkills typeof JobPreferredSkills);

/// Skills a job prefers its vehicle to have.
pub struct JobPreferredSkills {
    /// Vehicle should have at least one skill of every group.
    pub one_of: Vec<HashSet<String>>,
}

impl JobPreferredSkills {
    /// Creates a new instance of [`JobPreferredSkills`]. Empty groups are dropped: no vehicle could meet them.
    pub fn new(one_of: Vec<Vec<String>>) -> Self {
        Self {
            one_of: one_of
                .into_iter()
                .filter(|group| !group.is_empty())
                .map(|group| group.into_iter().collect())
                .collect(),
        }
    }

    /// Returns how many groups a vehicle with the given skills does not meet.
    pub fn count_unmet(&self, vehicle_skills: Option<&HashSet<String>>) -> usize {
        self.one_of
            .iter()
            .filter(|group| !vehicle_skills.is_some_and(|skills| group.iter().any(|skill| skills.contains(skill))))
            .count()
    }
}

/// Creates a feature which minimizes the number of unmet preferred skill groups of assigned jobs.
pub fn create_preferred_skills_feature(name: &str) -> GenericResult<Feature> {
    FeatureBuilder::default().with_name(name).with_objective(PreferredSkillsObjective).build()
}

struct PreferredSkillsObjective;

impl FeatureObjective for PreferredSkillsObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        solution
            .solution
            .routes
            .iter()
            .map(|route_ctx| route_ctx.route().tour.jobs().map(|job| get_unmet(route_ctx, job)).sum::<Cost>())
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => get_unmet(route_ctx, job),
            MoveContext::Activity { .. } => Cost::default(),
        }
    }
}

fn get_unmet(route_ctx: &RouteContext, job: &Job) -> Cost {
    job.dimens().get_job_preferred_skills().map_or(Cost::default(), |preferred_skills| {
        preferred_skills.count_unmet(route_ctx.route().actor.vehicle.dimens.get_vehicle_skills()) as Cost
    })
}
