//! A job-vehicle skills feature.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/skills_test.rs"]
mod skills_test;

use super::*;
use std::collections::HashSet;

custom_dimension!(pub JobSkills typeof JobSkills);
custom_dimension!(pub VehicleSkills typeof HashSet<String>);

/// A job skills limitation for a vehicle.
pub struct JobSkills {
    /// Vehicle should have all of these skills defined.
    pub all_of: Option<HashSet<String>>,
    /// Vehicle should have at least one skill of every group.
    pub one_of: Option<Vec<HashSet<String>>>,
    /// Vehicle should have none of these skills defined.
    pub none_of: Option<HashSet<String>>,
}

impl JobSkills {
    /// Creates a new instance of [`JobSkills`]. An empty `one_of` group is kept, so it can never be satisfied.
    pub fn new(all_of: Option<Vec<String>>, one_of: Option<Vec<Vec<String>>>, none_of: Option<Vec<String>>) -> Self {
        let map: fn(Option<Vec<_>>) -> Option<HashSet<_>> =
            |skills| skills.and_then(|v| if v.is_empty() { None } else { Some(v.into_iter().collect()) });
        let one_of = one_of
            .filter(|groups| !groups.is_empty())
            .map(|groups| groups.into_iter().map(|group| group.into_iter().collect()).collect());

        Self { all_of: map(all_of), one_of, none_of: map(none_of) }
    }

    /// Returns true when a vehicle with the given skills can serve the job.
    pub fn is_satisfied_by(&self, vehicle_skills: Option<&HashSet<String>>) -> bool {
        let has = |skill: &String| vehicle_skills.is_some_and(|skills| skills.contains(skill));

        self.all_of.iter().flatten().all(has)
            && self.one_of.iter().flatten().all(|group| group.iter().any(has))
            && !self.none_of.iter().flatten().any(has)
    }

    fn implies(&self, other: &JobSkills) -> bool {
        let is_subset = |own: Option<&HashSet<String>>, other: Option<&HashSet<String>>| match (own, other) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(own), Some(other)) => other.is_subset(own),
        };
        let implies_group = |group: &HashSet<String>| {
            self.all_of.as_ref().is_some_and(|all_of| !all_of.is_disjoint(group))
                || self.one_of.iter().flatten().any(|own_group| own_group.is_subset(group))
        };

        is_subset(self.all_of.as_ref(), other.all_of.as_ref())
            && is_subset(self.none_of.as_ref(), other.none_of.as_ref())
            && other.one_of.iter().flatten().all(implies_group)
    }
}

/// Creates a skills feature as hard constraint.
pub fn create_skills_feature(name: &str, code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default().with_name(name).with_constraint(SkillsConstraint { code }).build()
}

struct SkillsConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for SkillsConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => {
                if let Some(job_skills) = job.dimens().get_job_skills() {
                    let vehicle_skills = route_ctx.route().actor.vehicle.dimens.get_vehicle_skills();
                    if !job_skills.is_satisfied_by(vehicle_skills) {
                        return ConstraintViolation::fail(self.code);
                    }
                }

                None
            }
            MoveContext::Activity { .. } => None,
        }
    }

    // The merged job keeps the source's skills, so every vehicle that serves the source must serve the candidate.
    fn merge(&self, source: Job, candidate: Job) -> Result<Job, ViolationCode> {
        let is_implied = match (source.dimens().get_job_skills(), candidate.dimens().get_job_skills()) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(source_skills), Some(candidate_skills)) => source_skills.implies(candidate_skills),
        };

        if is_implied { Ok(source) } else { Err(self.code) }
    }
}
