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

/// A resolved path across one ferry crossing: which crossing and direction, when the vehicle
/// leaves `from` and must be at the boarding quay, the sailing it catches, and the total
/// duration a caller compares against the road alternative.
///
/// For `best_departure`, `total_duration` is the journey's own length (approach + wait +
/// crossing + egress) and `depart_at` echoes the given `departure`. For `best_arrival`,
/// `total_duration` is `arrival - depart_at`: the two are not computed the same way, so a
/// caller must not assume a value returned by one variant means the same thing on the other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FerryPath {
    /// Index of the crossing within the `FerryIndex` that produced this path.
    pub crossing_idx: usize,
    /// Direction of travel across the crossing.
    pub direction: FerryDirection,
    /// Time the vehicle leaves `from`: the given `departure` for `best_departure`, the latest
    /// feasible departure for `best_arrival`.
    pub depart_at: Timestamp,
    /// Time the vehicle must be at the boarding quay, seconds on the problem's time base.
    pub arrive_quay_at: Timestamp,
    /// Departure time of the sailing caught, seconds on the problem's time base.
    pub sailing_dep: Timestamp,
    /// Arrival time of the sailing caught, seconds on the problem's time base. Reporting only:
    /// the duration calculation uses `crossing_sec`, never `sailing_arr - sailing_dep`, so a
    /// timetable oddity on one sailing can't make it look faster than another on the same
    /// crossing.
    pub sailing_arr: Timestamp,
    /// Total duration a caller compares against the road alternative; see the struct doc for
    /// how it differs between the two functions that produce a `FerryPath`.
    pub total_duration: Duration,
}

/// Tries every crossing and direction in `index`, calling `select` for each to build the
/// candidate that leg would produce (`select` computes its own `total_duration` and applies its
/// own lower-bound prune against `best_so_far`, since the two callers define "total" - and so
/// what bounds it - differently), then keeps the candidate with the smallest total duration.
/// Shared by `best_departure` and `best_arrival`, which differ only in how they pick a sailing
/// and in what a "total" means for that direction of query.
fn best_path(
    index: &FerryIndex,
    road: &dyn Fn(Location, Location) -> Duration,
    from: Location,
    to: Location,
    select: impl Fn(&FerryCrossing, FerryDirection, Duration, Duration, Duration) -> Option<FerryPath>,
) -> Option<FerryPath> {
    let mut best: Option<FerryPath> = None;

    for (crossing_idx, crossing) in index.crossings().iter().enumerate() {
        for direction in [FerryDirection::AToB, FerryDirection::BToA] {
            let (from_quay, to_quay) = match direction {
                FerryDirection::AToB => (crossing.quay_a, crossing.quay_b),
                FerryDirection::BToA => (crossing.quay_b, crossing.quay_a),
            };

            // quays resolving to the same location would be a zero-length "crossing" the
            // solver could take for free; nothing upstream validates against this, so skip it
            // here rather than return a nonsense path.
            if from_quay == to_quay {
                continue;
            }

            let approach = road(from, from_quay);
            let egress = road(to_quay, to);
            if approach >= UNREACHABLE_DURATION_THRESHOLD || egress >= UNREACHABLE_DURATION_THRESHOLD {
                continue;
            }

            let best_so_far = best.map(|path| path.total_duration).unwrap_or(Duration::INFINITY);

            let Some(candidate) = select(crossing, direction, approach, egress, best_so_far) else { continue };

            // a variant's own prune only bounds total_duration from below, so a candidate that
            // cleared it can still end up worse than the current best once its actual total is
            // known; only accept it if it truly is better.
            if candidate.total_duration >= best_so_far {
                continue;
            }

            best = Some(FerryPath { crossing_idx, ..candidate });
        }
    }

    best
}

