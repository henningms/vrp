//! Provides solo riding constraint for pickup-delivery jobs.
//!
//! When a job is marked as `solo_riding`, it cannot share the vehicle with other
//! dynamic pickup-delivery jobs while it is onboard.
//!
//! # Semantics
//! - A job marked with `solo_riding = true` can be picked up only when no other dynamic
//!   pickup-delivery job is currently onboard.
//! - While this solo job is onboard, pickups of any other dynamic pickup-delivery jobs are forbidden.
//! - The same solo job can still have multiple pickups/deliveries (e.g. companions) and those activities
//!   are allowed.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/solo_riding_test.rs"]
mod solo_riding_test;

use super::*;
use crate::models::common::{ConfigurableLoad, MultiDimLoad, SingleDimLoad};
use crate::models::problem::Single;
use crate::models::solution::Activity;
use rustc_hash::FxHashMap;

custom_dimension!(pub JobSoloRiding typeof bool);

/// Creates a solo riding feature as a hard constraint.
pub fn create_solo_riding_feature(name: &str, code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default().with_name(name).with_constraint(SoloRidingConstraint { code }).build()
}

struct SoloRidingConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for SoloRidingConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else {
            return None;
        };

        if !self.has_solo_job(route_ctx, activity_ctx.target) {
            return None;
        }

        if self.check_solo_riding(route_ctx, activity_ctx) {
            Some(ConstraintViolation { code: self.code, stopped: false })
        } else {
            None
        }
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, ViolationCode> {
        if self.is_solo_job(&source) || self.is_solo_job(&candidate) { Err(self.code) } else { Ok(source) }
    }
}

impl SoloRidingConstraint {
    fn has_solo_job(&self, route_ctx: &RouteContext, target: &Activity) -> bool {
        route_ctx.route().tour.jobs().any(|job| self.is_solo_job(job))
            || target.retrieve_job().is_some_and(|job| self.is_solo_job(&job))
    }

    fn check_solo_riding(&self, route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> bool {
        let mut onboard: FxHashMap<Job, usize> = FxHashMap::default();
        let mut active_solo_job: Option<Job> = None;
        let tour = &route_ctx.route().tour;

        // `activity_ctx.index` is the leg index, which equals the index of `activity_ctx.prev`.
        // The target is inserted AFTER prev, so prev must be processed BEFORE target.
        for idx in 0..=activity_ctx.index {
            if let Some(activity) = tour.get(idx)
                && self.process_activity(activity, &mut onboard, &mut active_solo_job).is_err()
            {
                return true;
            }
        }

        if self.process_activity(activity_ctx.target, &mut onboard, &mut active_solo_job).is_err() {
            return true;
        }

        for idx in activity_ctx.index + 1..tour.total() {
            if let Some(activity) = tour.get(idx)
                && self.process_activity(activity, &mut onboard, &mut active_solo_job).is_err()
            {
                return true;
            }
        }

        false
    }

    fn process_activity(
        &self,
        activity: &Activity,
        onboard: &mut FxHashMap<Job, usize>,
        active_solo_job: &mut Option<Job>,
    ) -> Result<(), ()> {
        let Some(single) = activity.job.as_ref().map(|single| single.as_ref()) else {
            return Ok(());
        };

        let is_pickup = self.is_dynamic_pickup(single);
        let is_delivery = self.is_dynamic_delivery(single);
        if !is_pickup && !is_delivery {
            return Ok(());
        }

        let Some(job) = activity.retrieve_job() else {
            return Ok(());
        };

        if is_pickup {
            if active_solo_job.as_ref().is_some_and(|solo| solo != &job) {
                return Err(());
            }

            if self.is_solo_job(&job) && onboard.iter().any(|(other, count)| *count > 0 && other != &job) {
                return Err(());
            }

            *onboard.entry(job.clone()).or_insert(0) += 1;

            if self.is_solo_job(&job) {
                *active_solo_job = Some(job.clone());
            }
        }

        if is_delivery {
            if active_solo_job.as_ref().is_some_and(|solo| solo != &job) {
                return Err(());
            }

            if let Some(count) = onboard.get_mut(&job) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    onboard.remove(&job);
                }
            }

            if active_solo_job.as_ref() == Some(&job) && !onboard.contains_key(&job) {
                *active_solo_job = None;
            }
        }

        if let Some(solo_job) = active_solo_job.as_ref()
            && onboard.iter().any(|(job, count)| *count > 0 && job != solo_job)
        {
            return Err(());
        }

        Ok(())
    }

    fn is_solo_job(&self, job: &Job) -> bool {
        job.dimens().get_job_solo_riding().copied().unwrap_or(false)
    }

    fn is_dynamic_pickup(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.pickup.1.value != 0)
            || single
                .dimens
                .get_job_demand::<MultiDimLoad>()
                .is_some_and(|d| has_non_zero_values(&d.pickup.1.load, d.pickup.1.size))
            || single
                .dimens
                .get_job_demand::<ConfigurableLoad>()
                .is_some_and(|d| has_non_zero_values(&d.pickup.1.load, d.pickup.1.size))
    }

    fn is_dynamic_delivery(&self, single: &Single) -> bool {
        single.dimens.get_job_demand::<SingleDimLoad>().is_some_and(|d| d.delivery.1.value != 0)
            || single
                .dimens
                .get_job_demand::<MultiDimLoad>()
                .is_some_and(|d| has_non_zero_values(&d.delivery.1.load, d.delivery.1.size))
            || single
                .dimens
                .get_job_demand::<ConfigurableLoad>()
                .is_some_and(|d| has_non_zero_values(&d.delivery.1.load, d.delivery.1.size))
    }
}

fn has_non_zero_values(load: &[i32], size: usize) -> bool {
    size > 0 && load.iter().take(size).any(|v| *v != 0)
}
