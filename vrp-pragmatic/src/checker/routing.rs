#[cfg(test)]
#[path = "../../tests/unit/checker/routing_test.rs"]
mod routing_test;

use super::*;
use crate::format::solution::FerryLegDirection;
use crate::format_time;
use crate::utils::combine_error_results;
use vrp_core::models::common::Timestamp;
use vrp_core::models::problem::{
    FerryAwareTransportCost, FerryDirection, FerryTransport, FerryTransportExtraProperty, TransportCost, TravelTime,
};
use vrp_core::models::solution::Route;
use vrp_core::prelude::GenericResult;

/// Checks that matrix routing information is used properly.
pub fn check_routing(context: &CheckerContext) -> Result<(), Vec<GenericError>> {
    combine_error_results(&[check_routing_rules(context), check_ferry_legs_rules(context)])
}

fn check_routing_rules(context: &CheckerContext) -> GenericResult<()> {
    if context.matrices.as_ref().is_none_or(|m| m.is_empty()) {
        return Ok(());
    }
    let skip_distance_check = skip_distance_check(&context.solution);

    // absent for a problem with no ferry crossings, so a plain matrix-only problem checks exactly
    // as it did before this feature existed - the raw-matrix path below is untouched for it.
    let ferry_transport = context.core_problem.extras.get_ferry_transport();

    context.solution.tours.iter().try_for_each::<_, GenericResult<_>>(|tour| {
        let profile = context.get_vehicle_profile(&tour.vehicle_id)?;

        // `get_matrix_data` re-derives a leg straight from the raw routing matrix, which knows
        // nothing about ferries: a leg the solve crossed by water would be checked against the
        // road-only number and fail. Resolving through the same `FerryAwareTransportCost` the
        // solve used - not `core_problem.transport` wholesale, which may also carry the
        // reserved-time wrapper - keeps this check about routing (ferries included) rather than
        // about reserved-time bookkeeping, which is a separate, already-existing concern this fix
        // does not touch.
        let ferry_aware = ferry_transport
            .as_ref()
            .map(|ferry| -> GenericResult<_> {
                let actor = context.get_actor(tour)?;
                let transport = FerryAwareTransportCost::new(ferry.road.clone(), ferry.index.clone());
                Ok((transport, Route { actor, tour: Default::default() }))
            })
            .transpose()?;

        // `departure` is the fold's running `arrival_time` from just before this leg (the
        // truncated departure this leg actually starts from - see the call sites: for a plain
        // Point-Point leg that's `from`'s own departure, for a Transit-Point leg (resuming after
        // a break) it's the transit stop's departure, not the point stop before the break).
        let get_matrix_data = |from: &PointStop, to: &PointStop, departure: i64| -> GenericResult<(i64, i64)> {
            let from_idx = context.get_location_index(&from.location)?;
            let to_idx = context.get_location_index(&to.location)?;

            if let Some((transport, route)) = &ferry_aware {
                // the solution format only stores whole-second timestamps, so a fractional real
                // departure (e.g. 3.5s, from a non-integer service duration or profile scale)
                // round-trips through `departure` as its truncation - harmless for a road
                // duration, which does not depend on when you leave, but not for a ferry, whose
                // sailing choice is a step function of departure: resolving at the truncated
                // instant can catch a sailing the solver's real, later departure actually missed.
                // Truncation only ever rounds down by less than a second, so the true departure
                // lies in `[departure, departure + 1)`; trying both endpoints and keeping whichever
                // reproduces the reported arrival disambiguates the sailing without having to
                // recover the real fractional departure, which this format cannot represent.
                let expected_arrival = parse_time(&to.time.arrival) as i64;
                let resolve = |departure: Float| {
                    let travel_time = TravelTime::Departure(departure);
                    let distance = transport.distance(route, from_idx, to_idx, travel_time) as i64;
                    let duration = transport.duration(route, from_idx, to_idx, travel_time) as i64;
                    (distance, duration)
                };

                let candidates = [resolve(departure as Float), resolve(departure as Float + 1.)];
                let (distance, duration) = candidates
                    .into_iter()
                    .min_by_key(|&(_, duration)| (departure + duration - expected_arrival).abs())
                    .expect("two candidates");
                return Ok((distance, duration));
            }

            context.get_matrix_data(&profile, from_idx, to_idx)
        };

        let first_stop = tour.stops.first().ok_or_else(|| "empty tour".to_string())?;
        let first_activity =
            first_stop.activities().first().ok_or_else(|| "no activities in first stop".to_string())?;
        let time_offset = parse_time(
            first_activity
                .time
                .as_ref()
                .map(|interval| &interval.end)
                .unwrap_or_else(|| &first_stop.schedule().departure),
        ) as i64;

        let (departure_time, total_distance) = tour.stops.windows(2).enumerate().try_fold::<_, _, GenericResult<_>>(
            (parse_time(&first_stop.schedule().departure) as i64, 0),
            |(arrival_time, total_distance), (leg_idx, stops)| {
                let (from, to) = match stops {
                    [from, to] => (from, to),
                    _ => unreachable!(),
                };

                let (distance, duration, to_distance) = match (from, to) {
                    (Stop::Point(from), Stop::Point(to)) => {
                        let (distance, duration) = get_matrix_data(from, to, arrival_time)?;
                        (distance, duration, to.distance)
                    }
                    (prev, Stop::Transit(transit)) => {
                        let prev_departure = parse_time(&prev.schedule().departure);
                        let next_arrival = parse_time(&transit.time.arrival);
                        // NOTE an edge case: duration of break will be counted in transit stop
                        let duration = if next_arrival == prev_departure {
                            0.
                        } else {
                            parse_time(&transit.time.departure) - next_arrival
                        };
                        (0_i64, duration as i64, total_distance)
                    }
                    (Stop::Transit(_), Stop::Point(to)) => {
                        assert!(leg_idx > 0);
                        let from = tour
                            .stops
                            .get(leg_idx - 1)
                            .unwrap()
                            .as_point()
                            .expect("two consistent transit stops are not supported");
                        let (distance, duration) = get_matrix_data(from, to, arrival_time)?;
                        (distance, duration, to.distance)
                    }
                };

                let arrival_time = arrival_time + duration;
                let total_distance = total_distance + distance;

                check_stop_statistic(
                    arrival_time,
                    total_distance,
                    to.schedule(),
                    to_distance,
                    leg_idx + 1,
                    tour,
                    skip_distance_check,
                )?;

                Ok((parse_time(&to.schedule().departure) as i64, to_distance))
            },
        )?;

        check_tour_statistic(departure_time, total_distance, time_offset, tour, skip_distance_check)
    })?;

    check_solution_statistic(&context.solution)
}

