use crate::models::common::{Distance, Duration, Location, Profile, Timestamp};
use crate::models::problem::{ActivityCost, OffsetProfile, SimpleActivityCost, TransportCost, TravelTime};
use crate::models::solution::{Activity, Route};
use rosomaxa::prelude::Float;
use std::ops::ControlFlow;
use std::sync::Arc;

#[derive(Default)]
pub struct TestTransportCost {}

impl TransportCost for TestTransportCost {
    fn duration_approx(&self, _: &Profile, from: Location, to: Location) -> Duration {
        fake_routing(from, to)
    }

    fn distance_approx(&self, _: &Profile, from: Location, to: Location) -> Distance {
        fake_routing(from, to)
    }

    fn duration(&self, _: &Route, from: Location, to: Location, _: TravelTime) -> Duration {
        fake_routing(from, to)
    }

    fn distance(&self, _: &Route, from: Location, to: Location, _: TravelTime) -> Distance {
        fake_routing(from, to)
    }

    fn size(&self) -> usize {
        1
    }
}

impl TestTransportCost {
    pub fn new_shared() -> Arc<dyn TransportCost + Sync + Send> {
        Arc::new(Self::default())
    }
}

pub fn fake_routing(from: Location, to: Location) -> Float {
    to.abs_diff(from) as Float
}

/// Returns the time of day in seconds since midnight.
pub fn clock(hours: Float, minutes: Float) -> Timestamp {
    hours * 3600. + minutes * 60.
}

/// An eight minute delay from 07:00 to 09:00, building up from 06:52 and clearing by 09:08.
pub fn rush_hour_offset_profile() -> OffsetProfile {
    OffsetProfile::new(vec![(clock(6., 52.), 0.), (clock(7., 0.), 480.), (clock(9., 0.), 480.), (clock(9., 8.), 0.)])
        .unwrap()
}

#[derive(Default)]
pub struct TestActivityCost {
    inner: SimpleActivityCost,
}

impl ActivityCost for TestActivityCost {
    fn estimate_departure(
        &self,
        route: &Route,
        activity: &Activity,
        arrival: Timestamp,
    ) -> ControlFlow<Timestamp, Timestamp> {
        self.inner.estimate_departure(route, activity, arrival)
    }

    fn estimate_arrival(
        &self,
        route: &Route,
        activity: &Activity,
        departure: Timestamp,
    ) -> ControlFlow<Timestamp, Timestamp> {
        self.inner.estimate_arrival(route, activity, departure)
    }
}

impl TestActivityCost {
    pub fn new_shared() -> Arc<dyn ActivityCost + Sync + Send> {
        Arc::new(Self::default())
    }
}
