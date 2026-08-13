#[cfg(test)]
#[path = "../../tests/unit/checker/assignment_test.rs"]
mod assignment_test;

use super::*;
use crate::format::get_indices;
use crate::format::solution::activity_matcher::*;
use crate::utils::combine_error_results;
use std::collections::HashSet;
use vrp_core::construction::clustering::vicinity::ServingPolicy;
use vrp_core::models::solution::Place;
use vrp_core::prelude::GenericResult;
use vrp_core::utils::GenericError;

/// Checks assignment of jobs and vehicles.
pub fn check_assignment(ctx: &CheckerContext) -> Result<(), Vec<GenericError>> {
    combine_error_results(&[
        check_vehicles(ctx),
        check_jobs_presence(ctx),
        check_jobs_match(ctx),
        check_fixed_order(ctx),
        check_max_ride_duration(ctx),
        check_solo_riding(ctx),
        check_lifo(ctx),
        check_groups(ctx),
    ])
}

/// Checks the final scheduled interval constrained by `maxRideDuration`.
///
/// For jobs with multiple pickups/deliveries, the earliest pickup departure to
/// latest delivery arrival is the longest passenger/item ride and therefore the
/// conservative interval to validate.
fn check_max_ride_duration(ctx: &CheckerContext) -> GenericResult<()> {
    let constrained_jobs = ctx
        .problem
        .plan
        .jobs
        .iter()
        .filter_map(|job| job.max_ride_duration.map(|limit| (job.id.as_str(), limit)))
        .collect::<HashMap<_, _>>();
    if constrained_jobs.is_empty() {
        return Ok(());
    }

    ctx.solution.tours.iter().try_for_each(|tour| {
        let mut intervals = HashMap::<&str, (Vec<Float>, Vec<Float>)>::new();
        for stop in &tour.stops {
            let schedule = stop.schedule();
            for activity in stop.activities() {
                if !constrained_jobs.contains_key(activity.job_id.as_str()) {
                    continue;
                }
                let entry = intervals.entry(activity.job_id.as_str()).or_default();
                match activity.activity_type.as_str() {
                    "pickup" => {
                        entry.0.push(parse_time(activity.time.as_ref().map_or(&schedule.departure, |time| &time.end)))
                    }
                    "delivery" => {
                        entry.1.push(parse_time(activity.time.as_ref().map_or(&schedule.arrival, |time| &time.start)))
                    }
                    _ => {}
                }
            }
        }

        intervals.into_iter().try_for_each(|(job_id, (pickups, deliveries))| {
            if pickups.is_empty() || deliveries.is_empty() {
                return Ok(());
            }
            let pickup_departure = pickups.into_iter().min_by(Float::total_cmp).unwrap();
            let delivery_arrival = deliveries.into_iter().max_by(Float::total_cmp).unwrap();
            let ride_duration = delivery_arrival - pickup_departure;
            let limit = constrained_jobs[job_id];
            if ride_duration > limit + 1. {
                Err(format!(
                    "max ride duration is not respected for job '{}': duration {:.0}s exceeds {:.0}s",
                    job_id, ride_duration, limit
                )
                .into())
            } else {
                Ok(())
            }
        })
    })
}

/// Checks that vehicles in each tour are used once per shift and they are known in problem.
fn check_vehicles(ctx: &CheckerContext) -> GenericResult<()> {
    let all_vehicles: HashSet<_> = ctx.problem.fleet.vehicles.iter().flat_map(|v| v.vehicle_ids.iter()).collect();
    let mut used_vehicles = HashSet::<(String, usize)>::new();

    ctx.solution.tours.iter().try_for_each(|tour| {
        if !all_vehicles.contains(&tour.vehicle_id) {
            return Err(format!("used vehicle with unknown id: '{}'", tour.vehicle_id));
        }

        if !(used_vehicles.insert((tour.vehicle_id.to_string(), tour.shift_index))) {
            Err(format!("vehicle with '{}' id used more than once for shift {}", tour.vehicle_id, tour.shift_index))
        } else {
            Ok(())
        }
    })?;

    Ok(())
}

