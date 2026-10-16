//! Provides the way to split a tour into runs.
//!
//! A *run* is a maximal stretch of a tour while at least one pickup-delivery job is on board, i.e.
//! the union of overlapping spans from each multi job's first to its last activity. Only multi jobs
//! with all their activities in the tour count; other jobs never open a run.

#[cfg(test)]
#[path = "../../../tests/unit/construction/enablers/tour_runs_test.rs"]
mod tour_runs_test;

use crate::construction::heuristics::{ActivityContext, RouteContext, RouteState};
use crate::models::common::Cost;
use crate::models::problem::{Job, VehiclePerRunCostDimension};
use crate::models::solution::Tour;
use rustc_hash::FxHashMap;
use std::sync::Arc;

custom_tour_state!(RouteRuns typeof Vec<TourRun<()>>);

/// A run as an inclusive range of tour activity indices with a value folded over its jobs.
#[derive(Clone, Debug, Default)]
pub struct TourRun<T> {
    /// Index of the run's first activity.
    pub start: usize,
    /// Index of the run's last activity.
    pub end: usize,
    /// A value folded over the run's jobs.
    pub value: T,
}

impl<T> TourRun<T> {
    /// Checks whether a job on board over the tour activities `[lo, hi]` shares this run.
    pub fn overlaps(&self, (lo, hi): (usize, usize)) -> bool {
        self.start <= hi && self.end >= lo
    }
}

/// Splits the tour into runs, folding the jobs of every run into its value with `add_job`.
pub fn get_tour_runs<T: Default>(tour: &Tour, add_job: impl Fn(&mut T, &Job)) -> Vec<TourRun<T>> {
    // job -> (first index, last index, activities in tour)
    let mut spans: FxHashMap<Job, (usize, usize, usize)> = FxHashMap::default();
    tour.all_activities().enumerate().for_each(|(idx, activity)| {
        if let Some(job @ Job::Multi(_)) = activity.retrieve_job() {
            spans
                .entry(job)
                .and_modify(|(_, last, count)| (*last, *count) = (idx, *count + 1))
                .or_insert((idx, idx, 1));
        }
    });

    let mut spans = spans
        .into_iter()
        .filter(|(job, (.., count))| job.as_multi().is_some_and(|multi| multi.jobs.len() == *count))
        .map(|(job, (first, last, _))| (first, last, job))
        .collect::<Vec<_>>();
    spans.sort_unstable_by_key(|(first, ..)| *first);

    spans.into_iter().fold(Vec::<TourRun<T>>::new(), |mut runs, (first, last, job)| {
        match runs.last_mut() {
            Some(run) if first < run.end => {
                run.end = run.end.max(last);
                add_job(&mut run.value, &job);
            }
            _ => {
                let mut value = T::default();
                add_job(&mut value, &job);
                runs.push(TourRun { start: first, end: last, value });
            }
        }
        runs
    })
}

/// Counts the runs of the tour.
pub fn count_tour_runs(tour: &Tour) -> usize {
    get_tour_runs(tour, |_: &mut (), _| {}).len()
}

/// Returns the activities `[lo, hi]` of the current tour over which the target's multi job is on
/// board once the target is inserted, or `None` when the target does not complete a multi job.
///
/// The evaluator inserts a multi job's activities one by one into a shadow route, so the job's span
/// is known only when its last activity is evaluated.
pub fn get_insertion_run_span(route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> Option<(usize, usize)> {
    let job = activity_ctx.target.retrieve_job()?;
    let multi = job.as_multi()?;

    let placed = route_ctx.route().tour.all_activities().enumerate().filter(|(_, activity)| {
        activity.job.as_ref().is_some_and(|single| multi.jobs.iter().any(|sub| Arc::ptr_eq(sub, single)))
    });
    let (count, first, last) =
        placed.fold((0, usize::MAX, 0), |(count, first, last), (idx, _)| (count + 1, first.min(idx), last.max(idx)));
    if count + 1 != multi.jobs.len() {
        return None;
    }

    // the target goes right after `index`
    let index = activity_ctx.index;
    Some((first.min(index + 1), last.max(index)))
}

/// Caches the route's runs when its vehicle has a per-run cost.
pub fn update_route_runs(route_ctx: &mut RouteContext) {
    if route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost().is_some() {
        let runs = get_tour_runs(&route_ctx.route().tour, |_: &mut (), _| {});
        route_ctx.state_mut().set_route_runs(runs);
    }
}

/// Returns the route's run fees: its vehicle's per-run cost for every run of the tour, or `None` when
/// the vehicle has no per-run cost.
pub fn get_route_run_cost(route_ctx: &RouteContext) -> Option<Cost> {
    let per_run = *route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost()?;
    let runs = route_ctx.state().get_route_runs().map_or_else(|| count_tour_runs(&route_ctx.route().tour), Vec::len);

    Some(per_run * runs as Cost)
}

/// Estimates the change of the route's run fees when the target activity is inserted: completing a
/// multi job opens a new run when the vehicle is otherwise empty over the job's span, joins the run
/// it overlaps, or merges every run it overlaps into one. Returns `None` when there is no change to
/// price: the vehicle has no per-run cost or the target does not complete a multi job.
pub fn estimate_route_run_cost(route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> Option<Cost> {
    let per_run = *route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost()?;
    let span = get_insertion_run_span(route_ctx, activity_ctx)?;

    let count_overlapping = |runs: &[TourRun<()>]| runs.iter().filter(|run| run.overlaps(span)).count();
    let overlapping = match route_ctx.state().get_route_runs() {
        Some(runs) => count_overlapping(runs),
        None => count_overlapping(&get_tour_runs(&route_ctx.route().tour, |_: &mut (), _| {})),
    };

    Some(per_run * (1. - overlapping as Cost))
}
