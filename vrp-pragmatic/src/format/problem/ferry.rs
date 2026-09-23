#[cfg(test)]
#[path = "../../../tests/unit/format/problem/ferry_test.rs"]
mod ferry_test;

use crate::format::{CoordIndex, FormatError, Location, MultiFormatError};
use serde::{Deserialize, Serialize};
use vrp_core::models::problem::{FerryCrossing as CoreFerryCrossing, FerryIndex, FerrySailing as CoreFerrySailing};

/// A quay coordinate. Unlike job/vehicle locations, a quay is always a concrete geo-coordinate:
/// it can never be a matrix reference or a custom placeholder, since resolving it into a fresh
/// routing matrix index is the whole point of a crossing.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location2D {
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
}

impl From<&Location2D> for Location {
    fn from(value: &Location2D) -> Self {
        Location::new_coordinate(value.lat, value.lng)
    }
}

/// A single scheduled sailing, seconds on the problem's own time base (the same base as job
/// time windows). The backend converts from wall-clock times; this format never parses a date.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FerrySailing {
    /// Departure time, seconds.
    pub dep: f64,
    /// Arrival time, seconds.
    pub arr: f64,
}

/// Sailings for both directions of a ferry crossing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FerrySailings {
    /// Sailings from quay A to quay B.
    pub a_to_b: Vec<FerrySailing>,
    /// Sailings from quay B to quay A.
    pub b_to_a: Vec<FerrySailing>,
}

/// A ferry crossing: two quays connected by scheduled sailings, letting a route cross open
/// water instead of (or in addition to) the road network.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FerryCrossing {
    /// Unique crossing id.
    pub id: String,
    /// Quay coordinate on one side of the crossing.
    pub quay_a: Location2D,
    /// Quay coordinate on the other side of the crossing.
    pub quay_b: Location2D,
    /// Time on the water: the sailing's crossing duration, seconds.
    pub crossing_sec: f64,
    /// Time to reserve before a sailing's departure for boarding, seconds.
    pub boarding_buffer_sec: f64,
    /// Sailings in both directions.
    pub sailings: FerrySailings,
}

impl FerryCrossing {
    /// Returns quay coordinates as format locations, in the order quay A then quay B — the
    /// order `CoordIndex` walks them in.
    pub(crate) fn quay_locations(&self) -> (Location, Location) {
        (Location::from(&self.quay_a), Location::from(&self.quay_b))
    }
}

/// Resolves `ferryCrossings` wire input into a `FerryIndex`, using matrix indices already
/// assigned by `coord_index`.
///
/// A quay that fails to resolve means the coordinate set and the index disagree - exactly the
/// bug that would silently permute every travel time in the matrix - so this is a hard format
/// error, never a silent skip.
pub fn create_ferry_index(
    crossings: &[FerryCrossing],
    coord_index: &CoordIndex,
) -> Result<FerryIndex, MultiFormatError> {
    let resolve = |crossing_id: &str, location: &Location, label: &str| {
        coord_index.get_by_loc(location).ok_or_else(|| {
            MultiFormatError::from(vec![FormatError::new(
                "E0005".to_string(),
                "cannot resolve ferry crossing quay".to_string(),
                format!(
                    "ensure '{label}' of ferry crossing '{crossing_id}' is present in the problem's coordinate index"
                ),
            )])
        })
    };

    let core_crossings = crossings
        .iter()
        .map(|crossing| {
            let (quay_a_loc, quay_b_loc) = crossing.quay_locations();
            let quay_a = resolve(&crossing.id, &quay_a_loc, "quayA")?;
            let quay_b = resolve(&crossing.id, &quay_b_loc, "quayB")?;

            Ok(CoreFerryCrossing::new(
                crossing.id.clone(),
                quay_a,
                quay_b,
                crossing.crossing_sec,
                crossing.boarding_buffer_sec,
                crossing
                    .sailings
                    .a_to_b
                    .iter()
                    .map(|sailing| CoreFerrySailing { dep: sailing.dep, arr: sailing.arr })
                    .collect(),
                crossing
                    .sailings
                    .b_to_a
                    .iter()
                    .map(|sailing| CoreFerrySailing { dep: sailing.dep, arr: sailing.arr })
                    .collect(),
            ))
        })
        .collect::<Result<Vec<_>, MultiFormatError>>()?;

    Ok(FerryIndex::new(core_crossings))
}