/// Checks job task rules.
fn check_jobs_presence(ctx: &CheckerContext) -> GenericResult<()> {
    struct JobAssignment {
        pub tour_info: (String, usize),
        pub pickups: Vec<usize>,
        pub deliveries: Vec<usize>,
        pub replacements: Vec<usize>,
        pub services: Vec<usize>,
    }
    let new_assignment = |tour_info: (String, usize)| JobAssignment {
        tour_info,
        pickups: vec![],
        deliveries: vec![],
        replacements: vec![],
        services: vec![],
    };
    let activity_types: HashSet<_> = vec!["pickup", "delivery", "service", "replacement"].into_iter().collect();

    let all_jobs = ctx.problem.plan.jobs.iter().map(|job| (job.id.clone(), job.clone())).collect::<HashMap<_, _>>();
    let mut used_jobs = HashMap::<String, JobAssignment>::new();

    ctx.solution.tours.iter().try_for_each(|tour| {
        tour.stops
            .iter()
            .flat_map(|stop| stop.activities())
            .enumerate()
            .filter(|(_, activity)| activity_types.contains(&activity.activity_type.as_str()))
            .try_for_each(|(idx, activity)| {
                let tour_info = (tour.vehicle_id.clone(), tour.shift_index);
                let asgn =
                    used_jobs.entry(activity.job_id.clone()).or_insert_with(|| new_assignment(tour_info.clone()));

                if asgn.tour_info != tour_info {
                    return Err(GenericError::from(format!("job served in multiple tours: '{}'", activity.job_id)));
                }

                match activity.activity_type.as_str() {
                    "pickup" => asgn.pickups.push(idx),
                    "delivery" => asgn.deliveries.push(idx),
                    "service" => asgn.services.push(idx),
                    "replacement" => asgn.replacements.push(idx),
                    _ => {}
                }

                Ok(())
            })
    })?;

    used_jobs.iter().try_for_each(|(id, asgn)| {
        // TODO validate whether each job task is served once
        let job = all_jobs.get(id).ok_or_else(|| format!("cannot find job with id {id}"))?;
        let expected_tasks = job.pickups.as_ref().map_or(0, |p| p.len())
            + job.deliveries.as_ref().map_or(0, |d| d.len())
            + job.services.as_ref().map_or(0, |s| s.len())
            + job.replacements.as_ref().map_or(0, |r| r.len());
        let assigned_tasks = asgn.pickups.len() + asgn.deliveries.len() + asgn.services.len() + asgn.replacements.len();

        if expected_tasks != assigned_tasks {
            return Err(GenericError::from(format!(
                "not all tasks served for '{id}', expected: {expected_tasks}, assigned: {assigned_tasks}"
            )));
        }

        if !asgn.deliveries.is_empty() && asgn.pickups.iter().max() > asgn.deliveries.iter().min() {
            return Err(GenericError::from(format!("found pickup after delivery for '{id}'")));
        }

        Ok(())
    })?;

    let all_unassigned_jobs = ctx
        .solution
        .unassigned
        .iter()
        .flat_map(|jobs| jobs.iter().filter(|job| !job.job_id.ends_with("_break")))
        .map(|job| job.job_id.clone())
        .collect::<Vec<_>>();

    let unique_unassigned_jobs = all_unassigned_jobs.iter().cloned().collect::<HashSet<_>>();

    if unique_unassigned_jobs.len() != all_unassigned_jobs.len() {
        return Err("duplicated job ids in the list of unassigned jobs".into());
    }

    unique_unassigned_jobs.iter().try_for_each::<_, GenericResult<_>>(|job_id| {
        if !all_jobs.contains_key(job_id) {
            return Err(format!("unknown job id in the list of unassigned jobs: '{job_id}'").into());
        }

        if used_jobs.contains_key(job_id) {
            return Err(format!("job present as assigned and unassigned: '{job_id}'").into());
        }

        Ok(())
    })?;

    let all_used_job = unique_unassigned_jobs.into_iter().chain(used_jobs.into_keys()).collect::<Vec<_>>();

    if all_used_job.len() != all_jobs.len() {
        return Err(format!(
            "amount of jobs present in problem and solution doesn't match: {} vs {}",
            all_jobs.len(),
            all_used_job.len()
        )
        .into());
    }

    Ok(())
}

