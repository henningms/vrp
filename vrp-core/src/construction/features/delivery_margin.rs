//! Scores the shortfall between the requested delivery margin and the remaining time window.
#[cfg(test)]
#[path = "../../../tests/unit/construction/features/delivery_margin_test.rs"]
mod delivery_margin_test;

use super::*;
use crate::construction::enablers::schedule_after;
use crate::models::solution::Activity;

/// Creates a soft objective measured in seconds of missing margin per rider delivery.
pub fn create_delivery_margin_feature(
    name: &str,
    target_margin: Duration,
    transport: Arc<dyn TransportCost>,
    activity: Arc<dyn ActivityCost>,
) -> GenericResult<Feature> {
    if !target_margin.is_finite() || target_margin < 0. {
        return Err("delivery target margin must be a finite nonnegative duration in seconds".into());
    }
    FeatureBuilder::default()
        .with_name(name)
        .with_objective(DeliveryMargin { target_margin, transport, activity })
        .build()
}

struct DeliveryMargin {
    target_margin: Duration,
    transport: Arc<dyn TransportCost>,
    activity: Arc<dyn ActivityCost>,
}

impl DeliveryMargin {
    fn shortfall(&self, a: &Activity, arrival: Timestamp) -> Cost {
        let Some(single) = &a.job else { return 0. };
        if !Multi::is_child(single)
            || single.dimens.get_job_outside_ride_limit().copied().unwrap_or(false)
            || !super::ride_duration::is_delivery(single)
        {
            return 0.;
        }
        let service_start = arrival.max(a.place.time.start);
        (self.target_margin - (a.place.time.end - service_start)).max(0.)
    }
}

impl FeatureObjective for DeliveryMargin {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        solution
            .solution
            .routes
            .iter()
            .flat_map(|rc| rc.route().tour.all_activities())
            .map(|a| self.shortfall(a, a.schedule.arrival))
            .sum()
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        let MoveContext::Activity { route_ctx, activity_ctx, .. } = move_ctx else { return 0. };
        let route = route_ctx.route();
        let project = |previous, a| schedule_after(route, self.activity.as_ref(), self.transport.as_ref(), previous, a);
        let target = activity_ctx.target;
        let prev = activity_ctx.prev;
        let mut schedule = project((prev.place.location, prev.schedule.departure), target);
        let mut delta = self.shortfall(target, schedule.arrival);
        let mut location = target.place.location;
        for a in route.tour.all_activities().skip(activity_ctx.index + 1) {
            schedule = project((location, schedule.departure), a);
            delta += self.shortfall(a, schedule.arrival) - self.shortfall(a, a.schedule.arrival);
            // Once a window absorbs the delay, all subsequent legs leave at their old time.
            if schedule.departure == a.schedule.departure {
                break;
            }
            location = a.place.location;
        }
        delta
    }
}
