use crate::format::problem::*;
use crate::format::solution::Tour;
use crate::helpers::*;

fn create_job_with_window(
    id: &str,
    pickup: (f64, f64),
    pickup_window: Vec<(i32, i32)>,
    delivery: (f64, f64),
    delivery_window: Vec<(i32, i32)>,
) -> Job {
    create_pickup_delivery_job_with_params(id, vec![1], (pickup, 0., pickup_window), (delivery, 0., delivery_window))
}

fn get_pickup_order(tour: &Tour) -> Vec<String> {
    tour.stops
        .iter()
        .flat_map(|stop| stop.activities().iter())
        .filter(|activity| activity.activity_type == "pickup")
        .map(|activity| activity.job_id.clone())
        .collect()
}

fn solve_single_vehicle(jobs: Vec<Job>, generations: usize) -> crate::format::solution::Solution {
    let problem = Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType { capacity: Some(vec![3]), ..create_default_vehicle_type() }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    solve_with_metaheuristic_and_iterations(problem, Some(vec![matrix]), generations)
}

// All three jobs go to one destination and may only be delivered late, so the vehicle cannot serve the limited job
// on its own first and come back for the others: the only way to board it without other pickups on the way is last.
fn create_shared_destination_jobs(limits: Option<RideStopLimits>) -> Vec<Job> {
    let delivery_window = vec![(21, 24)];
    let mut limited = create_job_with_window("a", (1., 0.), vec![], (10., 0.), delivery_window.clone());
    limited.ride_stop_limits = limits;
    vec![
        limited,
        create_job_with_window("b", (5., 0.), vec![], (10., 0.), delivery_window.clone()),
        create_job_with_window("c", (8., 0.), vec![], (10., 0.), delivery_window),
    ]
}

#[test]
fn can_serve_jobs_in_cheapest_order_without_limits() {
    let solution = solve_single_vehicle(create_shared_destination_jobs(None), 500);

    assert!(solution.unassigned.is_none());
    assert_eq!(get_pickup_order(&solution.tours[0]).first().map(String::as_str), Some("a"));
}

#[test]
fn can_pick_up_limited_job_last() {
    let limits = RideStopLimits { pickups: Some(0), deliveries: None, stops: None };

    let solution = solve_single_vehicle(create_shared_destination_jobs(Some(limits)), 3000);

    assert!(solution.unassigned.is_none());
    assert_eq!(get_pickup_order(&solution.tours[0]).last().map(String::as_str), Some("a"));
}

#[test]
fn can_unassign_job_due_to_ride_stop_limits() {
    // The tour order forces the second job to be picked up while the limited job is on board.
    let with_order = |mut job: Job, pickup_order: i32, delivery_order: i32| {
        job.pickups.as_mut().unwrap()[0].order = Some(pickup_order);
        job.deliveries.as_mut().unwrap()[0].order = Some(delivery_order);
        job
    };
    let jobs = |limits: Option<RideStopLimits>| {
        let mut limited = with_order(create_pickup_delivery_job("a", (1., 0.), (3., 0.)), 1, 3);
        limited.ride_stop_limits = limits;
        vec![limited, with_order(create_pickup_delivery_job("b", (2., 0.), (4., 0.)), 2, 4)]
    };

    let regular_solution = solve_single_vehicle(jobs(None), 500);
    let limited_solution =
        solve_single_vehicle(jobs(Some(RideStopLimits { pickups: Some(0), deliveries: None, stops: None })), 500);

    assert!(regular_solution.unassigned.is_none(), "both jobs fit without limits");
    assert_eq!(limited_solution.unassigned.as_ref().map_or(0, |unassigned| unassigned.len()), 1);
}