/// Checks job constraint violations.
fn check_jobs_match(ctx: &CheckerContext) -> GenericResult<()> {
    let (job_index, coord_index) = get_indices(&ctx.core_problem.extras)?;
    let (job_index, coord_index) = (job_index.as_ref(), coord_index.as_ref());

    let job_ids = ctx
        .solution
        .tours
        .iter()
        .flat_map(move |tour| {
            tour.stops.iter().flat_map(move |stop| {
                stop.activities()
                    .iter()
                    .enumerate()
                    .filter({
                        move |(idx, activity)| {
                            match stop {
                                Stop::Point(stop) => {
                                    let result = try_match_point_job(tour, stop, activity, job_index, coord_index);
                                    match result {
                                        Err(_) => {
                                            // NOTE required break is not a job
                                            if activity.activity_type == "break" {
                                                try_match_break_activity(&ctx.problem, tour, &stop.time, activity)
                                                    .is_err()
                                            } else {
                                                true
                                            }
                                        }
                                        Ok(Some(JobInfo(_, _, place, time))) => {
                                            is_valid_job_info(ctx, stop, activity, *idx, place, time)
                                        }
                                        _ => false,
                                    }
                                }
                                Stop::Transit(stop) => {
                                    try_match_transit_activity(&ctx.problem, tour, stop, activity).is_err()
                                }
                            }
                        }
                    })
                    .map(|(_, activity)| {
                        format!(
                            "{}:{}",
                            activity.job_id.clone(),
                            activity.job_tag.as_ref().unwrap_or(&"<no tag>".to_string())
                        )
                    })
            })
        })
        .collect::<Vec<_>>();

    if !job_ids.is_empty() {
        return Err(format!("cannot match activities to jobs: {}", job_ids.join(", ")).into());
    }

    Ok(())
}

/// Checks that jobs marked with `fixedOrder` use the exact task sequence used by the problem reader:
/// pickups, deliveries, replacements, then services, preserving order within each task collection.
fn check_fixed_order(ctx: &CheckerContext) -> GenericResult<()> {
    let activities_by_job = ctx
        .solution
        .tours
        .iter()
        .flat_map(|tour| tour.stops.iter())
        .flat_map(|stop| stop.activities())
        .filter(|activity| matches!(activity.activity_type.as_str(), "pickup" | "delivery" | "replacement" | "service"))
        .fold(HashMap::<&str, Vec<&Activity>>::new(), |mut activities_by_job, activity| {
            activities_by_job.entry(activity.job_id.as_str()).or_default().push(activity);
            activities_by_job
        });

    ctx.problem.plan.jobs.iter().filter(|job| job.fixed_order == Some(true)).try_for_each(|job| {
        let expected = get_ordered_tasks(job);
        let actual = activities_by_job.get(job.id.as_str()).map_or(&[] as &[_], Vec::as_slice);

        // An unassigned job has no activities and is valid from an ordering perspective.
        if actual.is_empty() {
            return Ok(());
        }

        if actual.len() != expected.len() {
            return Err(format!(
                "fixed order cannot be checked for job '{}': expected {} activities, found {}",
                job.id,
                expected.len(),
                actual.len()
            )
            .into());
        }

        expected.iter().zip(actual.iter()).enumerate().try_for_each(|(idx, ((expected_type, task), activity))| {
            let has_multiple_tasks_of_type =
                expected.iter().filter(|(activity_type, _)| activity_type == expected_type).count() > 1;
            let tag_matches = !has_multiple_tasks_of_type
                || task.places.iter().any(|place| place.tag.as_ref() == activity.job_tag.as_ref());

            if activity.activity_type == *expected_type && tag_matches {
                Ok(())
            } else {
                let expected_tags = task.places.iter().filter_map(|place| place.tag.as_deref()).collect::<Vec<_>>();
                Err(format!(
                    "fixed order is not respected for job '{}': activity {} expected type '{}' with tag in {:?}, found type '{}' with tag {:?}",
                    job.id,
                    idx,
                    expected_type,
                    expected_tags,
                    activity.activity_type,
                    activity.job_tag
                )
                .into())
            }
        })
    })
}

