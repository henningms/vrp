//! Domain types for ferry crossings: quays already resolved to routing matrix locations and
//! their sailing schedules. The crossing-aware transport cost consumes a `FerryIndex` to decide
//! whether waiting for a sailing beats the road alternative between two locations.

#[cfg(test)]
#[path = "../../../tests/unit/models/problem/ferry_test.rs"]
mod ferry_test;

use crate::models::common::{Duration, Location, Timestamp};

/// Above this many seconds, a road-network duration between two locations is "no road", not
/// "a very slow road". The routing service represents an unreachable pair as a large finite
/// number rather than an infinity (an actual infinity would poison downstream sums/minimums and
/// cannot round-trip through JSON), so any duration at or above this sentinel must be treated as
/// no road at all. A separate Rust service is intended to apply the same threshold; keep both in
/// sync once it does.
pub const UNREACHABLE_DURATION_THRESHOLD: Duration = 1e9;

/// Direction of travel across a ferry crossing, selecting which sailing list applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FerryDirection {
    /// From quay A to quay B.
    AToB,
    /// From quay B to quay A.
    BToA,
}

/// A single scheduled sailing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FerrySailing {
    /// Departure time, seconds on the problem's own time base.
    pub dep: Timestamp,
    /// Arrival time, seconds on the problem's own time base.
    pub arr: Timestamp,
}

/// A ferry crossing with quay coordinates already resolved to routing matrix locations.
#[derive(Clone, Debug)]
pub struct FerryCrossing {
    /// Crossing id, carried through from the wire format for diagnostics.
    pub id: String,
    /// Matrix location of quay A.
    pub quay_a: Location,
    /// Matrix location of quay B.
    pub quay_b: Location,
    /// Time on the water: the sailing's crossing duration, seconds.
    pub crossing_sec: Duration,
    /// Time to reserve before a sailing's departure for boarding, seconds.
    pub boarding_buffer_sec: Duration,
    /// Sailings from quay A to quay B, sorted by `dep` ascending. Private so the sort `new`
    /// performs can't be bypassed by a struct literal or a later `push`; read via `sailings`.
    sailings_a_to_b: Vec<FerrySailing>,
    /// Sailings from quay B to quay A, sorted by `dep` ascending. Same reasoning as above.
    sailings_b_to_a: Vec<FerrySailing>,
}

impl FerryCrossing {
    /// Creates a new resolved ferry crossing, sorting each sailing list by departure time.
    ///
    /// # Panics (debug builds only)
    ///
    /// Panics if any sailing has `arr < dep`: an inverted sailing would make a crossing look
    /// free or negative-duration to the cost function.
    pub fn new(
        id: String,
        quay_a: Location,
        quay_b: Location,
        crossing_sec: Duration,
        boarding_buffer_sec: Duration,
        mut sailings_a_to_b: Vec<FerrySailing>,
        mut sailings_b_to_a: Vec<FerrySailing>,
    ) -> Self {
        sailings_a_to_b.sort_by(|a, b| a.dep.total_cmp(&b.dep));
        sailings_b_to_a.sort_by(|a, b| a.dep.total_cmp(&b.dep));

        debug_assert!(sailings_a_to_b.iter().chain(sailings_b_to_a.iter()).all(|sailing| sailing.arr >= sailing.dep));

        Self { id, quay_a, quay_b, crossing_sec, boarding_buffer_sec, sailings_a_to_b, sailings_b_to_a }
    }

    /// Returns the sailing list for the given direction, sorted by departure time.
    pub fn sailings(&self, direction: FerryDirection) -> &[FerrySailing] {
        match direction {
            FerryDirection::AToB => &self.sailings_a_to_b,
            FerryDirection::BToA => &self.sailings_b_to_a,
        }
    }
}

/// Resolved ferry crossings available to the crossing-aware transport cost.
#[derive(Clone, Debug, Default)]
pub struct FerryIndex {
    crossings: Vec<FerryCrossing>,
}

impl FerryIndex {
    /// Creates a new `FerryIndex` from resolved crossings.
    pub fn new(crossings: Vec<FerryCrossing>) -> Self {
        Self { crossings }
    }

    /// Returns true when the problem has no ferry crossings.
    pub fn is_empty(&self) -> bool {
        self.crossings.is_empty()
    }

    /// Returns all resolved crossings.
    pub fn crossings(&self) -> &[FerryCrossing] {
        &self.crossings
    }
}