/// Reconciles `ferryLegs` in both directions instead of only checking that whatever is present is
/// plausible: every tour leg whose elapsed time the road alone cannot explain must carry a
/// matching entry (this is what actually catches a leg the writer silently dropped - a weaker
/// "every present entry looks fine" rule would accept a missing one for free), and every entry
/// present is checked against the crossing's own timetable and the drives on either side of it,
/// including any required-break dwell spliced in as its own transit stop between the two point
/// stops a leg names.
///
/// Never indexes a slice with a stop index taken from the solution document (`fromStopIndex`/
/// `toStopIndex`) - only `.get()` - because this runs from the standalone checker CLI on a
/// user-supplied file: an out-of-order or out-of-range pair must produce an error, not a panic.
fn check_ferry_legs_rules(context: &CheckerContext) -> GenericResult<()> {
    let ferry_transport = context.core_problem.extras.get_ferry_transport();

    let Some(ferry_transport) = ferry_transport else {
        // a problem with no crossings can never produce a leg to report.
        return if context.solution.ferry_legs.is_some() {
            Err("solution reports ferryLegs but the problem has no ferry crossings".into())
        } else {
            Ok(())
        };
    };

    let road = ferry_transport.road.as_ref();
    let ferry_legs = context.solution.ferry_legs.as_deref().unwrap_or(&[]);

    // every entry a tour's own point-stop walk below matches gets removed; anything left over
    // names a stop-index pair that is not actually an adjacent point-stop pair in any tour.
    let mut unmatched: HashSet<usize> = (0..ferry_legs.len()).collect();

    context.solution.tours.iter().try_for_each(|tour| -> GenericResult<()> {
        let actor = context.get_actor(tour)?;
        let route = Route { actor, tour: Default::default() };

        // mirrors the writer's own point-stops-only walk (skipping transit/break stops), so a
        // pair here is exactly a pair the writer could have reported a `FerryLeg` for.
        let point_stops: Vec<(usize, &PointStop)> = tour
            .stops
            .iter()
            .enumerate()
            .filter_map(|(idx, stop)| stop.as_point().map(|point| (idx, point)))
            .collect();

        point_stops.windows(2).try_for_each(|pair| -> GenericResult<()> {
            let (from_stop_index, from) = pair[0];
            let (to_stop_index, to) = pair[1];

            let matched = ferry_legs.iter().enumerate().find(|(_, leg)| {
                leg.vehicle_id == tour.vehicle_id
                    && leg.shift_index == tour.shift_index
                    && leg.from_stop_index == from_stop_index
                    && leg.to_stop_index == to_stop_index
            });

            let from_idx = context.get_location_index(&from.location)?;
            let to_idx = context.get_location_index(&to.location)?;
            // NOTE `break_writer`'s `TransitBreakMoved` case (a break whose window starts before
            // the leg does) extends an activity's own `Interval` at the previous stop without
            // updating that stop's `PointStop.time.departure`, so in principle this re-walk could
            // resolve at a departure the vehicle never actually left at. Unguarded here: the
            // existing (pre-ferry, break-unrelated) gap in `check_routing_rules`'s own leg fold
            // already rejects any solution with a required break landing on a leg with real
            // travel before it, so this case cannot currently reach a passing `check()` for this
            // rule to be exercised against. If that other gap is ever fixed, this comment is the
            // reminder to revisit whether `from_departure` still holds.
            let from_departure = parse_time(&from.time.departure);
            let to_arrival = parse_time(&to.time.arrival);

            // any stop strictly between the two point stops is a transit (required break) stop
            // spliced in by the writer - its own dwell (departure minus arrival) is real elapsed
            // time the next point stop's arrival must still include. `.get` on the range rather
            // than `[..]`: `from_stop_index`/`to_stop_index` here are always increasing (from our
            // own `windows(2)` walk), but this keeps the same safe pattern as the lookup below.
            let break_dwell: Duration = tour
                .stops
                .get(from_stop_index + 1..to_stop_index)
                .map(|between| {
                    between
                        .iter()
                        .map(|stop| parse_time(&stop.schedule().departure) - parse_time(&stop.schedule().arrival))
                        .sum()
                })
                .unwrap_or(0.);

            match matched {
                None => {
                    let road_duration =
                        road.duration(&route, from_idx, to_idx, TravelTime::Departure(from_departure));
                    let actual_elapsed = to_arrival - from_departure - break_dwell;
                    if (actual_elapsed - road_duration).abs() > 1. {
                        return Err(format!(
                            "tour '{}' leg {from_stop_index}->{to_stop_index} elapsed {actual_elapsed}s, which the \
                             road alone ({road_duration}s) cannot explain, but no ferryLeg was reported for it",
                            tour.vehicle_id
                        )
                        .into());
                    }
                    Ok(())
                }
                Some((leg_idx, leg)) => {
                    unmatched.remove(&leg_idx);
                    check_one_ferry_leg(&ferry_transport, road, &route, from_idx, to_idx, from_departure, to_arrival, break_dwell, leg)
                }
            }
        })
    })?;

    if let Some(&leg_idx) = unmatched.iter().next() {
        let leg = &ferry_legs[leg_idx];
        return Err(format!(
            "ferryLeg for vehicle '{}' names stop indices {}->{} that are not an adjacent point-stop pair in that tour",
            leg.vehicle_id, leg.from_stop_index, leg.to_stop_index
        )
        .into());
    }

    Ok(())
}