/// Checks that a solo-riding job does not overlap another dynamic pickup-delivery job while onboard.
/// A parent job becomes active on its first dynamic pickup and completes on its final dynamic delivery,
/// which also handles companion jobs with unequal pickup and delivery activity counts.
fn check_solo_riding(ctx: &CheckerContext) -> GenericResult<()> {
    let jobs = ctx.problem.plan.jobs.iter().map(|job| (job.id.as_str(), job)).collect::<HashMap<_, _>>();
    let solo_jobs = jobs
        .iter()
        .filter_map(|(job_id, job)| (job.solo_riding == Some(true)).then_some(*job_id))
        .collect::<HashSet<_>>();

    if solo_jobs.is_empty() {
        return Ok(());
    }

    let dynamic_delivery_counts = jobs
        .iter()
        .map(|(job_id, job)| {
            let count = job.deliveries.iter().flatten().filter(|task| is_dynamic_task(job, "delivery", task)).count();
            (*job_id, count)
        })
        .collect::<HashMap<_, _>>();

    ctx.solution.tours.iter().try_for_each(|tour| {
        let mut active_jobs = HashSet::<&str>::new();
        let mut completed_deliveries = HashMap::<&str, usize>::new();
        let mut active_solo: Option<&str> = None;

        tour.stops
            .iter()
            .flat_map(|stop| stop.activities())
            .enumerate()
            .try_for_each(|(activity_idx, activity)| {
                let Some(job) = jobs.get(activity.job_id.as_str()) else {
                    return Ok(());
                };
                let Some(task) = get_activity_task(job, activity)? else {
                    return Ok(());
                };

                if !is_dynamic_task(job, activity.activity_type.as_str(), task) {
                    return Ok(());
                }

                let job_id = job.id.as_str();
                match activity.activity_type.as_str() {
                    "pickup" => {
                        if active_solo.is_some_and(|solo_job_id| solo_job_id != job_id) {
                            return Err(format!(
                                "solo riding is not respected in tour '{}'/{} at activity {}: job '{}' is picked up while solo job '{}' is onboard",
                                tour.vehicle_id,
                                tour.shift_index,
                                activity_idx,
                                job_id,
                                active_solo.unwrap()
                            )
                            .into());
                        }

                        if solo_jobs.contains(job_id) && active_jobs.iter().any(|active_id| *active_id != job_id) {
                            return Err(format!(
                                "solo riding is not respected in tour '{}'/{} at activity {}: solo job '{}' is picked up while another job is onboard",
                                tour.vehicle_id, tour.shift_index, activity_idx, job_id
                            )
                            .into());
                        }

                        active_jobs.insert(job_id);
                        if solo_jobs.contains(job_id) {
                            active_solo = Some(job_id);
                        }
                    }
                    "delivery" => {
                        if active_solo.is_some_and(|solo_job_id| solo_job_id != job_id) {
                            return Err(format!(
                                "solo riding is not respected in tour '{}'/{} at activity {}: job '{}' is delivered while solo job '{}' is onboard",
                                tour.vehicle_id,
                                tour.shift_index,
                                activity_idx,
                                job_id,
                                active_solo.unwrap()
                            )
                            .into());
                        }

                        let completed = completed_deliveries.entry(job_id).or_default();
                        *completed += 1;
                        if *completed >= dynamic_delivery_counts.get(job_id).copied().unwrap_or_default() {
                            active_jobs.remove(job_id);
                            if active_solo == Some(job_id) {
                                active_solo = None;
                            }
                        }
                    }
                    _ => {}
                }

                if let Some(solo_job_id) = active_solo
                    && active_jobs.iter().any(|active_id| *active_id != solo_job_id)
                {
                    return Err(format!(
                        "solo riding is not respected in tour '{}'/{} at activity {}: solo job '{}' overlaps another job",
                        tour.vehicle_id, tour.shift_index, activity_idx, solo_job_id
                    )
                    .into());
                }

                Ok(())
            })
    })
}

