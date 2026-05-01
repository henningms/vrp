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
    /// Creates a new instance of `RecreateWithSoloAwareCheapest`.
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
}

impl Recreate for RecreateWithSoloAwareCheapest {
    fn run(&self, refinement_ctx: &RefinementContext, insertion_ctx: InsertionContext) -> InsertionContext {
        self.recreate.run(refinement_ctx, insertion_ctx)
    }
}