/// Best duration from `from` to `to` departing at `departure`, considering every crossing in
/// `index`. `road` gives the ferries-excluded duration between two locations. Returns `None`
/// when no crossing offers a usable path: every sailing already departed, or a quay is
/// unreachable by road.
pub fn best_departure(
    index: &FerryIndex,
    road: &dyn Fn(Location, Location) -> Duration,
    from: Location,
    to: Location,
    departure: Timestamp,
) -> Option<FerryPath> {
    best_path(index, road, from, to, |crossing, direction, approach, egress, best_so_far| {
        // waiting only ever adds to the zero-wait bound, so a crossing whose best possible case
        // (no wait at all) can't beat what's already found needs no sailing lookup at all.
        let zero_wait_bound = approach + crossing.crossing_sec + egress;
        if zero_wait_bound >= best_so_far {
            return None;
        }

        let at_quay = departure + approach;
        let earliest = at_quay + crossing.boarding_buffer_sec;
        let sailings = crossing.sailings(direction);
        // first sailing departing at or after `earliest`.
        let idx = sailings.partition_point(|sailing| sailing.dep < earliest);
        let sailing = *sailings.get(idx)?;

        let wait = sailing.dep - at_quay;
        let total_duration = approach + wait + crossing.crossing_sec + egress;

        Some(FerryPath {
            crossing_idx: 0, // overwritten by best_path once a candidate is accepted
            direction,
            depart_at: departure,
            arrive_quay_at: at_quay,
            sailing_dep: sailing.dep,
            sailing_arr: sailing.arr,
            total_duration,
        })
    })
}

/// The mirror of `best_departure` for a required arrival: the crossing and sailing that let the
/// vehicle leave `from` as late as possible while still reaching `to` by `arrival`. `road` gives
/// the ferries-excluded duration between two locations. Returns `None` when no crossing offers a
/// usable path.
///
/// This does not minimise journey length: a short crossing with one sailing an hour away can
/// force an earlier departure than a longer crossing sailing every few minutes, so `total_duration`
/// here is `arrival - depart_at` and minimising it is exactly maximising the latest feasible
/// departure — the quantity that actually matters for a deadline.
pub fn best_arrival(
    index: &FerryIndex,
    road: &dyn Fn(Location, Location) -> Duration,
    from: Location,
    to: Location,
    arrival: Timestamp,
) -> Option<FerryPath> {
    best_path(index, road, from, to, |crossing, direction, approach, egress, best_so_far| {
        // a lower bound on total_duration = arrival - depart_at: every sailing has arr >= dep
        // (enforced on construction) and a feasible one has arr + egress <= arrival, so
        // dep <= arrival - egress, and depart_at = dep - buffer - approach follows the same
        // bound. crossing_sec plays no part in this variant's total, so - unlike the departure
        // side - it cannot be used here; a crossing whose best possible case can't beat what's
        // already found needs no sailing lookup at all.
        let zero_wait_bound = approach + egress + crossing.boarding_buffer_sec;
        if zero_wait_bound >= best_so_far {
            return None;
        }

        let sailings = crossing.sailings(direction);
        // sailings are sorted by dep ascending, but real timetables aren't always monotone in
        // arr too (a later departure can arrive earlier), so a binary search on arr cannot be
        // trusted; scanning from the end in descending-dep order and taking the first sailing
        // that meets the deadline is exact regardless, and is cheap given a crossing carries at
        // most a few dozen sailings a day per direction.
        let sailing = sailings.iter().rev().find(|sailing| sailing.arr + egress <= arrival)?;

        // the vehicle need only be at the quay for the boarding buffer, not any earlier; using
        // the buffer directly (rather than re-deriving it by subtracting arrive_quay_at back
        // out of sailing.dep) avoids a double float subtraction that can drift by a couple of
        // ULPs and silently break the deadline-monotonicity property below.
        let arrive_quay_at = sailing.dep - crossing.boarding_buffer_sec;
        let depart_at = arrive_quay_at - approach;
        let total_duration = arrival - depart_at;

        Some(FerryPath {
            crossing_idx: 0, // overwritten by best_path once a candidate is accepted
            direction,
            depart_at,
            arrive_quay_at,
            sailing_dep: sailing.dep,
            sailing_arr: sailing.arr,
            total_duration,
        })
    })
}
