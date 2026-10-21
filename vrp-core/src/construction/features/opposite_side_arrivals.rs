//! Provides a feature to minimize opposite-side arrivals.
//!
//! Whoever builds the routing matrix can route arrivals at some locations on the curb side, so a
//! rider does not cross the road, and mark the cells where that costs too much: the route of a
//! marked cell arrives with its destination on the opposite side. Every leg a tour drives along a
//! marked cell counts as one unit, so a stop counts once however many jobs it serves.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/opposite_side_arrivals_test.rs"]
mod opposite_side_arrivals_test;

use super::*;
use crate::models::solution::Activity;

/// Creates a feature which minimizes legs driven along routing cells marked opposite-side.
pub fn create_opposite_side_arrivals_feature(name: &str, cells: Arc<OppositeSideCells>) -> GenericResult<Feature> {
    FeatureBuilder::default().with_name(name).with_objective(OppositeSideArrivalsObjective { cells }).build()
}

/// Routing matrix cells, per profile, whose route arrives with its destination on the opposite side.
pub struct OppositeSideCells {
    size: usize,
    profiles: Vec<Vec<u64>>,
}

impl OppositeSideCells {
    /// Creates cells of a `size` x `size` matrix from flat `from * size + to` indices, given per
    /// profile index.
    pub fn new(size: usize, cells: impl IntoIterator<Item = (usize, Vec<usize>)>) -> GenericResult<Self> {
        let cell_count = size * size;
        let mut profiles: Vec<Vec<u64>> = Vec::new();

        for (profile, indices) in cells {
            if profiles.len() <= profile {
                profiles.resize(profile + 1, Vec::new());
            }
            let bits = &mut profiles[profile];
            if bits.is_empty() {
                bits.resize(cell_count.div_ceil(64), 0);
            }
            for index in indices {
                if index >= cell_count {
                    return Err(format!("opposite-side cell {index} is outside a {size}x{size} matrix").into());
                }
                bits[index / 64] |= 1 << (index % 64);
            }
        }

        Ok(Self { size, profiles })
    }

    /// Returns true when the route from `from` to `to` arrives with `to` on the opposite side.
    /// Staying at a location is no arrival, and locations outside the matrix, such as custom
    /// ones, are never marked.
    pub fn is_opposite(&self, profile: &Profile, from: Location, to: Location) -> bool {
        if from == to || from >= self.size || to >= self.size {
            return false;
        }
        let index = from * self.size + to;

        self.profiles
            .get(profile.index)
            .and_then(|bits| bits.get(index / 64))
            .is_some_and(|word| (word >> (index % 64)) & 1 == 1)
    }
}

struct OppositeSideArrivalsObjective {
    cells: Arc<OppositeSideCells>,
}

impl OppositeSideArrivalsObjective {
    fn arrival(&self, profile: &Profile, from: &Activity, to: &Activity) -> Cost {
        if self.cells.is_opposite(profile, from.place.location, to.place.location) { 1. } else { Cost::default() }
    }
}

impl FeatureObjective for OppositeSideArrivalsObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        solution
            .solution
            .routes
            .iter()
            .map(|route_ctx| {
                let route = route_ctx.route();
                let profile = &route.actor.vehicle.profile;
                let activities = route.tour.all_activities();

                activities
                    .clone()
                    .zip(activities.skip(1))
                    .map(|(from, to)| self.arrival(profile, from, to))
                    .sum::<Cost>()
            })
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else {
            return Cost::default();
        };
        let profile = &route_ctx.route().actor.vehicle.profile;
        let (prev, target) = (activity_ctx.prev, activity_ctx.target);

        let into_next = activity_ctx
            .next
            .map_or(Cost::default(), |next| self.arrival(profile, target, next) - self.arrival(profile, prev, next));

        self.arrival(profile, prev, target) + into_next
    }
}
