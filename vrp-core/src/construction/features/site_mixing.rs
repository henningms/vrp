//! Provides features to minimize mixing of sites within a run and the number of site visits.
//!
//! A *run* is a maximal stretch of a tour while at least one pickup-delivery job is on board, i.e.
//! the union of overlapping spans from each job's first to its last activity. A job can carry a site
//! key (e.g. the school it travels to or from). Jobs without a site keep the vehicle loaded, so they
//! can join runs, but they add no site.
//!
//! * site mixing: a run costs one unit for every distinct site beyond the first, so a run serving
//!   one school costs nothing and a run mixing three schools costs two.
//! * site visits: a run costs one unit for every distinct site, so serving a school with one more
//!   run costs one unit, which pushes riders of the same site into the same run.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/site_mixing_test.rs"]
mod site_mixing_test;

use super::*;
use rustc_hash::FxHashMap;

custom_dimension!(pub JobSite typeof u64);
custom_tour_state!(SiteRuns typeof SiteRuns);

/// Creates a feature which minimizes the number of extra sites served within each run.
pub fn create_site_mixing_feature(name: &str) -> GenericResult<Feature> {
    create_feature(name, SiteCount::Extra)
}

/// Creates a feature which minimizes the number of site visits: distinct sites summed over runs.
pub fn create_site_visits_feature(name: &str) -> GenericResult<Feature> {
    create_feature(name, SiteCount::All)
}

fn create_feature(name: &str, count: SiteCount) -> GenericResult<Feature> {
    FeatureBuilder::default()
        .with_name(name)
        .with_objective(SiteRunsObjective { count })
        .with_state(SiteRunsState)
        .build()
}

/// Specifies which distinct sites of a run are counted.
#[derive(Clone, Copy)]
enum SiteCount {
    /// Every site beyond the first one.
    Extra,
    /// Every site.
    All,
}

impl SiteCount {
    fn of(&self, sites: usize) -> Cost {
        match self {
            SiteCount::Extra => sites.saturating_sub(1) as Cost,
            SiteCount::All => sites as Cost,
        }
    }
}

/// Runs of a tour. The state is the same for every counting mode, so features can share it.
#[derive(Clone, Default)]
struct SiteRuns {
    runs: Vec<Run>,
}

/// A run as an inclusive range of activity indices with the distinct sites served.
#[derive(Clone)]
struct Run {
    start: usize,
    end: usize,
    sites: Vec<u64>,
}

struct SiteRunsObjective {
    count: SiteCount,
}

impl FeatureObjective for SiteRunsObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        solution
            .solution
            .routes
            .iter()
            .map(|route_ctx| {
                let penalty = |site_runs: &SiteRuns| -> Cost {
                    site_runs.runs.iter().map(|run| self.count.of(run.sites.len())).sum()
                };
                route_ctx.state().get_site_runs().map_or_else(|| penalty(&get_site_runs(route_ctx)), penalty)
            })
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else {
            return Cost::default();
        };
        let Some(job) = activity_ctx.target.retrieve_job() else {
            return Cost::default();
        };
        let Some(multi) = job.as_multi() else {
            return Cost::default();
        };

        // NOTE the evaluator inserts a multi job's activities one by one into a shadow route, so the
        // job's runs are known only when its last activity is evaluated: earlier ones cost nothing.
        let placed = route_ctx.route().tour.all_activities().enumerate().filter(|(_, activity)| {
            activity.job.as_ref().is_some_and(|single| multi.jobs.iter().any(|sub| Arc::ptr_eq(sub, single)))
        });
        let (count, first, last) = placed
            .fold((0, usize::MAX, 0), |(count, first, last), (idx, _)| (count + 1, first.min(idx), last.max(idx)));
        if count + 1 != multi.jobs.len() {
            return Cost::default();
        }

        // the target goes right after `index`, so the job stays on board over [lo, hi] of the current tour
        let index = activity_ctx.index;
        let (lo, hi) = (first.min(index + 1), last.max(index));

        let computed;
        let site_runs = match route_ctx.state().get_site_runs() {
            Some(site_runs) => site_runs,
            None => {
                computed = get_site_runs(route_ctx);
                &computed
            }
        };

        let mut sites = job.dimens().get_job_site().copied().into_iter().collect::<Vec<_>>();
        let mut merged_penalty = Cost::default();
        site_runs.runs.iter().filter(|run| run.start <= hi && run.end >= lo).for_each(|run| {
            merged_penalty += self.count.of(run.sites.len());
            run.sites.iter().for_each(|site| add_site(&mut sites, *site));
        });

        self.count.of(sites.len()) - merged_penalty
    }
}

struct SiteRunsState;

impl FeatureState for SiteRunsState {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, _: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
    }

    fn accept_route_state(&self, route_ctx: &mut RouteContext) {
        let site_runs = get_site_runs(route_ctx);
        route_ctx.state_mut().set_site_runs(site_runs);
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        solution_ctx
            .routes
            .iter_mut()
            .filter(|route_ctx| route_ctx.is_stale())
            .for_each(|route_ctx| self.accept_route_state(route_ctx));
    }
}

/// Builds runs from multi jobs which have all their activities in the tour.
fn get_site_runs(route_ctx: &RouteContext) -> SiteRuns {
    // job -> (first index, last index, activities in tour)
    let mut spans: FxHashMap<Job, (usize, usize, usize)> = FxHashMap::default();
    route_ctx.route().tour.all_activities().enumerate().for_each(|(idx, activity)| {
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
        .map(|(job, (first, last, _))| (first, last, job.dimens().get_job_site().copied()))
        .collect::<Vec<_>>();
    spans.sort_unstable_by_key(|(first, ..)| *first);

    let runs = spans.into_iter().fold(Vec::<Run>::new(), |mut runs, (first, last, site)| {
        match runs.last_mut() {
            Some(run) if first < run.end => {
                run.end = run.end.max(last);
                site.into_iter().for_each(|site| add_site(&mut run.sites, site));
            }
            _ => runs.push(Run { start: first, end: last, sites: site.into_iter().collect() }),
        }
        runs
    });

    SiteRuns { runs }
}

fn add_site(sites: &mut Vec<u64>, site: u64) {
    if !sites.contains(&site) {
        sites.push(site);
    }
}
