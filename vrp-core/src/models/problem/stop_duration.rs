//! Stop durations: the time a vehicle needs to stop at a location, charged once per arrival from another location.

#[cfg(test)]
#[path = "../../../tests/unit/models/problem/stop_duration_test.rs"]
mod stop_duration_test;

use crate::models::Extras;
use crate::models::common::{Distance, Duration, Location, Profile};
use crate::models::solution::Route;
use std::sync::Arc;

use super::{TransportCost, TravelTime};

/// A stop duration per routing matrix location: the time to pull over once per stop, on top of each job's own duration.
#[derive(Clone, Debug, Default)]
pub struct StopDurations {
    durations: Vec<Duration>,
}

impl StopDurations {
    /// Creates a table from location and duration pairs, keeping the largest duration per location and ignoring
    /// durations which are not positive.
    pub fn new(durations: impl IntoIterator<Item = (Location, Duration)>) -> Self {
        let positive = durations.into_iter().filter(|&(_, duration)| duration > 0.).collect::<Vec<_>>();
        let size = positive.iter().map(|&(location, _)| location + 1).max().unwrap_or(0);

        let mut table: Vec<Duration> = vec![0.; size];
        positive.into_iter().for_each(|(location, duration)| table[location] = table[location].max(duration));

        Self { durations: table }
    }

    /// Returns the stop duration at the location, `0` when it has none.
    pub fn get(&self, location: Location) -> Duration {
        self.durations.get(location).copied().unwrap_or(0.)
    }

    /// Returns the stop duration charged on a leg: none when the vehicle stays at the same location.
    pub fn on_leg(&self, from: Location, to: Location) -> Duration {
        if from == to { 0. } else { self.get(to) }
    }

    /// Returns true when no location has a stop duration.
    pub fn is_empty(&self) -> bool {
        self.durations.is_empty()
    }
}

/// Wraps a transport cost so that the duration of a leg into a location from another one includes the stop duration
/// there. Distance is unchanged.
pub struct StopDurationTransportCost {
    inner: Arc<dyn TransportCost>,
    stop_durations: Arc<StopDurations>,
}

impl StopDurationTransportCost {
    /// Creates a new wrapper around `inner` charging the stop durations from the given table.
    pub fn new(inner: Arc<dyn TransportCost>, stop_durations: Arc<StopDurations>) -> Self {
        Self { inner, stop_durations }
    }
}

/// A negative road duration marks an unreachable leg, so it must reach the caller unchanged.
fn with_stop_duration(road: Duration, stop_duration: Duration) -> Duration {
    if road < 0. { road } else { road + stop_duration }
}

/// An arrival time here is when service can start, while the road leg ends earlier, when the vehicle pulls in.
fn on_road(travel_time: TravelTime, stop_duration: Duration) -> TravelTime {
    match travel_time {
        TravelTime::Arrival(arrival) => TravelTime::Arrival(arrival - stop_duration),
        departure => departure,
    }
}

impl TransportCost for StopDurationTransportCost {
    fn duration_approx(&self, profile: &Profile, from: Location, to: Location) -> Duration {
        let road = self.inner.duration_approx(profile, from, to);

        with_stop_duration(road, self.stop_durations.on_leg(from, to))
    }

    fn distance_approx(&self, profile: &Profile, from: Location, to: Location) -> Distance {
        self.inner.distance_approx(profile, from, to)
    }

    fn duration(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Duration {
        let stop_duration = self.stop_durations.on_leg(from, to);
        let road = self.inner.duration(route, from, to, on_road(travel_time, stop_duration));

        with_stop_duration(road, stop_duration)
    }

    fn distance(&self, route: &Route, from: Location, to: Location, travel_time: TravelTime) -> Distance {
        let stop_duration = self.stop_durations.on_leg(from, to);

        self.inner.distance(route, from, to, on_road(travel_time, stop_duration))
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}

custom_extra_property!(pub StopDurations typeof StopDurations);
