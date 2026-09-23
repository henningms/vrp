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
/// must be at the boarding quay, the sailing it catches, and the total duration of the journey
/// (approach + wait + crossing + egress) that a caller compares against the road alternative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FerryPath {
    /// Index of the crossing within the `FerryIndex` that produced this path.
    pub crossing_idx: usize,
    /// Direction of travel across the crossing.
    pub direction: FerryDirection,
    /// Time the vehicle must be at the boarding quay, seconds on the problem's time base.
    pub arrive_quay_at: Timestamp,
    /// Departure time of the sailing caught, seconds on the problem's time base.
    pub sailing_dep: Timestamp,
    /// Arrival time of the sailing caught, seconds on the problem's time base. Reporting only:
    /// the duration calculation uses `crossing_sec`, never `sailing_arr - sailing_dep`, so a
    /// timetable oddity on one sailing can't make it look faster than another on the same
    /// crossing.
    pub sailing_arr: Timestamp,
    /// Total duration of the journey from `from` to `to`, including approach, wait, the
    /// crossing itself and egress.
    pub total_duration: Duration,
}

/// Tries every crossing and direction in `index`, calling `select` for each to find the sailing
/// (and the time the vehicle must be at the boarding quay) that leg would use, then keeps the
/// candidate with the smallest total duration. Shared by `best_departure` and `best_arrival`,
/// which differ only in how they pick a sailing from a crossing's timetable.
fn best_path(
    index: &FerryIndex,
    road: &dyn Fn(Location, Location) -> Duration,
    from: Location,
    to: Location,
    select: impl Fn(&FerryCrossing, FerryDirection, Duration, Duration) -> Option<(Timestamp, FerrySailing)>,
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

            // waiting only ever adds to the zero-wait bound, so once a cheaper candidate is
            // found, a crossing whose best possible case can't beat it needs no sailing lookup.
            let zero_wait_bound = approach + crossing.crossing_sec + egress;
            if zero_wait_bound >= best_so_far {
                continue;
            }

            let Some((arrive_quay_at, sailing)) = select(crossing, direction, approach, egress) else { continue };

            let wait = sailing.dep - arrive_quay_at;
            let total_duration = approach + wait + crossing.crossing_sec + egress;
            if total_duration >= best_so_far {
                continue;
            }

            best = Some(FerryPath {
                crossing_idx,
                direction,
                arrive_quay_at,
                sailing_dep: sailing.dep,
                sailing_arr: sailing.arr,
                total_duration,
            });
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
    best_path(index, road, from, to, |crossing, direction, approach, _egress| {
        let at_quay = departure + approach;
        let earliest = at_quay + crossing.boarding_buffer_sec;
        let sailings = crossing.sailings(direction);
        // first sailing departing at or after `earliest`.
        let idx = sailings.partition_point(|sailing| sailing.dep < earliest);
        sailings.get(idx).map(|sailing| (at_quay, *sailing))
    })
}

/// The mirror of `best_departure` for a required arrival: the latest sailing whose crossing (plus
/// egress to `to`) still lands by `arrival`, and the duration of that whole journey. `road` gives
/// the ferries-excluded duration between two locations. Returns `None` when no crossing offers a
/// usable path.
pub fn best_arrival(
    index: &FerryIndex,
    road: &dyn Fn(Location, Location) -> Duration,
    from: Location,
    to: Location,
    arrival: Timestamp,
) -> Option<FerryPath> {
    best_path(index, road, from, to, |crossing, direction, _approach, egress| {
        let threshold = arrival - egress;
        let sailings = crossing.sailings(direction);
        // last sailing whose arrival still meets the deadline: `partition_point` needs arr
        // non-decreasing in the same order as dep, true for any real timetable (a sailing that
        // departs later cannot arrive earlier on the same crossing).
        let idx = sailings.partition_point(|sailing| sailing.arr <= threshold);
        let sailing = *sailings.get(idx.checked_sub(1)?)?;
        // the vehicle need only be at the quay for the boarding buffer, not any earlier.
        let arrive_quay_at = sailing.dep - crossing.boarding_buffer_sec;
        Some((arrive_quay_at, sailing))
    })
}
