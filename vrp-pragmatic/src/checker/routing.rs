#[cfg(test)]
#[path = "../../tests/unit/checker/routing_test.rs"]
mod routing_test;

use super::*;
use crate::format::solution::FerryLegDirection;
use crate::format_time;
use crate::utils::combine_error_results;
use vrp_core::models::problem::{FerryAwareTransportCost, FerryTransportExtraProperty, TransportCost, TravelTime};
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

/// Reconciles every reported `FerryLeg` against the stops it names, independently of the
/// leg-by-leg fold above: the quay arrival must follow from the previous stop's departure plus
/// the drive to the quay, and the next stop's arrival must follow from the sailing's arrival plus
/// the drive from the far quay (plus any required-break dwell spliced in between as its own
/// transit stop - a leg the writer correctly reports across a break still elapses that break's
/// time before the next point stop is reached). This is what actually proves `ferryLegs`
/// describes the solve rather than merely being present: a wrong stop index, a road leg reported
/// as a crossing (or the reverse), an inverted direction, or a departure anchor that resolved a
/// different sailing than the one actually caught would all fail here even though nothing above
/// re-derives a leg's own duration/distance from `ferryLegs` at all.
fn check_ferry_legs_rules(context: &CheckerContext) -> GenericResult<()> {
    let ferry_transport = context.core_problem.extras.get_ferry_transport();

    let (Some(ferry_transport), Some(ferry_legs)) = (&ferry_transport, context.solution.ferry_legs.as_ref()) else {
        // a problem with no crossings can never produce a leg to report; the reverse (crossings
        // exist, nothing reported) is legitimate whenever no leg actually won against the road.
        return if ferry_transport.is_none() && context.solution.ferry_legs.is_some() {
            Err("solution reports ferryLegs but the problem has no ferry crossings".into())
        } else {
            Ok(())
        };
    };

    let road = ferry_transport.road.as_ref();

    ferry_legs.iter().try_for_each(|leg| -> GenericResult<()> {
        let tour = context
            .solution
            .tours
            .iter()
            .find(|tour| tour.vehicle_id == leg.vehicle_id && tour.shift_index == leg.shift_index)
            .ok_or_else(|| {
                GenericError::from(format!(
                    "ferryLeg references unknown vehicle/shift: {}/{}",
                    leg.vehicle_id, leg.shift_index
                ))
            })?;

        let from = tour.stops.get(leg.from_stop_index).and_then(|stop| stop.as_point()).ok_or_else(|| {
            GenericError::from(format!("ferryLeg fromStopIndex {} is not a point stop", leg.from_stop_index))
        })?;
        let to = tour.stops.get(leg.to_stop_index).and_then(|stop| stop.as_point()).ok_or_else(|| {
            GenericError::from(format!("ferryLeg toStopIndex {} is not a point stop", leg.to_stop_index))
        })?;

        let crossing = ferry_transport.index.crossings().iter().find(|crossing| crossing.id == leg.crossing_id).ok_or_else(
            || GenericError::from(format!("ferryLeg references unknown crossing '{}'", leg.crossing_id)),
        )?;

        let (board_quay, disembark_quay) = match leg.direction {
            FerryLegDirection::AToB => (crossing.quay_a, crossing.quay_b),
            FerryLegDirection::BToA => (crossing.quay_b, crossing.quay_a),
        };

        let actor = context.get_actor(tour)?;
        let route = Route { actor, tour: Default::default() };

        let from_idx = context.get_location_index(&from.location)?;
        let to_idx = context.get_location_index(&to.location)?;

        let from_departure = parse_time(&from.time.departure);
        let expected_quay_arrival =
            from_departure + road.duration(&route, from_idx, board_quay, TravelTime::Departure(from_departure));
        if (expected_quay_arrival - leg.arrive_quay_at).abs() > 1. {
            return Err(format!(
                "ferryLeg '{}' quay arrival mismatch: expected ~{expected_quay_arrival}, got {}",
                leg.crossing_id, leg.arrive_quay_at
            )
            .into());
        }

        // any stop strictly between the two point stops a leg names is a transit (required
        // break) stop spliced in by the writer - its own dwell (departure minus arrival) is real
        // elapsed time the next point stop's arrival must still include.
        let break_dwell: Duration = tour.stops[leg.from_stop_index + 1..leg.to_stop_index]
            .iter()
            .map(|stop| parse_time(&stop.schedule().departure) - parse_time(&stop.schedule().arrival))
            .sum();

        let expected_to_arrival = leg.sailing_arrival
            + road.duration(&route, disembark_quay, to_idx, TravelTime::Departure(leg.sailing_arrival))
            + break_dwell;
        let actual_to_arrival = parse_time(&to.time.arrival);
        if (expected_to_arrival - actual_to_arrival).abs() > 1. {
            return Err(format!(
                "ferryLeg '{}' next stop arrival mismatch: expected ~{expected_to_arrival}, got {actual_to_arrival}",
                leg.crossing_id
            )
            .into());
        }

        Ok(())
    })
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
