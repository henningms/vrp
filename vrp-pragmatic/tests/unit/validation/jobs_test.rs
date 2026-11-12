use super::*;
use crate::helpers::*;
use vrp_core::prelude::Float;

fn assert_result(code: &str, action: &str, result: Option<FormatError>) {
    assert_eq!(result.clone().map(|err| err.code), Some(code.to_string()));
    assert!(result.map_or("".to_string(), |err| err.action).contains(action));
}

parameterized_test! {can_detect_reserved_ids, (job_id, expected), {
    can_detect_reserved_ids_impl(job_id.to_string(), expected);
}}

can_detect_reserved_ids! {
    case01: ("job1", None),
    case02: ("departure", Some("departure")),
    case03: ("arrival", Some("arrival")),
    case04: ("break", Some("break")),
    case05: ("reload", Some("reload")),
}

fn can_detect_reserved_ids_impl(job_id: String, expected: Option<&str>) {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job(job_id.as_str(), (1., 0.))], ..create_empty_plan() },
        fleet: create_default_fleet(),
        ..create_empty_problem()
    };

    let result = check_e1104_no_reserved_ids(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    if let Some(action) = expected {
        assert_result("E1104", action, result);
    } else {
        assert!(result.is_none());
    }
}

#[test]
fn can_detect_empty_job() {
    let problem = Problem {
        plan: Plan { jobs: vec![Job { deliveries: Some(vec![]), ..create_job("job1") }], ..create_empty_plan() },
        ..create_empty_problem()
    };

    let result = check_e1105_empty_jobs(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    assert_result("E1105", "job1", result);
}

#[test]
fn can_detect_negative_duration() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job_with_duration("job1", (1., 0.), -10.)], ..create_empty_plan() },
        ..create_empty_problem()
    };

    let result =
        check_e1106_negative_duration(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    assert_result("E1106", "job1", result);
}

#[test]
fn can_detect_negative_demand() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job_with_demand("job1", (1., 0.), vec![0, -1])],
            ..create_empty_plan()
        },
        ..create_empty_problem()
    };

    let result = check_e1107_negative_demand(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    assert_result("E1107", "job1", result);
}

parameterized_test! {can_detect_empty_one_of_skill_group, (one_of, expected), {
    can_detect_empty_one_of_skill_group_impl(one_of, expected);
}}

can_detect_empty_one_of_skill_group! {
    case01: (vec![vec!["driver:a"], vec!["ramp", "lift"]], None),
    case02: (vec![vec!["driver:a"], vec![]], Some("job1")),
    case03: (vec![], None),
}

fn can_detect_empty_one_of_skill_group_impl(one_of: Vec<Vec<&str>>, expected: Option<&str>) {
    let skills = JobSkills { all_of: None, one_of: Some(one_of.into_iter().map(to_strings).collect()), none_of: None };
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job_with_skills("job1", (1., 0.), skills)], ..create_empty_plan() },
        ..create_empty_problem()
    };

    let result =
        check_e9103_no_empty_one_of_skill_group(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem)))
            .err();

    if let Some(action) = expected {
        assert_result("E9103", action, result);
    } else {
        assert!(result.is_none());
    }
}

#[test]
fn can_iterate_all_task_kinds_in_validation_order() {
    let task_with_duration = |duration| JobTask {
        places: vec![JobPlace { duration, ..create_job_place((duration, 0.), None) }],
        demand: None,
        named_demand: None,
        order: None,
    };
    let problem = Problem {
        plan: Plan {
            jobs: vec![Job {
                pickups: Some(vec![task_with_duration(1.)]),
                deliveries: Some(vec![task_with_duration(2.)]),
                replacements: Some(vec![task_with_duration(3.)]),
                services: Some(vec![task_with_duration(4.)]),
                ..create_job("job1")
            }],
            ..create_empty_plan()
        },
        ..create_empty_problem()
    };

    let durations = job_tasks(&problem.plan.jobs[0]).map(|task| task.places[0].duration).collect::<Vec<_>>();

    assert_eq!(durations, vec![1., 2., 3., 4.]);
}

parameterized_test! {can_detect_invalid_stop_duration, (stop_duration, expected), {
    can_detect_invalid_stop_duration_impl(stop_duration, expected);
}}

can_detect_invalid_stop_duration! {
    case01: (None, None),
    case02: (Some(0.), None),
    case03: (Some(30.), None),
    case04: (Some(-1.), Some("E9104".to_string())),
    case05: (Some(Float::NAN), Some("E9104".to_string())),
    case06: (Some(Float::INFINITY), Some("E9104".to_string())),
}

fn can_detect_invalid_stop_duration_impl(stop_duration: Option<Float>, expected: Option<String>) {
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job("job1", (1., 0.)), stop_duration)],
            ..create_empty_plan()
        },
        ..create_empty_problem()
    };

    let result = check_e9104_stop_duration_value(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem)));

    assert_eq!(result.err().map(|err| err.code), expected);
}

parameterized_test! {can_detect_stop_duration_conflict, (stop_duration, has_clustering, has_required_break, expected), {
    can_detect_stop_duration_conflict_impl(stop_duration, has_clustering, has_required_break, expected);
}}

