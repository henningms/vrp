use serde::Serialize;
use std::collections::HashMap;
use vrp_core::prelude::GenericError;
use vrp_pragmatic::format::problem::{JobPlace, Matrix, Problem};
use vrp_pragmatic::format::solution::{Activity, Interval, Solution};
use vrp_pragmatic::format::{CoordIndex, Location};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RideQualityReport {
    definition: Definition,
    coverage: Coverage,
    ride_time_seconds: Distribution,
    matrix_direct_seconds: Distribution,
    excess_over_direct_seconds: Distribution,
    ratio_over_direct: Distribution,
    requested_span_seconds: Distribution,
    excess_over_requested_seconds: Distribution,
    ratio_over_requested: Distribution,
    zero_direct_ride_time_seconds: Distribution,
    thresholds: Thresholds,
    worst_by_direct_excess: Vec<RideLeg>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Definition {
    actual_ride_time: &'static str,
    direct_baseline: &'static str,
    requested_baseline: &'static str,
    ratio_eligibility: String,
    multi_pickup_jobs: &'static str,
    zero_direct_legs: &'static str,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    problem_jobs: usize,
    problem_ride_legs: usize,
    multi_pickup_jobs: usize,
    solution_tours: usize,
    solution_unassigned_jobs: usize,
    measured_ride_legs: usize,
    legs_with_requested_baseline: usize,
    zero_direct_legs: usize,
    ratio_eligible_legs: usize,
    missing_activity_pairs: usize,
    missing_problem_place: usize,
    missing_matrix_value: usize,
    invalid_ride_time: usize,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Distribution {
    count: usize,
    min: Option<f64>,
    mean: Option<f64>,
    median: Option<f64>,
    p90: Option<f64>,
    p95: Option<f64>,
    p99: Option<f64>,
    max: Option<f64>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Thresholds {
    direct_ratio: Vec<ThresholdCount>,
    direct_excess_seconds: Vec<ThresholdCount>,
    requested_ratio: Vec<ThresholdCount>,
    requested_excess_seconds: Vec<ThresholdCount>,
    painful_rides: PainfulRides,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThresholdCount {
    threshold: f64,
    count: usize,
    share: f64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct PainfulRides {
    definition: &'static str,
    count: usize,
    share_of_ratio_eligible: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RideLeg {
    job_id: String,
    pickup_tag: Option<String>,
    vehicle_id: String,
    vehicle_type_id: String,
    pickup_departure: String,
    delivery_arrival: String,
    ride_seconds: f64,
    direct_seconds: i64,
    excess_over_direct_seconds: f64,
    ratio_over_direct: Option<f64>,
    requested_span_seconds: Option<f64>,
    excess_over_requested_seconds: Option<f64>,
    ratio_over_requested: Option<f64>,
}

#[derive(Clone)]
struct ObservedActivity {
    tag: Option<String>,
    location: Option<Location>,
    start: String,
    end: String,
}

#[derive(Default)]
struct ObservedJob {
    vehicle_id: String,
    vehicle_type_id: String,
    matrix_profile: String,
    pickups: Vec<ObservedActivity>,
    deliveries: Vec<ObservedActivity>,
}

/// Calculates passenger ride-quality diagnostics and returns a pretty JSON report.
///
/// Each pickup in a pickup-delivery job is treated as one rider leg ending at the job's
/// single delivery. This also represents companion jobs with two pickups without hiding
/// either passenger. Ratios exclude matrix-direct durations below `min_direct_seconds`.
pub fn get_ride_quality_serialized(
    problem: &Problem,
    matrices: &[Matrix],
    solution: &Solution,
    min_direct_seconds: i64,
    worst_count: usize,
) -> Result<String, GenericError> {
    if min_direct_seconds < 1 {
        return Err("minimum direct time must be at least one second".into());
    }

    let coord_index = CoordIndex::new(problem);
    let matrix_size = validate_matrices(matrices)?;

    let vehicle_profiles = problem
        .fleet
        .vehicles
        .iter()
        .map(|vehicle| (vehicle.type_id.as_str(), vehicle.profile.matrix.as_str()))
        .collect::<HashMap<_, _>>();
    let jobs = problem.plan.jobs.iter().map(|job| (job.id.as_str(), job)).collect::<HashMap<_, _>>();
    let mut observed_jobs: HashMap<String, ObservedJob> = HashMap::new();

    for tour in &solution.tours {
        let matrix_profile = vehicle_profiles
            .get(tour.type_id.as_str())
            .ok_or_else(|| format!("cannot find vehicle type '{}' in problem", tour.type_id))?;

        for stop in &tour.stops {
            for activity in stop.activities() {
                if activity.activity_type != "pickup" && activity.activity_type != "delivery" {
                    continue;
                }

                let observed = to_observed_activity(
                    activity,
                    stop.location(),
                    &stop.schedule().arrival,
                    &stop.schedule().departure,
                );
                let entry = observed_jobs.entry(activity.job_id.clone()).or_default();

                if entry.vehicle_id.is_empty() {
                    entry.vehicle_id.clone_from(&tour.vehicle_id);
                    entry.vehicle_type_id.clone_from(&tour.type_id);
                    entry.matrix_profile = (*matrix_profile).to_string();
                } else if entry.vehicle_id != tour.vehicle_id {
                    return Err(format!("job '{}' has activities on multiple vehicles", activity.job_id).into());
                }

                if activity.activity_type == "pickup" {
                    entry.pickups.push(observed);
                } else {
                    entry.deliveries.push(observed);
                }
            }
        }
    }

    let mut coverage = Coverage {
        problem_jobs: problem.plan.jobs.len(),
        problem_ride_legs: problem
            .plan
            .jobs
            .iter()
            .filter(|job| job.deliveries.as_ref().is_some_and(|deliveries| deliveries.len() == 1))
            .map(|job| job.pickups.as_ref().map_or(0, Vec::len))
            .sum(),
        multi_pickup_jobs: problem
            .plan
            .jobs
            .iter()
            .filter(|job| job.pickups.as_ref().is_some_and(|p| p.len() > 1))
            .count(),
        solution_tours: solution.tours.len(),
        solution_unassigned_jobs: solution.unassigned.as_ref().map_or(0, Vec::len),
        ..Coverage::default()
    };
    let mut legs = Vec::new();

    for (job_id, observed_job) in observed_jobs {
        let Some(job) = jobs.get(job_id.as_str()) else {
            continue;
        };
        if observed_job.deliveries.len() != 1 || observed_job.pickups.is_empty() {
            coverage.missing_activity_pairs += observed_job.pickups.len().max(1);
            continue;
        }

        let delivery = &observed_job.deliveries[0];
        let Some(delivery_place) = find_place(job.deliveries.as_deref(), delivery) else {
            coverage.missing_problem_place += observed_job.pickups.len();
            continue;
        };
        let delivery_location = delivery.location.as_ref().unwrap_or(&delivery_place.location);

        for pickup in &observed_job.pickups {
            let Some(pickup_place) = find_place(job.pickups.as_deref(), pickup) else {
                coverage.missing_problem_place += 1;
                continue;
            };
            let pickup_location = pickup.location.as_ref().unwrap_or(&pickup_place.location);
            let ride_seconds = interval_seconds(&pickup.end, &delivery.start);
            if !ride_seconds.is_finite() || ride_seconds < 0. {
                coverage.invalid_ride_time += 1;
                continue;
            }

            let Some(direct_seconds) = get_direct_time(
                matrices,
                &observed_job.matrix_profile,
                &coord_index,
                matrix_size,
                pickup_location,
                delivery_location,
            ) else {
                coverage.missing_matrix_value += 1;
                continue;
            };

            let requested_span_seconds = match (&pickup_place.requested_time, &delivery_place.requested_time) {
                (Some(start), Some(end)) => {
                    let seconds = interval_seconds(start, end);
                    (seconds > 0.).then_some(seconds)
                }
                _ => None,
            };
            let ratio_over_direct =
                (direct_seconds >= min_direct_seconds).then_some(ride_seconds / direct_seconds as f64);
            let ratio_over_requested = requested_span_seconds
                .filter(|seconds| *seconds >= min_direct_seconds as f64)
                .map(|seconds| ride_seconds / seconds);

            legs.push(RideLeg {
                job_id: job_id.clone(),
                pickup_tag: pickup.tag.clone(),
                vehicle_id: observed_job.vehicle_id.clone(),
                vehicle_type_id: observed_job.vehicle_type_id.clone(),
                pickup_departure: pickup.end.clone(),
                delivery_arrival: delivery.start.clone(),
                ride_seconds,
                direct_seconds,
                excess_over_direct_seconds: ride_seconds - direct_seconds as f64,
                ratio_over_direct,
                requested_span_seconds,
                excess_over_requested_seconds: requested_span_seconds.map(|seconds| ride_seconds - seconds),
                ratio_over_requested,
            });
        }
    }

    coverage.measured_ride_legs = legs.len();
    coverage.legs_with_requested_baseline = legs.iter().filter(|leg| leg.requested_span_seconds.is_some()).count();
    coverage.zero_direct_legs = legs.iter().filter(|leg| leg.direct_seconds == 0).count();
    coverage.ratio_eligible_legs = legs.iter().filter(|leg| leg.ratio_over_direct.is_some()).count();

    let direct_ratios = legs.iter().filter_map(|leg| leg.ratio_over_direct).collect::<Vec<_>>();
    let direct_excess = legs.iter().map(|leg| leg.excess_over_direct_seconds).collect::<Vec<_>>();
    let requested_ratios = legs.iter().filter_map(|leg| leg.ratio_over_requested).collect::<Vec<_>>();
    let requested_excess = legs.iter().filter_map(|leg| leg.excess_over_requested_seconds).collect::<Vec<_>>();
    let painful_count = legs
        .iter()
        .filter(|leg| {
            leg.direct_seconds >= min_direct_seconds && leg.direct_seconds <= 600 && leg.ride_seconds >= 2400.
        })
        .count();

    let mut worst_by_direct_excess = legs.clone();
    worst_by_direct_excess
        .sort_by(|left, right| right.excess_over_direct_seconds.total_cmp(&left.excess_over_direct_seconds));
    worst_by_direct_excess.truncate(worst_count);

    let report = RideQualityReport {
        definition: Definition {
            actual_ride_time: "delivery activity start minus pickup activity end; includes all driving, waiting, and intermediate stops while aboard",
            direct_baseline: "pickup-to-delivery travel time from the vehicle profile's routing matrix",
            requested_baseline: "delivery requestedTime minus pickup requestedTime, when both exist",
            ratio_eligibility: format!(
                "matrix-direct or requested baseline must be at least {min_direct_seconds} seconds"
            ),
            multi_pickup_jobs: "each pickup is reported as a separate rider leg to the job's single delivery",
            zero_direct_legs: "reported separately and excluded from direct ratios; these can represent companion staff returning to the same location",
        },
        ride_time_seconds: distribution(legs.iter().map(|leg| leg.ride_seconds)),
        matrix_direct_seconds: distribution(legs.iter().map(|leg| leg.direct_seconds as f64)),
        excess_over_direct_seconds: distribution(direct_excess.iter().copied()),
        ratio_over_direct: distribution(direct_ratios.iter().copied()),
        requested_span_seconds: distribution(legs.iter().filter_map(|leg| leg.requested_span_seconds)),
        excess_over_requested_seconds: distribution(requested_excess.iter().copied()),
        ratio_over_requested: distribution(requested_ratios.iter().copied()),
        zero_direct_ride_time_seconds: distribution(
            legs.iter().filter(|leg| leg.direct_seconds == 0).map(|leg| leg.ride_seconds),
        ),
        thresholds: Thresholds {
            direct_ratio: threshold_counts(&direct_ratios, &[1.5, 2., 3.]),
            direct_excess_seconds: threshold_counts(&direct_excess, &[900., 1800.]),
            requested_ratio: threshold_counts(&requested_ratios, &[1.5, 2., 3.]),
            requested_excess_seconds: threshold_counts(&requested_excess, &[900., 1800.]),
            painful_rides: PainfulRides {
                definition: "matrix-direct time between the ratio minimum and 10 minutes, but actual ride is at least 40 minutes",
                count: painful_count,
                share_of_ratio_eligible: share(painful_count, direct_ratios.len()),
            },
        },
        coverage,
        worst_by_direct_excess,
    };

    serde_json::to_string_pretty(&report).map_err(|err| format!("cannot serialize ride quality: '{err}'").into())
}

fn to_observed_activity(
    activity: &Activity,
    stop_location: Option<&Location>,
    stop_arrival: &str,
    stop_departure: &str,
) -> ObservedActivity {
    ObservedActivity {
        tag: activity.job_tag.clone(),
        location: activity.location.clone().or_else(|| stop_location.cloned()),
        start: activity.time.as_ref().map_or_else(|| stop_arrival.to_string(), |time| time.start.clone()),
        end: activity.time.as_ref().map_or_else(|| stop_departure.to_string(), |time| time.end.clone()),
    }
}

fn find_place<'a>(
    tasks: Option<&'a [vrp_pragmatic::format::problem::JobTask]>,
    observed: &ObservedActivity,
) -> Option<&'a JobPlace> {
    let places = tasks?.iter().flat_map(|task| task.places.iter()).collect::<Vec<_>>();
    observed
        .tag
        .as_ref()
        .and_then(|tag| places.iter().find(|place| place.tag.as_ref() == Some(tag)).copied())
        .or_else(|| {
            observed
                .location
                .as_ref()
                .and_then(|location| places.iter().find(|place| &place.location == location).copied())
        })
        .or_else(|| (places.len() == 1).then(|| places[0]))
}

fn get_direct_time(
    matrices: &[Matrix],
    profile: &str,
    coord_index: &CoordIndex,
    matrix_size: usize,
    from: &Location,
    to: &Location,
) -> Option<i64> {
    let matrix = matrices
        .iter()
        .find(|matrix| matrix.profile.as_deref() == Some(profile))
        .or_else(|| (matrices.len() == 1).then(|| &matrices[0]))?;
    let from = coord_index.get_by_loc(from)?;
    let to = coord_index.get_by_loc(to)?;
    let index = from.checked_mul(matrix_size)?.checked_add(to)?;

    if matrix.error_codes.as_ref().and_then(|codes| codes.get(index)).is_some_and(|code| *code > 0) {
        return None;
    }

    matrix.travel_times.get(index).copied().filter(|time| *time >= 0)
}

fn validate_matrices(matrices: &[Matrix]) -> Result<usize, GenericError> {
    if matrices.is_empty() {
        return Err("ride-quality analysis requires at least one routing matrix".into());
    }
    let matrix_size = (matrices[0].travel_times.len() as f64).sqrt() as usize;
    let expected_len =
        matrix_size.checked_mul(matrix_size).ok_or_else(|| GenericError::from("routing matrix size overflow"))?;
    if expected_len != matrices[0].travel_times.len() {
        return Err(format!(
            "routing matrix has a non-square travel-time array of length {}",
            matrices[0].travel_times.len()
        )
        .into());
    }
    if let Some(matrix) = matrices.iter().find(|matrix| matrix.travel_times.len() != expected_len) {
        return Err(format!(
            "routing matrix profile {:?} has {} travel times, expected {} for {} locations",
            matrix.profile,
            matrix.travel_times.len(),
            expected_len,
            matrix_size
        )
        .into());
    }

    Ok(matrix_size)
}

fn interval_seconds(start: &str, end: &str) -> f64 {
    Interval { start: start.to_string(), end: end.to_string() }.duration()
}

fn distribution(values: impl IntoIterator<Item = f64>) -> Distribution {
    let mut values = values.into_iter().filter(|value| value.is_finite()).collect::<Vec<_>>();
    if values.is_empty() {
        return Distribution::default();
    }

    values.sort_by(f64::total_cmp);
    let sum = values.iter().sum::<f64>();
    Distribution {
        count: values.len(),
        min: values.first().copied(),
        mean: Some(sum / values.len() as f64),
        median: percentile(&values, 0.5),
        p90: percentile(&values, 0.9),
        p95: percentile(&values, 0.95),
        p99: percentile(&values, 0.99),
        max: values.last().copied(),
    }
}

fn percentile(sorted: &[f64], percentile: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let position = percentile * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let weight = position - lower as f64;
    Some(sorted[lower] * (1. - weight) + sorted[upper] * weight)
}

fn threshold_counts(values: &[f64], thresholds: &[f64]) -> Vec<ThresholdCount> {
    thresholds
        .iter()
        .map(|threshold| {
            let count = values.iter().filter(|value| **value >= *threshold).count();
            ThresholdCount { threshold: *threshold, count, share: share(count, values.len()) }
        })
        .collect()
}

fn share(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.
    } else {
        count as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;
    use vrp_pragmatic::format::problem::deserialize_problem;
    use vrp_pragmatic::format::solution::deserialize_solution;

    #[test]
    fn can_calculate_distribution() {
        let distribution = distribution([1., 2., 3., 4., 5.]);

        assert_eq!(distribution.count, 5);
        assert_eq!(distribution.mean, Some(3.));
        assert_eq!(distribution.median, Some(3.));
        assert_eq!(distribution.p90, Some(4.6));
    }

    #[test]
    fn can_count_thresholds() {
        let counts = threshold_counts(&[1., 1.5, 2., 3.], &[1.5, 2.]);

        assert_eq!(counts[0].count, 3);
        assert_eq!(counts[0].share, 0.75);
        assert_eq!(counts[1].count, 2);
    }

    #[test]
    fn can_measure_activity_level_ride_time() {
        let problem = deserialize_problem(BufReader::new(
            File::open("../examples/data/pragmatic/simple.basic.problem.json").unwrap(),
        ))
        .unwrap();
        let solution = deserialize_solution(BufReader::new(
            File::open("../examples/data/pragmatic/simple.basic.solution.json").unwrap(),
        ))
        .unwrap();
        let mut matrix = Matrix {
            profile: Some("normal_car".to_string()),
            timestamp: None,
            travel_times: vec![0; 16],
            distances: vec![0; 16],
            error_codes: None,
        };
        // job3 pickup is matrix location 1 and its delivery is location 2.
        matrix.travel_times[6] = 300;

        let report = get_ride_quality_serialized(&problem, &[matrix], &solution, 60, 5).unwrap();
        let report: serde_json::Value = serde_json::from_str(&report).unwrap();

        assert_eq!(report["coverage"]["measuredRideLegs"], 1);
        assert_eq!(report["rideTimeSeconds"]["median"], 371.);
        assert_eq!(report["matrixDirectSeconds"]["median"], 300.);
        assert_eq!(report["excessOverDirectSeconds"]["median"], 71.);
    }
}
