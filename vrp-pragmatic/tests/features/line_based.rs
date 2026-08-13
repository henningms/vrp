use crate::format::feasibility::{FeasibilityContext, job_locations};
use crate::format::problem::*;
use crate::format::solution::Solution;
use crate::format::{CoordIndex, Location};
use crate::format_time;
use crate::helpers::*;

fn create_line_task(
    location: (f64, f64),
    tag: &str,
    order: i32,
    time_window: (i32, i32),
    demand: Option<Vec<i32>>,
) -> JobTask {
    JobTask {
        places: vec![JobPlace {
            location: location.to_loc(),
            duration: 1.,
            times: Some(vec![vec![format_time(time_window.0 as f64), format_time(time_window.1 as f64)]]),
            tag: Some(tag.to_string()),
            requested_time: None,
        }],
        demand,
        named_demand: None,
        order: Some(order),
    }
}

fn create_passenger(id: &str, pickup: ((f64, f64), i32, (i32, i32)), delivery: ((f64, f64), i32, (i32, i32))) -> Job {
    Job {
        pickups: Some(vec![create_line_task(pickup.0, "pickup", pickup.1, pickup.2, Some(vec![1]))]),
        deliveries: Some(vec![create_line_task(delivery.0, "delivery", delivery.1, delivery.2, Some(vec![1]))]),
        ..create_job(id)
    }
}

fn create_control_stop() -> Job {
    Job {
        services: Some(vec![create_line_task((5., 0.), "control", 5_000, (490, 510), None)]),
        ..create_job("control-stop")
    }
}

fn create_early_passenger() -> Job {
    create_passenger("early", ((2., 0.), 2_000, (190, 210)), ((4., 0.), 4_000, (200, 450)))
}

fn create_late_passenger() -> Job {
    create_passenger("late", ((8., 0.), 8_000, (790, 810)), ((9., 0.), 9_000, (800, 950)))
}

fn create_scheduled_run(jobs: Vec<Job>) -> Problem {
    Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    start: ShiftStart {
                        earliest: format_time(0.),
                        latest: Some(format_time(0.)),
                        location: (0., 0.).to_loc(),
                    },
                    end: Some(ShiftEnd { earliest: None, latest: format_time(1_000.), location: (10., 0.).to_loc() }),
                    breaks: None,
                    reloads: None,
                    recharges: None,
                    required_stops: None,
                    via: None,
                }],
                ..create_vehicle_with_capacity("line", vec![8])
            }],
            ..create_default_fleet()
        },
        objectives: None,
    }
}

fn activities(solution: &Solution) -> Vec<(String, String)> {
    solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities())
        .filter(|activity| activity.job_id != "departure" && activity.job_id != "arrival")
        .map(|activity| (activity.job_id.clone(), activity.activity_type.clone()))
        .collect()
}

fn create_matrix_with_extra_locations(problem: &Problem, extra_locations: &[Location]) -> Matrix {
    let unique = CoordIndex::new_with_extra_locations(problem, extra_locations).unique();
    let data = unique
        .iter()
        .cloned()
        .flat_map(|from| {
            let (from_lat, from_lng) = from.to_lat_lng();
            unique.iter().map(move |to| {
                let (to_lat, to_lng) = to.to_lat_lng();
                ((from_lat - to_lat).powi(2) + (from_lng - to_lng).powi(2)).sqrt().round() as i64
            })
        })
        .collect();

    create_matrix(data)
}

#[test]
fn full_solve_uses_line_position_instead_of_booking_arrival_order() {
    // The late passenger deliberately appears first in the problem input.
    let problem = create_scheduled_run(vec![create_late_passenger(), create_control_stop(), create_early_passenger()]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.as_ref().is_none_or(Vec::is_empty));
    assert_eq!(
        activities(&solution),
        vec![
            ("early".to_string(), "pickup".to_string()),
            ("early".to_string(), "delivery".to_string()),
            ("control-stop".to_string(), "service".to_string()),
            ("late".to_string(), "pickup".to_string()),
            ("late".to_string(), "delivery".to_string()),
        ]
    );
    assert_eq!(solution.tours[0].stops[0].schedule().departure, format_time(0.));
}

#[test]
fn incremental_check_can_insert_early_booking_after_late_booking_was_solved_first() {
    let base_problem = create_scheduled_run(vec![create_control_stop(), create_late_passenger()]);
    let base_matrix = create_matrix_from_problem(&base_problem);
    let base_solution = solve_with_metaheuristic(base_problem.clone(), Some(vec![base_matrix]));
    let base_solution_json = serde_json::to_string(&base_solution).expect("cannot serialize base solution");

    let early = create_early_passenger();
    let extra_locations = job_locations(&early);
    let feasibility_matrix = create_matrix_with_extra_locations(&base_problem, &extra_locations);
    let mut context =
        FeasibilityContext::new(base_problem, vec![feasibility_matrix], &base_solution_json, &extra_locations)
            .expect("cannot create feasibility context");

    let check = context.check_job(&early).expect("cannot check early booking");
    assert!(check.is_feasible, "an early booking must be insertable before existing later activities");

    context.accept_job(&early).expect("cannot accept early booking");
    let accepted: Solution =
        serde_json::from_str(&context.to_solution_json().expect("cannot serialize accepted solution"))
            .expect("cannot deserialize accepted solution");

    assert_eq!(
        activities(&accepted),
        vec![
            ("early".to_string(), "pickup".to_string()),
            ("early".to_string(), "delivery".to_string()),
            ("control-stop".to_string(), "service".to_string()),
            ("late".to_string(), "pickup".to_string()),
            ("late".to_string(), "delivery".to_string()),
        ]
    );
}

#[test]
fn passenger_whose_delivery_is_behind_pickup_is_rejected() {
    let backwards = create_passenger("backwards", ((8., 0.), 8_000, (700, 800)), ((2., 0.), 2_000, (700, 900)));
    let problem = create_scheduled_run(vec![create_control_stop(), backwards]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(solution.unassigned.as_ref().expect("backwards passenger should be unassigned")[0].job_id, "backwards");
    assert_eq!(activities(&solution), vec![("control-stop".to_string(), "service".to_string())]);
}