can_detect_stop_duration_conflict! {
    case01: (Some(30.), false, false, None),
    case02: (Some(30.), true, false, Some("E9105".to_string())),
    case03: (Some(30.), false, true, Some("E9105".to_string())),
    case04: (Some(0.), true, true, None),
    case05: (None, true, false, None),
}

fn can_detect_stop_duration_conflict_impl(
    stop_duration: Option<Float>,
    has_clustering: bool,
    has_required_break: bool,
    expected: Option<String>,
) {
    let clustering = Clustering::Vicinity {
        profile: VehicleProfile { matrix: "car".to_string(), scale: None },
        threshold: VicinityThresholdPolicy {
            duration: 30.,
            distance: 16.,
            min_shared_time: None,
            smallest_time_window: None,
            max_jobs_per_cluster: None,
        },
        visiting: VicinityVisitPolicy::Continue,
        serving: VicinityServingPolicy::Original { parking: 300. },
        filtering: None,
    };
    let required_break = VehicleBreak::Required {
        time: VehicleRequiredBreakTime::OffsetTime { earliest: 10., latest: 10. },
        duration: 5.,
    };
    let problem = Problem {
        plan: Plan {
            jobs: vec![set_stop_duration(create_delivery_job("job1", (1., 0.)), stop_duration)],
            clustering: has_clustering.then_some(clustering),
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    breaks: has_required_break.then(|| vec![required_break]),
                    ..create_default_vehicle_shift()
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };

    let result =
        check_e9105_stop_duration_conflict(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem)));

    assert_eq!(result.err().map(|err| err.code), expected);
}

parameterized_test! {can_detect_invalid_ride_stop_limits, (job_kind, limits, expected), {
    can_detect_invalid_ride_stop_limits_impl(job_kind, limits, expected);
}}

can_detect_invalid_ride_stop_limits! {
    case01_pickup_delivery_with_limit: ("pickup_delivery", Some((Some(0), None, None)), None),
    case02_pickup_delivery_with_all_limits: ("pickup_delivery", Some((Some(0), Some(1), Some(2))), None),
    case03_no_limits: ("pickup_delivery", None, None),
    case04_empty_limits: ("pickup_delivery", Some((None, None, None)), Some("at least one")),
    case05_delivery_only: ("delivery", Some((Some(0), None, None)), Some("both pickups and deliveries")),
    case06_pickup_only: ("pickup", Some((None, None, Some(1))), Some("both pickups and deliveries")),
    case07_service: ("service", Some((None, Some(1), None)), Some("both pickups and deliveries")),
}

fn can_detect_invalid_ride_stop_limits_impl(
    job_kind: &str,
    limits: Option<(Option<usize>, Option<usize>, Option<usize>)>,
    expected: Option<&str>,
) {
    let mut job = match job_kind {
        "pickup_delivery" => create_pickup_delivery_job("job1", (1., 0.), (2., 0.)),
        "delivery" => create_delivery_job("job1", (1., 0.)),
        "pickup" => create_pickup_job("job1", (1., 0.)),
        _ => create_service_job("job1", (1., 0.)),
    };
    job.ride_stop_limits = limits.map(|(pickups, deliveries, stops)| RideStopLimits { pickups, deliveries, stops });
    let problem = Problem { plan: Plan { jobs: vec![job], ..create_empty_plan() }, ..create_empty_problem() };

    let result =
        check_e9106_ride_stop_limits(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    if let Some(action) = expected {
        assert_result("E9106", action, result);
    } else {
        assert!(result.is_none());
    }
}

parameterized_test! {can_detect_empty_separation_classes, (class, not_in_same_run, not_in_same_tour, expected), {
    can_detect_empty_separation_classes_impl(class, not_in_same_run, not_in_same_tour, expected);
}}

can_detect_empty_separation_classes! {
    case01_class_only: ("a", None, None, false),
    case02_with_lists: ("a", Some(vec!["a", "b"]), Some(vec!["c"]), false),
    case03_empty_lists: ("a", Some(vec![]), Some(vec![]), false),
    case04_empty_class: ("", None, None, true),
    case05_empty_run_class: ("a", Some(vec!["b", ""]), None, true),
    case06_empty_tour_class: ("a", None, Some(vec![""]), true),
}

fn can_detect_empty_separation_classes_impl(
    class: &str,
    not_in_same_run: Option<Vec<&str>>,
    not_in_same_tour: Option<Vec<&str>>,
    expected: bool,
) {
    let to_classes = |classes: Option<Vec<&str>>| classes.map(|c| c.into_iter().map(str::to_string).collect());
    let mut job = create_pickup_delivery_job("job1", (1., 0.), (2., 0.));
    job.separation = Some(JobSeparation {
        class: class.to_string(),
        not_in_same_run: to_classes(not_in_same_run),
        not_in_same_tour: to_classes(not_in_same_tour),
    });
    let problem = Problem { plan: Plan { jobs: vec![job], ..create_empty_plan() }, ..create_empty_problem() };

    let result =
        check_e9107_separation_classes(&ValidationContext::new(&problem, None, &CoordIndex::new(&problem))).err();

    if expected {
        assert_result("E9107", "non-empty classes", result);
    } else {
        assert!(result.is_none());
    }
}
