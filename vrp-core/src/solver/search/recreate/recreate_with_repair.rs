use crate::construction::heuristics::*;
use crate::solver::RefinementContext;
use crate::solver::search::ConfigurableRecreate;
use crate::solver::search::recreate::{PhasedRecreate, Recreate};
use rosomaxa::prelude::{Random, SelectionPhase};
use std::collections::HashMap;
use std::sync::Arc;

/// A recreate strategy that re-attempts unassigned jobs with cost-blind first-fit
/// insertion. Intended as a safety net inside `PhasedRecreate` for the exploration
/// and exploitation phases — never used during construction (Initial), where the
/// cost-aware default produces the seed solution.
///
/// This operator pairs the [`UnassignedJobSelector`] with [`AnyFeasibleResultSelector`]
/// so that any unassigned job whose feasibility region is non-empty is reinserted
/// regardless of whether the resulting tour is cost-best. Lex-strict
/// `minimize-unassigned` at the solution level guarantees the move is accepted only
/// if it genuinely reduces unassigned count.
pub struct RecreateWithRepair {
    recreate: ConfigurableRecreate,
}

impl RecreateWithRepair {
    /// Creates a new instance of `RecreateWithRepair`.
    pub fn new(random: Arc<dyn Random>) -> Self {
        Self {
            recreate: ConfigurableRecreate::new(
                Box::<UnassignedJobSelector>::default(),
                Box::<AllRouteSelector>::default(),
                LegSelection::Stochastic(random),
                ResultSelection::Concrete(Box::<AnyFeasibleResultSelector>::default()),
                Default::default(),
            ),
        }
    }

    /// Wraps the operator in a `PhasedRecreate` that uses `default_recreate` during
    /// the `Initial` (construction) phase and the repair operator during
    /// `Exploration` and `Exploitation`.
    pub fn default_phased(default_recreate: Arc<dyn Recreate>, random: Arc<dyn Random>) -> PhasedRecreate {
        let repair: Arc<dyn Recreate> = Arc::new(Self::new(random));
        let mut recreates: HashMap<SelectionPhase, Arc<dyn Recreate>> = HashMap::default();
        recreates.insert(SelectionPhase::Initial, default_recreate);
        recreates.insert(SelectionPhase::Exploration, repair.clone());
        recreates.insert(SelectionPhase::Exploitation, repair);
        PhasedRecreate::new(recreates)
    }
}

impl Recreate for RecreateWithRepair {
    fn run(&self, refinement_ctx: &RefinementContext, insertion_ctx: InsertionContext) -> InsertionContext {
        self.recreate.run(refinement_ctx, insertion_ctx)
    }
}
