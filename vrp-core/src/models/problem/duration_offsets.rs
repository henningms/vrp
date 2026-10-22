//! Duration offsets: delays which depend on the departure time, added to chosen cells of a routing matrix.

#[cfg(test)]
#[path = "../../../tests/unit/models/problem/duration_offsets_test.rs"]
mod duration_offsets_test;

use crate::models::common::{Duration, Timestamp};
use rosomaxa::prelude::{Float, GenericResult};

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
    /// `time + offset` per point: the arrival after a leg without a base duration, which never decreases.
    #[expect(dead_code, reason = "kept for the arrival-keyed lookup")]
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

        let keys = points.iter().map(|&(time, offset)| time + offset).collect();
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
}

/// Returns the value at `x` on the line through two points.
fn interpolate((x0, y0): (Float, Float), (x1, y1): (Float, Float), x: Float) -> Float {
    y0 + (x - x0) * (y1 - y0) / (x1 - x0)
}
