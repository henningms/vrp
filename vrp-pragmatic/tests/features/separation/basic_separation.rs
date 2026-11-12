use crate::format::problem::*;
use crate::format::solution::{Solution, Tour, UnassignedJobDetail, UnassignedJobReason};
use crate::helpers::*;
use std::collections::HashSet;

fn separation(class: &str, not_in_same_run: &[&str], not_in_same_tour: &[&str]) -> Option<JobSeparation> {
    let to_classes = |classes: &[&str]| Some(classes.iter().map(|class| class.to_string()).collect());

    Some(JobSeparation {
        class: class.to_string(),
        not_in_same_run: to_classes(not_in_same_run),
        not_in_same_tour: to_classes(not_in_same_tour),
    })
}

// Both riders travel to one destination and are picked up close to each other, so sharing the ride is clearly
// the cheapest plan.
fn create_riders(a: Option<JobSeparation>, b: Option<JobSeparation>) -> Vec<Job> {
    let mut job_a = create_pickup_delivery_job("a", (1., 0.), (10., 0.));
    job_a.separation = a;
    let mut job_b = create_pickup_delivery_job("b", (2., 0.), (10., 0.));
    job_b.separation = b;

    vec![job_a, job_b]
}

fn solve(jobs: Vec<Job>, vehicles: usize) -> Solution {
    let problem = Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: (1..=vehicles).map(|idx| format!("my_vehicle_{idx}")).collect(),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    solve_with_metaheuristic_and_iterations(problem, Some(vec![matrix]), 500)
}

/// Checks whether both jobs are on board together at some point of the tour, by activity order.
fn ride_together(tour: &Tour, first: &str, second: &str) -> bool {
    let mut on_board = HashSet::new();
    tour.stops.iter().flat_map(|stop| stop.activities().iter()).any(|activity| {
        match activity.activity_type.as_str() {
            "pickup" => on_board.insert(activity.job_id.as_str()),
            "delivery" => on_board.remove(activity.job_id.as_str()),
            _ => false,
        };
        on_board.contains(first) && on_board.contains(second)
    })
}

fn find_tour<'a>(solution: &'a Solution, job_id: &str) -> &'a Tour {
    solution.tours.iter().find(|tour| get_ids_from_tour(tour).iter().flatten().any(|id| id == job_id)).unwrap()
}

#[test]
fn can_share_ride_without_separation() {
    let solution = solve(create_riders(separation("x", &[], &[]), separation("y", &[], &[])), 2);

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    assert!(ride_together(&solution.tours[0], "a", "b"));
}

#[test]
fn can_keep_conflicting_classes_in_separate_runs() {
    let solution = solve(create_riders(separation("x", &["y"], &[]), separation("y", &[], &[])), 2);

    assert!(solution.unassigned.is_none());
    assert!(solution.tours.iter().all(|tour| !ride_together(tour, "a", "b")));
}

#[test]
fn can_keep_conflicting_classes_in_separate_runs_when_only_one_side_states_it() {
    let solution = solve(create_riders(separation("x", &[], &[]), separation("y", &["x"], &[])), 2);

    assert!(solution.unassigned.is_none());
    assert!(solution.tours.iter().all(|tour| !ride_together(tour, "a", "b")));
}

#[test]
fn can_keep_conflicting_classes_in_separate_tours() {
    let solution = solve(create_riders(separation("x", &[], &["y"]), separation("y", &[], &[])), 2);

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 2);
    assert_ne!(find_tour(&solution, "a").vehicle_id, find_tour(&solution, "b").vehicle_id);
}

#[test]
fn can_unassign_job_due_to_tour_separation() {
    let solution = solve(create_riders(separation("x", &[], &["y"]), separation("y", &[], &[])), 1);

    assert_eq!(solution.tours.len(), 1);
    let reasons = solution.unassigned.iter().flatten().flat_map(|u| u.reasons.iter().cloned()).collect::<Vec<_>>();
    assert_eq!(
        reasons,
        vec![UnassignedJobReason {
            code: "SEPARATION_CONSTRAINT".to_string(),
            description: "cannot be assigned due to separation constraint".to_string(),
            details: Some(vec![UnassignedJobDetail { vehicle_id: "my_vehicle_1".to_string(), shift_index: 0 }])
        }]
    );
}

#[test]
fn can_unassign_job_due_to_run_separation() {
    // The tour order forces the second job to be picked up while the first one is on board.
    let with_order = |mut job: Job, pickup_order: i32, delivery_order: i32| {
        job.pickups.as_mut().unwrap()[0].order = Some(pickup_order);
        job.deliveries.as_mut().unwrap()[0].order = Some(delivery_order);
        job
    };
    let jobs = |a: Option<JobSeparation>| {
        let [job_a, job_b]: [Job; 2] = create_riders(a, separation("y", &[], &[])).try_into().unwrap();
        vec![with_order(job_a, 1, 3), with_order(job_b, 2, 4)]
    };

    let regular_solution = solve(jobs(separation("x", &[], &[])), 1);
    let separated_solution = solve(jobs(separation("x", &["y"], &[])), 1);

    assert!(regular_solution.unassigned.is_none(), "both jobs fit without separation");
    // NOTE the reported reason is the most frequent failure, which is the tour order of the positions tried
    // outside the run; the regular problem fitting both jobs proves that separation causes the unassignment.
    assert_eq!(separated_solution.unassigned.as_ref().map_or(0, |unassigned| unassigned.len()), 1);
}
