//! Provides the way to charge a vehicle's per-run cost.
//!
//! A run starts when a pickup-delivery job gets on board an otherwise empty vehicle and ends when the
//! vehicle is empty again. Runs are counted on stops, i.e. on consecutive tour activities at the same
//! location: a job is on board between the stop of its first activity and the stop of its last one,
//! and it shares a run with a job picked up at an earlier stop and delivered at a later one. So a
//! drop-off and a pickup at the same stop is a handover between two runs, whatever the order of the
//! activities within the stop, unless another job stays on board across it. A job which is picked up
//! and delivered at the same stop opens no run, and only multi jobs with all their activities in the
//! tour count.
//!
//! NOTE site mixing counts runs on activities instead, see [get_tour_runs](super::get_tour_runs).

#[cfg(test)]
#[path = "../../../tests/unit/construction/enablers/run_cost_test.rs"]
mod run_cost_test;

use super::tour_runs::{collect_multi_job_spans, is_complete};
use crate::construction::heuristics::{ActivityContext, RouteContext, RouteState};
use crate::models::common::Cost;
use crate::models::problem::{Job, Multi, VehiclePerRunCostDimension};
use crate::models::solution::{Activity, Tour};
use rustc_hash::FxHashMap;
use std::sync::Arc;

// NOTE the state is updated in place, so that a route reuses its buffers
custom_tour_state!(StopRuns typeof StopRuns, setter(allow(dead_code)));

/// Runs of a tour on stops with what an insertion estimate needs to price a run change.
#[derive(Default)]
struct StopRuns {
    /// Stop index of every tour activity.
    stops: Vec<usize>,
    /// First and last activity index of every multi job with all its activities in the tour, sorted.
    spans: Vec<(usize, usize)>,
    /// Multi jobs with only some of their activities in the tour: first, last activity index and count.
    partial: Vec<(Job, usize, usize, usize)>,
    /// Runs as the stops (start, end) between which the vehicle carries jobs, sorted and disjoint.
    runs: Vec<(usize, usize)>,
    /// A buffer which groups activities by job.
    jobs: FxHashMap<Job, (usize, usize, usize)>,
}

impl StopRuns {
    fn new(tour: &Tour) -> Self {
        let mut stop_runs = Self::with_capacity(tour.total());
        stop_runs.update(tour);

        stop_runs
    }

    fn with_capacity(activities: usize) -> Self {
        Self {
            stops: Vec::with_capacity(activities),
            spans: Vec::with_capacity(activities / 2),
            partial: Vec::default(),
            runs: Vec::with_capacity(activities / 2),
            jobs: FxHashMap::with_capacity_and_hasher(activities / 2, Default::default()),
        }
    }

    fn update(&mut self, tour: &Tour) {
        let Self { stops, spans, partial, runs, jobs } = self;

        stops.clear();
        let mut last_location = None;
        tour.all_activities().for_each(|activity| {
            let location = activity.place.location;
            let stop = match (stops.last(), last_location) {
                (Some(&stop), Some(last)) if last == location => stop,
                (Some(&stop), _) => stop + 1,
                (None, _) => 0,
            };
            last_location = Some(location);
            stops.push(stop);
        });

        collect_multi_job_spans(tour, jobs);
        spans.clear();
        partial.clear();
        jobs.drain().for_each(|(job, (first, last, count))| {
            if is_complete(&job, count) {
                spans.push((first, last));
            } else {
                partial.push((job, first, last, count));
            }
        });
        spans.sort_unstable();

        runs.clear();
        spans.iter().for_each(|&(first, last)| add_to_runs(runs, (stops[first], stops[last])));
    }

    /// Estimates the change in the number of runs when the target activity is inserted.
    fn estimate(&self, activity_ctx: &ActivityContext) -> Option<isize> {
        let ActivityContext { index, prev, target, next } = activity_ctx;
        let (index, location) = (*index, target.place.location);
        let prev_stop = self.stops[index];
        let splits_stop =
            next.is_some_and(|next| next.place.location == prev.place.location) && location != prev.place.location;
        let completed = self.get_completed_span(target);

        if splits_stop {
            return Some(self.count_runs_with_split(index, completed) as isize - self.runs.len() as isize);
        }

        let (first, last) = completed?;

        // NOTE positions are stop indices times three, so that a new stop fits between two existing ones
        let target_position = if location == prev.place.location {
            3 * prev_stop
        } else if next.is_some_and(|next| next.place.location == location) {
            3 * (prev_stop + 1)
        } else {
            3 * prev_stop + 1
        };
        let lo = (3 * self.stops[first]).min(target_position);
        let hi = (3 * self.stops[last]).max(target_position);
        if lo == hi {
            return Some(0);
        }

        let from = self.runs.partition_point(|&(_, end)| 3 * end <= lo);
        let to = self.runs.partition_point(|&(start, _)| 3 * start < hi);

        Some(1 - to.saturating_sub(from) as isize)
    }