/// Checks that dynamic pickup-delivery jobs follow LIFO ordering when their `lifoTag` is enforced
/// by the concrete vehicle. Each enforced tag has its own independent stack, matching the solver's
/// LIFO feature semantics.
fn check_lifo(ctx: &CheckerContext) -> GenericResult<()> {
    let jobs = ctx.problem.plan.jobs.iter().map(|job| (job.id.as_str(), job)).collect::<HashMap<_, _>>();

    if !jobs.values().any(|job| job.lifo_tag.is_some()) {
        return Ok(());
    }

    ctx.solution.tours.iter().try_for_each(|tour| {
        let vehicle = ctx
            .problem
            .fleet
            .vehicles
            .iter()
            .find(|vehicle| vehicle.vehicle_ids.contains(&tour.vehicle_id))
            .ok_or_else(|| GenericError::from(format!("cannot check LIFO for unknown vehicle '{}'", tour.vehicle_id)))?;
        let enforced_tags = vehicle
            .lifo_tags
            .iter()
            .flatten()
            .map(String::as_str)
            .collect::<HashSet<_>>();

        if enforced_tags.is_empty() {
            return Ok(());
        }

        let mut stacks = HashMap::<&str, Vec<&str>>::new();

        tour.stops
            .iter()
            .flat_map(|stop| stop.activities())
            .enumerate()
            .try_for_each(|(activity_idx, activity)| {
                let Some(job) = jobs.get(activity.job_id.as_str()) else {
                    return Ok(());
                };
                let Some(lifo_tag) = job.lifo_tag.as_deref().filter(|tag| enforced_tags.contains(tag)) else {
                    return Ok(());
                };
                let Some(task) = get_activity_task(job, activity)? else {
                    return Ok(());
                };

                if !is_dynamic_task(job, activity.activity_type.as_str(), task) {
                    return Ok(());
                }

                let job_id = job.id.as_str();
                let stack = stacks.entry(lifo_tag).or_default();

                match activity.activity_type.as_str() {
                    "pickup" => stack.push(job_id),
                    "delivery" => match stack.last().copied() {
                        Some(expected_job_id) if expected_job_id == job_id => {
                            stack.pop();
                        }
                        Some(expected_job_id) => {
                            return Err(format!(
                                "LIFO order is not respected in tour '{}'/{} at activity {} for tag '{}': delivery job '{}' expected job '{}' on top of the stack",
                                tour.vehicle_id,
                                tour.shift_index,
                                activity_idx,
                                lifo_tag,
                                job_id,
                                expected_job_id
                            )
                            .into());
                        }
                        None => {
                            return Err(format!(
                                "LIFO order is not respected in tour '{}'/{} at activity {} for tag '{}': delivery job '{}' has no matching pickup on the stack",
                                tour.vehicle_id, tour.shift_index, activity_idx, lifo_tag, job_id
                            )
                            .into());
                        }
                    },
                    _ => {}
                }

                Ok(())
            })
    })
}

fn get_ordered_tasks(job: &Job) -> Vec<(&'static str, &JobTask)> {
    job.pickups
        .iter()
        .flatten()
        .map(|task| ("pickup", task))
        .chain(job.deliveries.iter().flatten().map(|task| ("delivery", task)))
        .chain(job.replacements.iter().flatten().map(|task| ("replacement", task)))
        .chain(job.services.iter().flatten().map(|task| ("service", task)))
        .collect()
}

fn get_activity_task<'a>(job: &'a Job, activity: &Activity) -> GenericResult<Option<&'a JobTask>> {
    let tasks = match activity.activity_type.as_str() {
        "pickup" => job.pickups.as_ref(),
        "delivery" => job.deliveries.as_ref(),
        "replacement" => job.replacements.as_ref(),
        "service" => job.services.as_ref(),
        _ => return Ok(None),
    };
    let Some(tasks) = tasks else { return Ok(None) };

    let task_count = job_task_size(&job.pickups)
        + job_task_size(&job.deliveries)
        + job_task_size(&job.replacements)
        + job_task_size(&job.services);
    let pickup_count = job.pickups.as_ref().map_or(0, Vec::len);
    let delivery_count = job.deliveries.as_ref().map_or(0, Vec::len);

    if task_count < 2 || (task_count == 2 && pickup_count == 1 && delivery_count == 1) {
        return Ok(tasks.first());
    }

    let tag = activity.job_tag.as_ref().ok_or_else(|| {
        GenericError::from(format!("checker requires that multi job activity must have tag: '{}'", activity.job_id))
    })?;

    Ok(tasks.iter().find(|task| task.places.iter().any(|place| place.tag.as_ref() == Some(tag))))
}

