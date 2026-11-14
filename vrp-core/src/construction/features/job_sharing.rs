//! Provides a constraint which keeps jobs with some labels apart.
//!
//! For example, an operator runs several passenger services on one vehicle: riders of some services
//! must not be on board together, and some services must not use the same vehicle tour at all.
//!
//! # Semantics
//! - A job carries any number of labels and lists the labels it must not share a run or a tour with.
//! - Two jobs conflict when either one lists a label the other carries, so one side stating it is enough.
//!   A job may list a label it carries itself: then no two jobs with that label and that list may share a
//!   run or a tour.
//! - A *run* is a maximal stretch of a tour while at least one pickup-delivery job is on board, see
//!   [get_tour_runs](crate::construction::enablers::get_tour_runs). The run level applies to
//!   pickup-delivery (multi) jobs only; the tour level applies to every job.
//! - A job without sharing can still join two runs into one, so it is refused when the runs it joins
//!   hold conflicting labels.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/job_sharing_test.rs"]
mod job_sharing_test;

use super::*;
use crate::construction::enablers::{TourRun, collect_multi_job_spans, get_tour_runs, is_complete};
use crate::models::solution::Tour;
use rustc_hash::FxHashMap;

custom_dimension!(pub JobSharing typeof JobSharing);
custom_tour_state!(TourSharing typeof TourSharing);

/// Labels a job carries with the labels it must not share a run or a tour with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobSharing {
    /// Labels the job carries.
    pub labels: Vec<String>,
    /// Labels of pickup-delivery jobs which must not be on board together with this job.
    pub run_none_of: Vec<String>,
    /// Labels of jobs which must not be served in the same tour as this job.
    pub tour_none_of: Vec<String>,
}

/// Creates a job sharing feature as a hard constraint.
pub fn create_job_sharing_feature(name: &str, code: ViolationCode) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name(name)
        .with_constraint(JobSharingConstraint { code })
        .with_state(JobSharingState)
        .build()
}

struct JobSharingConstraint {
    code: ViolationCode,
}

impl FeatureConstraint for JobSharingConstraint {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => {
                let sharing = job.dimens().get_job_sharing()?;
                with_tour_sharing(route_ctx, |state| {
                    state.tour.conflicts_with_job(&sharing.labels, &sharing.tour_none_of)
                })
                .then_some(ConstraintViolation { code: self.code, stopped: true })
            }
            MoveContext::Activity { route_ctx, activity_ctx, .. } => {
                let job = activity_ctx.target.retrieve_job()?;
                let size = job.as_multi()?.jobs.len();
                with_tour_sharing(route_ctx, |state| state.has_run_conflict(&job, size, activity_ctx.index))
                    .then_some(ConstraintViolation { code: self.code, stopped: false })
            }
        }
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, ViolationCode> {
        let has_sharing = |job: &Job| job.dimens().get_job_sharing().is_some();
        if has_sharing(&source) || has_sharing(&candidate) { Err(self.code) } else { Ok(source) }
    }
}

struct JobSharingState;

impl FeatureState for JobSharingState {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, _: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
    }

    fn accept_route_state(&self, route_ctx: &mut RouteContext) {
        let state = get_tour_sharing(&route_ctx.route().tour);
        route_ctx.state_mut().set_tour_sharing(state);
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        solution_ctx
            .routes
            .iter_mut()
            .filter(|route_ctx| route_ctx.is_stale())
            .for_each(|route_ctx| self.accept_route_state(route_ctx));
    }
}

/// Labels some jobs carry with the labels they exclude.
#[derive(Clone, Default)]
struct LabelMix {
    labels: Vec<String>,
    excluded: Vec<String>,
}

impl LabelMix {
    fn add(&mut self, labels: &[String], excluded: &[String]) {
        labels.iter().for_each(|label| add_label(&mut self.labels, label));
        excluded.iter().for_each(|label| add_label(&mut self.excluded, label));
    }

    fn conflicts_with(&self, other: &LabelMix) -> bool {
        self.conflicts_with_job(&other.labels, &other.excluded)
    }

    fn conflicts_with_job(&self, labels: &[String], excluded: &[String]) -> bool {
        intersects(&self.excluded, labels) || intersects(&self.labels, excluded)
    }
}

/// Route state: the labels of the tour and of each of its runs.
#[derive(Clone, Default)]
struct TourSharing {
    /// All jobs of the tour with their `tour_none_of` labels.
    tour: LabelMix,
    /// Runs with their jobs' `run_none_of` labels.
    runs: Vec<TourRun<LabelMix>>,
    /// Multi jobs with only some activities in the tour: the job, its first and last activity index and
    /// the number of its activities. The evaluator inserts a multi job's activities one by one into a
    /// shadow route, so this holds the job being inserted when its last activity is evaluated.
    partial: Vec<(Job, usize, usize, usize)>,
}

impl TourSharing {
    /// Checks whether completing the multi `job` with the target inserted after `index` makes one run of
    /// jobs with conflicting labels. Only the activity which completes the job changes the runs.
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

        let sharing = job.dimens().get_job_sharing();
        joined.iter().enumerate().any(|(idx, run)| {
            sharing.is_some_and(|sharing| run.value.conflicts_with_job(&sharing.labels, &sharing.run_none_of))
                || joined[..idx].iter().any(|other| other.value.conflicts_with(&run.value))
        })
    }
}

fn with_tour_sharing<R>(route_ctx: &RouteContext, check: impl FnOnce(&TourSharing) -> R) -> R {
    match route_ctx.state().get_tour_sharing() {
        Some(state) => check(state),
        None => check(&get_tour_sharing(&route_ctx.route().tour)),
    }
}

fn get_tour_sharing(tour: &Tour) -> TourSharing {
    let mut state = TourSharing::default();

    tour.jobs().filter_map(|job| job.dimens().get_job_sharing()).for_each(|sharing| {
        state.tour.add(&sharing.labels, &sharing.tour_none_of);
    });

    // NOTE a conflict needs a job carrying a label, and a multi job being inserted has its first activity
    // in the tour by the time its run is checked, so without a label anywhere in the tour no run can conflict.
    if state.tour.labels.is_empty() {
        return state;
    }

    state.runs = get_tour_runs(tour, |mix: &mut LabelMix, job| {
        if let Some(sharing) = job.dimens().get_job_sharing() {
            mix.add(&sharing.labels, &sharing.run_none_of);
        }
    });

    let mut spans = FxHashMap::default();
    collect_multi_job_spans(tour, &mut spans);
    state.partial = spans
        .into_iter()
        .filter(|(job, (.., count))| !is_complete(job, *count))
        .map(|(job, (first, last, count))| (job, first, last, count))
        .collect();

    state
}

fn add_label(labels: &mut Vec<String>, label: &str) {
    if !labels.iter().any(|other| other == label) {
        labels.push(label.to_string());
    }
}

fn intersects(left: &[String], right: &[String]) -> bool {
    left.iter().any(|label| right.contains(label))
}
