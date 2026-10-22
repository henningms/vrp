//! Duration offsets: delays which depend on the departure time, added to chosen cells of a routing matrix.

#[cfg(test)]
#[path = "../../../tests/unit/models/problem/duration_offsets_test.rs"]
mod duration_offsets_test;

use crate::models::Extras;
use crate::models::common::{Distance, Duration, Location, Profile, Timestamp};
use crate::models::solution::Route;
use rosomaxa::prelude::{Float, GenericResult};
use std::sync::Arc;

use super::{TransportCost, TravelTime, UNREACHABLE_DURATION_THRESHOLD};

/// An extra travel time which depends on the departure time: linear between its points, and held before the first
/// point and after the last. The offset is added to a leg's duration as it is: the vehicle profile scale does not
/// apply to it.
///
/// Offsets are never negative and fall at most one second per second of departure, so that leaving later never
/// arrives earlier.
#[derive(Clone, Debug)]
pub struct OffsetProfile {
    times: Vec<Timestamp>,
    offsets: Vec<Duration>,
    /// `time + offset` per point: the arrival after a leg without a base duration. A running maximum, so that rounding
    /// never puts a key below its predecessor and the keys stay sorted.
    keys: Vec<Timestamp>,
}

impl OffsetProfile {
    /// Creates a profile from `(departure time, offset)` points with strictly increasing times. Refuses an empty
    /// list, a time or an offset which is not finite, a negative offset, and a profile which falls faster than one
    /// second per second.
    pub fn new(points: Vec<(Timestamp, Duration)>) -> GenericResult<Self> {
        if points.is_empty() {
            return Err("duration offset profile has no points".into());
        }

        if let Some(&(time, _)) = points.iter().find(|(time, _)| !time.is_finite()) {
            return Err(format!("duration offset profile has a time which is not finite: {time}").into());
        }

        if let Some(&(time, offset)) = points.iter().find(|(_, offset)| !offset.is_finite() || *offset < 0.) {
            return Err(format!("duration offset profile has an invalid offset {offset} at {time}").into());
        }

        for pair in points.windows(2) {
            let ((time, offset), (next_time, next_offset)) = (pair[0], pair[1]);

            if next_time <= time {
                return Err(format!("duration offset profile times do not increase: {next_time} after {time}").into());
            }

            if next_offset - offset < -(next_time - time) {
                return Err(
                    format!("duration offset profile falls faster than time between {time} and {next_time}").into()
                );
            }
        }

        let keys = points
            .iter()
            .scan(Float::NEG_INFINITY, |key, &(time, offset)| {
                *key = key.max(time + offset);
                Some(*key)
            })
            .collect();
        let (times, offsets) = points.into_iter().unzip();

        Ok(Self { times, offsets, keys })
    }

    /// Returns the offset for a departure at the given time.
    pub fn at(&self, departure: Timestamp) -> Duration {
        let last = self.times.len() - 1;

        if departure <= self.times[0] {
            return self.offsets[0];
        }

        if departure >= self.times[last] {
            return self.offsets[last];
        }

        let i = self.times.partition_point(|&time| time <= departure) - 1;

        interpolate((self.times[i], self.offsets[i]), (self.times[i + 1], self.offsets[i + 1]), departure)
    }

    /// Returns the latest departure which arrives by `target` after a leg without a base duration: the largest `t`
    /// with `t + at(t) <= target`. For a leg with a base duration, pass `target = arrival - base`.
    ///
    /// The inverse is exact because `t + at(t)` is continuous, piecewise linear and non-decreasing.
    pub fn latest_departure(&self, target: Timestamp) -> Timestamp {
        let last = self.keys.len() - 1;

        if target < self.keys[0] {
            return target - self.offsets[0];
        }

        if target >= self.keys[last] {
            return target - self.offsets[last];
        }

        // the last key at or below the target resolves a stretch of equal arrivals to its latest departure, and
        // leaves `keys[i + 1] > keys[i]`, so the segment is never empty
        let i = self.keys.partition_point(|&key| key <= target) - 1;

        interpolate((self.keys[i], self.times[i]), (self.keys[i + 1], self.times[i + 1]), target)
    }
}

/// Returns the value at `x` on the line through two points.
fn interpolate((x0, y0): (Float, Float), (x1, y1): (Float, Float), x: Float) -> Float {
    y0 + (x - x0) * (y1 - y0) / (x1 - x0)
}

/// Offset profiles per routing profile, and per matrix cell the profile which applies to the leg: `0` for none, `k`
/// for the `k`-th profile.
#[derive(Clone, Debug)]
pub struct DurationOffsets {
    /// The number of locations in the routing matrix.
    side: usize,
    /// Per routing profile index: the cells, row by row (`from * side + to`), and the profiles they name.
    tables: Vec<Option<(Vec<u16>, Vec<OffsetProfile>)>>,
}

