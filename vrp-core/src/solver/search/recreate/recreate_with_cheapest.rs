use crate::construction::heuristics::InsertionContext;
use crate::construction::heuristics::*;
use crate::solver::RefinementContext;
use crate::solver::search::ConfigurableRecreate;
use crate::solver::search::recreate::Recreate;
use rosomaxa::prelude::Random;
use std::sync::Arc;

/// A recreate method which is equivalent to cheapest insertion heuristic.
pub struct RecreateWithCheapest {
    recreate: ConfigurableRecreate,
}

impl RecreateWithCheapest {
    /// Creates a new instance of `RecreateWithCheapest`.
    pub fn new(random: Arc<dyn Random>) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                Box::<AllJobSelector>::default(),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<BestResultSelector>::default()),
                Default::default(),
            ),
        }
    }

    /// Creates a new instance with the per-iteration job pool capped at `cap`.
    ///
    /// Construction-time speedup variant: instead of evaluating all
    /// unassigned jobs against all routes each outer iteration (the standard
    /// cheapest insertion's O(N²) cost), evaluate at most `cap` random jobs.
    /// Quality drops to "best of K" but per-iteration work shrinks
    /// proportionally. Only applicable when `prepare()`'s default shuffle
    /// gives a fresh random sample each call.
    pub fn with_cap(random: Arc<dyn Random>, cap: usize) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                Box::new(CappedJobSelector::new(cap)),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<BestResultSelector>::default()),
                Default::default(),
            ),
        }
    }

    /// Creates a new instance with the per-iteration job pool configured from
    /// the [`CONSTRUCTION_JOB_CAP_ENV`] env var. Default is uncapped (matches
    /// [`RecreateWithCheapest::new`]); set `SOLVER_CONSTRUCTION_JOB_CAP=N` to
    /// cap to K=N. Intended for the construction-phase init operator chain
    /// only — refinement-time call sites should use `new()` to keep full
    /// cheapest-of-N quality.
    pub fn with_cap_from_env(random: Arc<dyn Random>) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                construction_job_selector_from_env(),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<BestResultSelector>::default()),
                Default::default(),
            ),
        }
    }
}

impl Recreate for RecreateWithCheapest {
    fn run(&self, refinement_ctx: &RefinementContext, insertion_ctx: InsertionContext) -> InsertionContext {
        self.recreate.run(refinement_ctx, insertion_ctx)
    }
}
