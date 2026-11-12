//! Provides a constraint which keeps jobs of some classes apart.
//!
//! For example, an operator runs several passenger services on one vehicle: riders of some services
//! must not be on board together, and some services must not use the same vehicle tour at all.
//!
//! # Semantics
//! - A job has a class and lists the classes it must not share a run or a tour with.
//! - Two jobs conflict when either one lists the class of the other, so one side stating it is enough.
//!   A class may be listed by its own job: then no two jobs of that class may share a run or a tour.
//! - A *run* is a maximal stretch of a tour while at least one pickup-delivery job is on board, see
//!   [get_tour_runs](crate::construction::enablers::get_tour_runs). The run level applies to
//!   pickup-delivery (multi) jobs only; the tour level applies to every job.
//! - A job without a class can still join two runs into one, so it is refused when the runs it joins
//!   hold conflicting classes.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/class_separation_test.rs"]
mod class_separation_test;

use super::*;
use crate::construction::enablers::{TourRun, collect_multi_job_spans, get_tour_runs, is_complete};
use crate::models::solution::Tour;
use rustc_hash::FxHashMap;

custom_dimension!(pub JobClassSeparation typeof ClassSeparation);
custom_tour_state!(TourClasses typeof TourClasses);

/// A job's class with the classes it must not share a run or a tour with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassSeparation {
    /// The job's class.
    pub class: String,
    /// Classes of pickup-delivery jobs which must not be on board together with this job.
    pub not_in_same_run: Vec<String>,
    /// Classes of jobs which must not be served in the same tour as this job.
    pub not_in_same_tour: Vec<String>,
}

/// Creates a class separation feature as a hard constraint.
pub fn create_class_separation_feature(name: &str, code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name(name)
        .with_constraint(ClassSeparationConstraint { code })
        .with_state(ClassSeparationState)
        .build()
}

struct ClassSeparationConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for ClassSeparationConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => {
                let separation = job.dimens().get_job_class_separation()?;
                with_tour_classes(route_ctx, |classes| {
                    classes.tour.conflicts_with_job(&separation.class, &separation.not_in_same_tour)
                })
                .then_some(ConstraintViolation { code: self.code, stopped: true })
            }
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                let job = activity_ctx.target.retrieve_job()?;
                let size = job.as_multi()?.jobs.len();
                with_tour_classes(route_ctx, |classes| classes.has_run_conflict(&job, size, activity_ctx.index))
                    .then_some(ConstraintViolation { code: self.code, stopped: false })
            }
        }
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, ViolationCode> {
        let has_separation = |job: &Job| job.dimens().get_job_class_separation().is_some();
        if has_separation(&source) || has_separation(&candidate) { Err(self.code) } else { Ok(source) }
    }
}

struct ClassSeparationState;

impl FeatureState for ClassSeparationState {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, _: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
    }

    fn accept_route_state(&self, route_ctx: &mut RouteContext) {
        let classes = get_tour_classes(&route_ctx.route().tour);
        route_ctx.state_mut().set_tour_classes(classes);
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        solution_ctx
            .routes
            .iter_mut()
            .filter(|route_ctx| route_ctx.is_stale())
            .for_each(|route_ctx| self.accept_route_state(route_ctx));
    }
}

/// Classes of some jobs with the classes they exclude.
#[derive(Clone, Default)]
struct ClassMix {
    classes: Vec<String>,
    excluded: Vec<String>,
}

impl ClassMix {
    fn add(&mut self, class: &str, excluded: &[String]) {
        add_class(&mut self.classes, class);
        excluded.iter().for_each(|class| add_class(&mut self.excluded, class));
    }

    fn conflicts_with(&self, other: &ClassMix) -> bool {
        self.classes.iter().any(|class| other.excluded.contains(class))
            || other.classes.iter().any(|class| self.excluded.contains(class))
    }

    fn conflicts_with_job(&self, class: &str, excluded: &[String]) -> bool {
        self.excluded.iter().any(|other| other == class) || self.classes.iter().any(|other| excluded.contains(other))
    }
}

/// Route state: the classes of the tour and of each of its runs.
#[derive(Clone, Default)]
struct TourClasses {
    /// All jobs of the tour with their `not_in_same_tour` classes.
    tour: ClassMix,
    /// Runs with their jobs' `not_in_same_run` classes.
    runs: Vec<TourRun<ClassMix>>,
    /// Multi jobs with only some activities in the tour: the job, its first and last activity index and
    /// the number of its activities. The evaluator inserts a multi job's activities one by one into a
    /// shadow route, so this holds the job being inserted when its last activity is evaluated.
    partial: Vec<(Job, usize, usize, usize)>,
}

impl TourClasses {
    /// Checks whether completing the multi `job` with the target inserted after `index` makes one run of
    /// jobs with conflicting classes. Only the activity which completes the job changes the runs.
    fn has_run_conflict(&self, job: &Job, size: usize, index: usize) -> bool {
        let (count, first, last) = self
            .partial
            .iter()
            .find(|(partial, ..)| partial == job)
            .map_or((0, usize::MAX, 0), |(_, first, last, count)| (*count, *first, *last));
        if count + 1 != size {
            return false;
        }

        // the target goes right after `index`
        let span = (first.min(index + 1), last.max(index));
        let Some(start) = self.runs.iter().position(|run| run.overlaps(span)) else {
            return false;
        };
        let joined = &self.runs[start..];
        let joined = &joined[..joined.iter().take_while(|run| run.overlaps(span)).count()];

        let separation = job.dimens().get_job_class_separation();
        joined.iter().enumerate().any(|(idx, run)| {
            separation
                .is_some_and(|separation| run.value.conflicts_with_job(&separation.class, &separation.not_in_same_run))
                || joined[..idx].iter().any(|other| other.value.conflicts_with(&run.value))
        })
    }
}

fn with_tour_classes<R>(route_ctx: &RouteContext, check: impl FnOnce(&TourClasses) -> R) -> R {
    match route_ctx.state().get_tour_classes() {
        Some(classes) => check(classes),
        None => check(&get_tour_classes(&route_ctx.route().tour)),
    }
}

fn get_tour_classes(tour: &Tour) -> TourClasses {
    let mut classes = TourClasses::default();

    tour.jobs().filter_map(|job| job.dimens().get_job_class_separation()).for_each(|separation| {
        classes.tour.add(&separation.class, &separation.not_in_same_tour);
    });

    // NOTE without a class anywhere in the tour, no run can conflict.
    if classes.tour.classes.is_empty() {
        return classes;
    }

    classes.runs = get_tour_runs(tour, |mix: &mut ClassMix, job| {
        if let Some(separation) = job.dimens().get_job_class_separation() {
            mix.add(&separation.class, &separation.not_in_same_run);
        }
    });

    let mut spans = FxHashMap::default();
    collect_multi_job_spans(tour, &mut spans);
    classes.partial = spans
        .into_iter()
        .filter(|(job, (.., count))| !is_complete(job, *count))
        .map(|(job, (first, last, count))| (job, first, last, count))
        .collect();

    classes
}

fn add_class(classes: &mut Vec<String>, class: &str) {
    if !classes.iter().any(|other| other == class) {
        classes.push(class.to_string());
    }
}
