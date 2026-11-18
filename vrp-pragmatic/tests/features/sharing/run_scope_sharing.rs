use super::basic_sharing::{create_riders, find_tour, ride_together, sharing};
use crate::format::problem::*;
use crate::format::solution::{Solution, UnassignedJobReason};
use crate::format_time;
use crate::helpers::*;

fn create_vehicle(id: &str, scope: Option<VehicleSharingScope>) -> VehicleType {
    VehicleType {
        sharing: scope.map(|scope| VehicleSharing { tour_rule_scope: Some(scope) }),
        ..create_default_vehicle(id)
    }
}

fn solve(jobs: Vec<Job>, vehicles: Vec<VehicleType>) -> Solution {
    let problem = Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet { vehicles, ..create_default_fleet() },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    solve_with_metaheuristic_and_iterations(problem, Some(vec![matrix]), 500)
}

fn create_tour_conflicting_riders() -> Vec<Job> {
    create_riders(sharing(&["x"], &[], &["y"]), sharing(&["y"], &[], &[]))
}

/// Both pickups close before a vehicle could deliver one rider and come back, so only a shared ride serves both.
fn with_early_pickups(jobs: Vec<Job>) -> Vec<Job> {
    jobs.into_iter()
        .map(|mut job| {
            job.pickups.iter_mut().flatten().flat_map(|task| task.places.iter_mut()).for_each(|place| {
                place.times = Some(vec![vec![format_time(0.), format_time(5.)]]);
            });
            job
        })
        .collect()
}

fn get_unassigned_ids(solution: &Solution) -> Vec<String> {
    solution.unassigned.iter().flatten().map(|job| job.job_id.clone()).collect()
}

fn get_reasons(solution: &Solution) -> Vec<UnassignedJobReason> {
    solution.unassigned.iter().flatten().flat_map(|job| job.reasons.iter().cloned()).collect()
}

fn has_sharing_reason_for(solution: &Solution, vehicle_id: &str) -> bool {
    get_reasons(solution).iter().any(|reason| {
        reason.code == "SHARING_CONSTRAINT"
            && reason.details.iter().flatten().any(|detail| detail.vehicle_id == vehicle_id)
    })
}

#[test]
fn can_serve_tour_conflicting_labels_in_different_runs_of_one_tour_on_run_scope_vehicle() {
    let solution = solve(create_tour_conflicting_riders(), vec![create_vehicle("run", Some(VehicleSharingScope::Run))]);

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    assert!(!ride_together(&solution.tours[0], "a", "b"));
}

#[test]
fn can_share_ride_without_conflicting_labels_on_run_scope_vehicle() {
    let jobs = create_riders(sharing(&["x"], &[], &[]), sharing(&["y"], &[], &[]));

    let solution = solve(jobs, vec![create_vehicle("run", Some(VehicleSharingScope::Run))]);

    assert!(solution.unassigned.is_none());
    assert!(ride_together(&solution.tours[0], "a", "b"), "sharing the ride is the cheaper plan");
}

#[test]
fn can_keep_tour_conflicting_labels_in_separate_tours_on_tour_scope_vehicle() {
    for scope in [None, Some(VehicleSharingScope::Tour)] {
        let two_vehicles = VehicleType {
            vehicle_ids: vec!["tour_1".to_string(), "tour_2".to_string()],
            ..create_vehicle("tour", scope)
        };

        let solution = solve(create_tour_conflicting_riders(), vec![two_vehicles]);

        assert!(solution.unassigned.is_none(), "{scope:?}");
        assert_eq!(solution.tours.len(), 2, "{scope:?}");
        assert_ne!(find_tour(&solution, "a").vehicle_id, find_tour(&solution, "b").vehicle_id);
    }
}

#[test]
fn can_unassign_job_due_to_tour_sharing_on_tour_scope_vehicle() {
    for scope in [None, Some(VehicleSharingScope::Tour)] {
        let solution = solve(create_tour_conflicting_riders(), vec![create_vehicle("tour", scope)]);

        assert_eq!(solution.tours.len(), 1, "{scope:?}");
        assert_eq!(get_unassigned_ids(&solution).len(), 1, "{scope:?}");
        assert!(has_sharing_reason_for(&solution, "tour_1"), "{scope:?}: {:?}", get_reasons(&solution));
    }
}

#[test]
fn can_unassign_job_due_to_tour_sharing_in_one_run_on_run_scope_vehicle() {
    let vehicles = || vec![create_vehicle("run", Some(VehicleSharingScope::Run))];
    let regular_jobs = with_early_pickups(create_riders(sharing(&["x"], &[], &[]), sharing(&["y"], &[], &[])));

    let regular_solution = solve(regular_jobs, vehicles());
    let refusing_solution = solve(with_early_pickups(create_tour_conflicting_riders()), vehicles());

    assert!(regular_solution.unassigned.is_none(), "both jobs fit in one run without a conflicting label");
    assert!(ride_together(&regular_solution.tours[0], "a", "b"));
    // NOTE as for a run.noneOf conflict, the reported reason is the most frequent failure of the positions tried,
    // and serving the job after the other one's run fails on its time window; the regular problem fitting both
    // jobs proves that sharing causes the unassignment.
    assert_eq!(get_unassigned_ids(&refusing_solution).len(), 1);
    assert_eq!(refusing_solution.tours.len(), 1);
}

#[test]
fn can_follow_tour_rule_scope_of_each_vehicle_in_one_fleet() {
    let with_skill = |suffix: &str| {
        create_tour_conflicting_riders()
            .into_iter()
            .map(|job| Job {
                id: format!("{}{suffix}", job.id),
                skills: Some(JobSkills { all_of: Some(vec![suffix.to_string()]), one_of: None, none_of: None }),
                ..job
            })
            .collect::<Vec<_>>()
    };
    let jobs = [with_skill("_run"), with_skill("_tour")].concat();
    let vehicles = vec![
        VehicleType { skills: Some(vec!["_run".to_string()]), ..create_vehicle("run", Some(VehicleSharingScope::Run)) },
        VehicleType { skills: Some(vec!["_tour".to_string()]), ..create_vehicle("tour", None) },
    ];

    let solution = solve(jobs, vehicles);

    let run_tour = find_tour(&solution, "a_run");
    assert_eq!(run_tour.vehicle_id, "run_1");
    assert_eq!(find_tour(&solution, "b_run").vehicle_id, "run_1");
    assert!(!ride_together(run_tour, "a_run", "b_run"));

    let unassigned = get_unassigned_ids(&solution);
    assert_eq!(unassigned.len(), 1);
    assert!(unassigned[0].ends_with("_tour"));
    assert!(has_sharing_reason_for(&solution, "tour_1"), "{:?}", get_reasons(&solution));
}