/// Validates one already-matched `FerryLeg` against its crossing's own timetable and the drives on
/// either side of it.
#[allow(clippy::too_many_arguments)]
fn check_one_ferry_leg(
    ferry_transport: &FerryTransport,
    road: &dyn TransportCost,
    route: &Route,
    from_idx: usize,
    to_idx: usize,
    from_departure: Timestamp,
    to_arrival: Timestamp,
    break_dwell: Duration,
    leg: &FerryLeg,
) -> GenericResult<()> {
    let crossing = ferry_transport
        .index
        .crossings()
        .iter()
        .find(|crossing| crossing.id == leg.crossing_id)
        .ok_or_else(|| GenericError::from(format!("ferryLeg references unknown crossing '{}'", leg.crossing_id)))?;

    let (direction, board_quay, disembark_quay) = match leg.direction {
        FerryLegDirection::AToB => (FerryDirection::AToB, crossing.quay_a, crossing.quay_b),
        FerryLegDirection::BToA => (FerryDirection::BToA, crossing.quay_b, crossing.quay_a),
    };

    let expected_quay_arrival =
        from_departure + road.duration(route, from_idx, board_quay, TravelTime::Departure(from_departure));
    if (expected_quay_arrival - leg.arrive_quay_at).abs() > 1. {
        return Err(format!(
            "ferryLeg '{}' quay arrival mismatch: expected ~{expected_quay_arrival}, got {}",
            leg.crossing_id, leg.arrive_quay_at
        )
        .into());
    }

    // the reported sailing must be a real one on this crossing/direction - nothing else ties
    // `sailingDeparture`/`sailingArrival` back to the timetable, so an invented or already-departed
    // sailing would otherwise pass unnoticed.
    let sailing_exists = crossing.sailings(direction).iter().any(|sailing| {
        (sailing.dep - leg.sailing_departure).abs() < 1e-6 && (sailing.arr - leg.sailing_arrival).abs() < 1e-6
    });
    if !sailing_exists {
        return Err(format!(
            "ferryLeg '{}' reports a sailing (dep {}, arr {}) that is not in the crossing's {:?} timetable",
            leg.crossing_id, leg.sailing_departure, leg.sailing_arrival, leg.direction
        )
        .into());
    }

    // the single most important property of the whole feature: the vehicle must actually be able
    // to board the sailing reported.
    if leg.sailing_departure + 1e-6 < leg.arrive_quay_at + crossing.boarding_buffer_sec {
        return Err(format!(
            "ferryLeg '{}' sailing departs at {} before the vehicle can board (quay arrival {} + buffer {})",
            leg.crossing_id, leg.sailing_departure, leg.arrive_quay_at, crossing.boarding_buffer_sec
        )
        .into());
    }

    // the cost model charges `crossing.crossing_sec`, not the sailing's own `arr - dep`: a
    // sailing's span differs from the crossing duration whenever the crossing duration comes from
    // a different sailing (or from map data) than the one actually caught - routine in production
    // timetables, not an edge case - so the expected disembark time is derived from
    // `sailing_departure + crossing_sec`, never from `sailing_arrival`.
    let disembark_at = leg.sailing_departure + crossing.crossing_sec;
    let expected_to_arrival =
        disembark_at + road.duration(route, disembark_quay, to_idx, TravelTime::Departure(disembark_at)) + break_dwell;
    if (expected_to_arrival - to_arrival).abs() > 1. {
        return Err(format!(
            "ferryLeg '{}' next stop arrival mismatch: expected ~{expected_to_arrival}, got {to_arrival}",
            leg.crossing_id
        )
        .into());
    }

    Ok(())
}