fn is_dynamic_task(job: &Job, activity_type: &str, task: &JobTask) -> bool {
    let has_pickups = job.pickups.as_ref().is_some_and(|tasks| !tasks.is_empty());
    let has_deliveries = job.deliveries.as_ref().is_some_and(|tasks| !tasks.is_empty());
    let has_demand = task.demand.as_ref().is_some_and(|demand| demand.iter().any(|value| *value != 0))
        || task.named_demand.as_ref().is_some_and(|demand| demand.values().any(|value| *value != 0));

    has_pickups && has_deliveries && matches!(activity_type, "pickup" | "delivery") && has_demand
}

fn is_valid_job_info(
    ctx: &CheckerContext,
    stop: &PointStop,
    activity: &Activity,
    activity_idx: usize,
    place: Place,
    time: TimeWindow,
) -> bool {
    let not_equal = |left: Float, right: Float| left != right;
    let parking = ctx.clustering.as_ref().map(|config| config.serving.get_parking()).unwrap_or(0.);
    let commute_profile = ctx.clustering.as_ref().map(|config| config.profile.clone());
    let domain_commute = ctx.get_commute_info(commute_profile, parking, stop, activity_idx);
    let extra_time = get_extra_time(stop, activity, &place).unwrap_or(0.);

    match (&ctx.clustering, &activity.commute, domain_commute) {
        (_, _, Err(_)) | (_, None, Ok(Some(_))) | (_, Some(_), Ok(None)) | (&None, &Some(_), Ok(Some(_))) => true,
        (_, None, Ok(None)) => {
            let expected_departure = time.start.max(place.time.start) + place.duration + extra_time;
            not_equal(time.end, expected_departure)
        }
        (Some(config), Some(commute), Ok(Some(d_commute))) => {
            let (service_time, parking) = match config.serving {
                ServingPolicy::Original { parking } => (place.duration, parking),
                ServingPolicy::Multiplier { multiplier, parking } => (place.duration * multiplier, parking),
                ServingPolicy::Fixed { value, parking } => (value, parking),
            };

            let a_commute = commute.to_domain(&ctx.coord_index);

            // NOTE: we keep parking in service time of a first activity of the non-first cluster
            let service_time =
                service_time + if a_commute.is_zero_distance() && activity_idx > 0 { parking } else { 0. };

            let expected_departure =
                time.start.max(place.time.start) + service_time + d_commute.backward.duration + extra_time;
            let actual_departure = time.end + d_commute.backward.duration;

            // NOTE: a "workaroundish" approach for two clusters in the same stop
            (not_equal(actual_departure, expected_departure)
                && not_equal(actual_departure, expected_departure - parking))
                // compare commute
                || not_equal(a_commute.forward.distance, d_commute.forward.distance)
                || not_equal(a_commute.forward.duration, d_commute.forward.duration)
                || not_equal(a_commute.backward.distance, d_commute.backward.distance)
                || not_equal(a_commute.backward.duration, d_commute.backward.duration)
        }
    }
}

fn check_groups(ctx: &CheckerContext) -> GenericResult<()> {
    let violations = ctx
        .solution
        .tours
        .iter()
        .fold(HashMap::<String, HashSet<_>>::default(), |mut acc, tour| {
            tour.stops
                .iter()
                .flat_map(|stop| stop.activities().iter())
                .flat_map(|activity| ctx.get_job_by_id(&activity.job_id))
                .flat_map(|job| job.group.as_ref())
                .for_each(|group| {
                    acc.entry(group.clone()).or_default().insert((
                        tour.type_id.clone(),
                        tour.vehicle_id.clone(),
                        tour.shift_index,
                    ));
                });

            acc
        })
        .into_iter()
        .filter(|(_, usage)| usage.len() > 1)
        .collect::<Vec<_>>();

    if violations.is_empty() {
        Ok(())
    } else {
        let err_info = violations.into_iter().map(|(group, _)| group).collect::<Vec<_>>().join(",");
        Err(format!("job groups are not respected: '{err_info}'").into())
    }
}