    /// Counts runs after inserting a target with a new location between two activities of one stop,
    /// which splits the stop in two: jobs dropped off after the split point now ride across the target.
    fn count_runs_with_split(&self, index: usize, completed: Option<(usize, usize)>) -> usize {
        let split_stop = self.stops[index];
        let position = |idx: usize| {
            let stop = self.stops[idx];
            3 * stop + if stop == split_stop && idx > index { 2 } else { 0 }
        };
        let target_position = 3 * split_stop + 1;

        let mut target_span =
            completed.map(|(first, last)| (position(first).min(target_position), position(last).max(target_position)));
        let mut runs = (0, None);
        self.spans.iter().for_each(|&(first, last)| {
            let span = (position(first), position(last));
            if let Some(target_span) = target_span.take_if(|target_span| target_span.0 <= span.0) {
                count_run(&mut runs, target_span);
            }
            count_run(&mut runs, span);
        });
        if let Some(target_span) = target_span {
            count_run(&mut runs, target_span);
        }

        runs.0
    }

    /// Returns the first and last activity index of the other activities of the target's multi job when
    /// the target is its last missing activity.
    fn get_completed_span(&self, target: &Activity) -> Option<(usize, usize)> {
        if self.partial.is_empty() {
            return None;
        }

        let multi = Multi::roots(target.job.as_ref()?)?;
        let (_, first, last, count) =
            self.partial.iter().find(|(job, ..)| job.as_multi().is_some_and(|other| Arc::ptr_eq(other, &multi)))?;

        (count + 1 == multi.jobs.len()).then_some((*first, *last))
    }
}

/// Adds a span of stops, sorted after the previous ones, to runs.
fn add_to_runs(runs: &mut Vec<(usize, usize)>, (start, end): (usize, usize)) {
    if start == end {
        return;
    }

    match runs.last_mut() {
        Some(run) if start < run.1 => run.1 = run.1.max(end),
        _ => runs.push((start, end)),
    }
}

/// Counts a span, sorted after the previous ones, as `add_to_runs` does with (count, last run end).
fn count_run(runs: &mut (usize, Option<usize>), (start, end): (usize, usize)) {
    if start == end {
        return;
    }

    match runs.1 {
        Some(run_end) if start < run_end => runs.1 = Some(run_end.max(end)),
        _ => *runs = (runs.0 + 1, Some(end)),
    }
}

/// Counts the runs of the tour.
pub fn count_tour_runs(tour: &Tour) -> usize {
    StopRuns::new(tour).runs.len()
}

/// Caches the route's runs when its vehicle has a per-run cost.
pub fn update_route_runs(route_ctx: &mut RouteContext) {
    if route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost().is_none() {
        return;
    }

    let (route, state) = route_ctx.as_mut();
    state
        .get_or_init_exclusive_tour_state::<StopRunsTourStateKey, StopRuns>(|| {
            StopRuns::with_capacity(route.tour.total())
        })
        .update(&route.tour);
}

/// Returns the route's run fees: its vehicle's per-run cost for every run of the tour, or `None` when
/// the vehicle has no per-run cost.
pub fn get_route_run_cost(route_ctx: &RouteContext) -> Option<Cost> {
    let per_run = *route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost()?;
    let runs = match route_ctx.state().get_stop_runs() {
        Some(stop_runs) => stop_runs.runs.len(),
        None => count_tour_runs(&route_ctx.route().tour),
    };

    Some(per_run * runs as Cost)
}

/// Estimates the change of the route's run fees when the target activity is inserted. Completing a
/// multi job opens a new run when the vehicle is otherwise empty over the job's stops, joins the run
/// it overlaps, or merges every run it overlaps into one. Any activity which splits a stop can merge
/// the runs handed over there. Returns `None` when the vehicle has no per-run cost or nothing changes.
pub fn estimate_route_run_cost(route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> Option<Cost> {
    let per_run = *route_ctx.route().actor.vehicle.dimens.get_vehicle_per_run_cost()?;

    let delta = match route_ctx.state().get_stop_runs() {
        Some(stop_runs) => stop_runs.estimate(activity_ctx),
        None => StopRuns::new(&route_ctx.route().tour).estimate(activity_ctx),
    }?;

    Some(per_run * delta as Cost)
}