impl DurationOffsets {
    /// Creates offsets for a routing matrix of `side` locations from `(routing profile index, cells, profiles)`
    /// tables. Refuses a table whose cells do not cover the matrix or name a profile it does not have, and a second
    /// table for a routing profile. A table whose cells are all `0` applies nowhere and is dropped, after its shape is
    /// checked.
    pub fn new(
        side: usize,
        matrices: impl IntoIterator<Item = (usize, Vec<u16>, Vec<OffsetProfile>)>,
    ) -> GenericResult<Self> {
        let mut tables: Vec<Option<(Vec<u16>, Vec<OffsetProfile>)>> = Vec::new();

        for (index, cells, profiles) in matrices {
            if cells.len() != side * side {
                return Err(format!(
                    "duration offsets of routing profile {index} have {} cells, expected {}",
                    cells.len(),
                    side * side
                )
                .into());
            }

            if let Some((position, &cell)) = cells.iter().enumerate().find(|&(_, &cell)| cell as usize > profiles.len())
            {
                return Err(format!(
                    "duration offsets of routing profile {index} name offset profile {cell} at cell {position}, \
                     but have {} profiles",
                    profiles.len()
                )
                .into());
            }

            if cells.iter().all(|&cell| cell == 0) {
                continue;
            }

            if tables.len() <= index {
                tables.resize_with(index + 1, || None);
            }

            if tables[index].is_some() {
                return Err(format!("duration offsets of routing profile {index} are given more than once").into());
            }

            tables[index] = Some((cells, profiles));
        }

        Ok(Self { side, tables })
    }

    /// Returns the offset profile of the leg `from -> to` for the routing profile, if one applies.
    pub fn get(&self, profile_index: usize, from: Location, to: Location) -> Option<&OffsetProfile> {
        if from >= self.side || to >= self.side {
            return None;
        }

        let (cells, profiles) = self.tables.get(profile_index)?.as_ref()?;

        match cells[from * self.side + to] {
            0 => None,
            cell => Some(&profiles[cell as usize - 1]),
        }
    }

    /// Returns the cells of the routing profile, if it has offsets.
    pub fn cells(&self, profile_index: usize) -> Option<&[u16]> {
        self.tables.get(profile_index)?.as_ref().map(|(cells, _)| cells.as_slice())
    }

    /// Returns true when no cell of any routing profile has an offset.
    pub fn is_empty(&self) -> bool {
        self.tables.iter().all(Option::is_none)
    }
}

/// Wraps a transport cost so that a leg whose cell has an offset profile takes its duration plus the offset at its
/// departure time. Distances and the time-independent estimates are unchanged.
///
/// `inner` must not depend on time: an arrival-keyed lookup inverts the offset around the duration `inner` gives at
/// the arrival time, as if that duration held at every departure.
pub struct DurationOffsetTransportCost {
    inner: Arc<dyn TransportCost>,
    offsets: Arc<DurationOffsets>,
}

impl DurationOffsetTransportCost {
    /// Creates a new wrapper around `inner` adding the given offsets.
    pub fn new(inner: Arc<dyn TransportCost>, offsets: Arc<DurationOffsets>) -> Self {
        Self { inner, offsets }
    }
}

impl TransportCost for DurationOffsetTransportCost {
    fn duration_approx(&self, profile: &Profile, from: Location, to: Location) -> Duration {
        self.inner.duration_approx(profile, from, to)
    }

    fn distance_approx(&self, profile: &Profile, from: Location, to: Location) -> Distance {
        self.inner.distance_approx(profile, from, to)
    }

    fn duration(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Duration {
        let base = self.inner.duration(route, from, to, travel_time);

        // the unreachable markers must reach the caller unchanged
        if base < 0. || base >= UNREACHABLE_DURATION_THRESHOLD {
            return base;
        }

        let Some(profile) = self.offsets.get(route.actor.vehicle.profile.index, from, to) else { return base };

        match travel_time {
            TravelTime::Departure(departure) => base + profile.at(departure),
            // an unbounded arrival has an unbounded latest departure, and the difference of the two cancels to 0
            TravelTime::Arrival(arrival) if !arrival.is_finite() || arrival == Float::MAX => base,
            TravelTime::Arrival(arrival) => arrival - profile.latest_departure(arrival - base),
        }
    }

    fn distance(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Distance {
        self.inner.distance(route, from, to, travel_time)
    }

    fn size(&self) -> usize {
        self.inner.size()
    }

    fn has_duration_offsets(&self) -> bool {
        true
    }
}

custom_extra_property!(pub DurationOffsets typeof DurationOffsets);
