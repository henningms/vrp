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
use crate::construction::enablers::{TourRun, get_insertion_run_span, get_tour_runs};

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

/// Runs of a tour with the distinct sites each serves. The state is the same for every counting
/// mode, so features can share it.
#[derive(Clone, Default)]
struct SiteRuns {
    runs: Vec<TourRun<Vec<u64>>>,
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
                    site_runs.runs.iter().map(|run| self.count.of(run.value.len())).sum()
                };
                route_ctx.state().get_site_runs().map_or_else(|| penalty(&get_site_runs(route_ctx)), penalty)
            })
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else {
            return Cost::default();
        };
        // NOTE only the activity which completes a multi job changes its runs: earlier ones cost nothing.
        let Some(span) = get_insertion_run_span(route_ctx, activity_ctx) else {
            return Cost::default();
        };
        let site = activity_ctx.target.retrieve_job().and_then(|job| job.dimens().get_job_site().copied());

        let computed;
        let site_runs = match route_ctx.state().get_site_runs() {
            Some(site_runs) => site_runs,
            None => {
                computed = get_site_runs(route_ctx);
                &computed
            }
        };

        let mut sites = site.into_iter().collect::<Vec<_>>();
        let mut merged_penalty = Cost::default();
        site_runs.runs.iter().filter(|run| run.overlaps(span)).for_each(|run| {
            merged_penalty += self.count.of(run.value.len());
            run.value.iter().for_each(|site| add_site(&mut sites, *site));
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

fn get_site_runs(route_ctx: &RouteContext) -> SiteRuns {
    let runs = get_tour_runs(&route_ctx.route().tour, |sites: &mut Vec<u64>, job| {
        job.dimens().get_job_site().into_iter().for_each(|site| add_site(sites, *site))
    });

    SiteRuns { runs }
}

fn add_site(sites: &mut Vec<u64>, site: u64) {
    if !sites.contains(&site) {
        sites.push(site);
    }
}