fn check_stop_statistic(
    arrival_time: i64,
    total_distance: i64,
    schedule: &Schedule,
    distance: i64,
    stop_idx: usize,
    tour: &Tour,
    skip_distance_check: bool,
) -> GenericResult<()> {
    #![allow(clippy::unnecessary_cast)]
    if (arrival_time - parse_time(&schedule.arrival) as i64).abs() > 1 {
        return Err(format!(
            "arrival time mismatch for {stop_idx} stop in the tour: {}, expected: '{}', got: '{}'",
            tour.vehicle_id,
            format_time(arrival_time as Float),
            schedule.arrival
        )
        .into());
    }

    if !skip_distance_check && (total_distance - distance).abs() > 1 {
        return Err(format!(
            "distance mismatch for {stop_idx} stop in the tour: {}, expected: '{total_distance}', got: '{distance}'",
            tour.vehicle_id
        )
        .into());
    }

    Ok(())
}

fn check_tour_statistic(
    departure_time: i64,
    total_distance: i64,
    time_offset: i64,
    tour: &Tour,
    skip_distance_check: bool,
) -> GenericResult<()> {
    if !skip_distance_check && (total_distance - tour.statistic.distance).abs() > 1 {
        return Err(format!(
            "distance mismatch for tour statistic: {}, expected: '{}', got: '{}'",
            tour.vehicle_id, total_distance, tour.statistic.distance,
        )
        .into());
    }

    let total_duration = departure_time - time_offset;
    if (total_duration - tour.statistic.duration).abs() > 1 {
        return Err(format!(
            "duration mismatch for tour statistic: {}, expected: '{}', got: '{}'",
            tour.vehicle_id, total_duration, tour.statistic.duration,
        )
        .into());
    }

    Ok(())
}

fn check_solution_statistic(solution: &Solution) -> GenericResult<()> {
    let statistic = solution.tours.iter().fold(Statistic::default(), |acc, tour| acc + tour.statistic.clone());

    // NOTE cost should be ignored due to floating point issues
    if statistic.duration != solution.statistic.duration || statistic.distance != solution.statistic.distance {
        Err(format!("solution statistic mismatch, expected: '{:?}', got: '{:?}'", statistic, solution.statistic).into())
    } else {
        Ok(())
    }
}

/// A workaround method for hre format output where distance is not defined.
fn skip_distance_check(solution: &Solution) -> bool {
    let skip_distance_check = solution
        .tours
        .iter()
        .flat_map(|tour| tour.stops.iter())
        .filter_map(|stop| stop.as_point())
        .all(|stop| stop.distance == 0);

    if skip_distance_check {
        // TODO use logging lib instead of println
        println!("all stop distances are zeros: no distance check will be performed");
    }

    skip_distance_check
}
