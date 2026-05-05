use crate::construction::heuristics::*;
use crate::solver::RefinementContext;
use crate::solver::search::ConfigurableRecreate;
use crate::solver::search::recreate::Recreate;
use rosomaxa::prelude::Random;
use std::sync::Arc;

/// A cost-cheapest recreate strategy that prefers placing solo-riding jobs onto
/// empty routes via [`SoloAwareResultSelector`].
///
/// Intended as the construction-phase default when solo-riding jobs are present.
/// Behaves identically to [`RecreateWithCheapest`] for non-solo jobs; for solos,
/// when both an empty-route and an in-use-route placement are feasible for the
/// same job, picks the empty-route option regardless of cost.
///
/// This is a construction-time heuristic only — refinement-phase recreate paths
/// keep cost-pure cheapest behaviour to avoid conflicting with `minimize-tours`
/// during the metaheuristic phase.
///
/// [`RecreateWithCheapest`]: super::RecreateWithCheapest
pub struct RecreateWithSoloAwareCheapest {
    recreate: ConfigurableRecreate,
}

impl RecreateWithSoloAwareCheapest {
    /// Creates a new instance of `RecreateWithSoloAwareCheapest` with the
    /// upstream uncapped per-iteration job pool. Matches the standard
    /// cheapest-insertion algorithm's `O(jobs × routes × positions)` cost
    /// per outer iteration.
    pub fn new(random: Arc<dyn Random>) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                Box::<AllJobSelector>::default(),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<SoloAwareResultSelector>::default()),
                Default::default(),
            ),
        }
    }

    /// Creates a new instance with the per-iteration job pool capped at `cap`.
    ///
    /// Construction-time speedup variant — analogous to
    /// [`RecreateWithCheapest::with_cap`]. See its docs for the trade-off.
    /// Pairs with the default `prepare()` shuffle so each call gets a fresh
    /// random sample of K jobs from the unassigned set.
    pub fn with_cap(random: Arc<dyn Random>, cap: usize) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                Box::new(CappedJobSelector::new(cap)),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<SoloAwareResultSelector>::default()),
                Default::default(),
            ),
        }
    }
}

impl Recreate for RecreateWithSoloAwareCheapest {
    fn run(&self, refinement_ctx: &RefinementContext, insertion_ctx: InsertionContext) -> InsertionContext {
        self.recreate.run(refinement_ctx, insertion_ctx)
    }
}
