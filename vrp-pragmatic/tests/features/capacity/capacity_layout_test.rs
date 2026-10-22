use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

type Trip = ((f64, f64), (f64, f64));

/// A minibus with layouts all_seated [8, 0] and one_wheelchair [4, 1], depot at (0, 0), and two trips: a wheelchair
/// passenger [0, 1] and a group of six seated passengers [6, 0], each from pickup to delivery location.
fn create_minibus_problem(wheelchair: Trip, seated: Trip) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![
                create_pickup_delivery_job_with_params(
                    "wheelchair_passenger",
                    vec![0, 1],
                    (wheelchair.0, 60., vec![]),
                    (wheelchair.1, 60., vec![]),
                ),
                create_pickup_delivery_job_with_params(
                    "seated_group",
                    vec![6, 0],
                    (seated.0, 60., vec![]),
                    (seated.1, 60., vec![]),
                ),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
                    CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![4, 1] },
                ]),
                ..create_vehicle_with_capacity("minibus", vec![])
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
        },
        ..create_empty_problem()
    }
}

fn get_single_tour(solution: Solution) -> Tour {
    assert!(solution.unassigned.is_none(), "expected all jobs to be assigned");
    solution.tours.into_iter().next().expect("expected a tour")
}

fn get_stop_layouts(tour: &Tour) -> Vec<Option<usize>> {
    tour.stops.iter().map(|stop| stop.layout().map(|layout| layout.index)).collect()
}

fn span(index: usize, name: &str, from_stop: usize, to_stop: usize) -> CapacityLayoutSpan {
    CapacityLayoutSpan { index, name: Some(name.to_string()), from_stop, to_stop }
}

/// The shortest feasible tour is (0,0) → (1,0) → (2,0) → (3,0) → (4,0) → (0,0): carrying the wheelchair passenger
/// past (3,0) would need [6, 1], which fits no layout. The vehicle is empty after (2,0), so it changes to all_seated
/// at (3,0) without passengers aboard.
#[test]
fn can_report_capacity_layouts_of_tour() {
    let problem = create_minibus_problem(((1., 0.), (2., 0.)), ((3., 0.), (4., 0.)));
    let matrix = create_matrix_from_problem(&problem);

    let tour = get_single_tour(solve_with_metaheuristic(problem, Some(vec![matrix])));

    assert_eq!(
        get_ids_from_tour(&tour),
        vec![
            vec!["departure"],
            vec!["wheelchair_passenger"],
            vec!["wheelchair_passenger"],
            vec!["seated_group"],
            vec!["seated_group"],
            vec!["arrival"],
        ]
    );
    assert_eq!(get_stop_layouts(&tour), vec![Some(1), Some(1), Some(1), Some(0), Some(0), Some(0)]);
    assert_eq!(tour.capacity_layouts, Some(vec![span(1, "one_wheelchair", 0, 2), span(0, "all_seated", 3, 5)]));
}

/// The wheelchair passenger boards at the depot, so the departure stop has the departure and the pickup, and it
/// leaves in one_wheelchair.
#[test]
fn can_report_capacity_layout_of_departure_stop_with_pickup() {
    let problem = create_minibus_problem(((0., 0.), (1., 0.)), ((2., 0.), (3., 0.)));
    let matrix = create_matrix_from_problem(&problem);

    // NOTE the load checker does not support a departure stop with other activities yet, see the ignored
    // can_check_load_when_departure_has_other_activity, so this solution is not checked
    let solution = solve_with_metaheuristic_and_iterations_without_check(problem, Some(vec![matrix]), 200);
    let tour = get_single_tour(solution);

    assert_eq!(
        get_ids_from_tour(&tour),
        vec![
            vec!["departure", "wheelchair_passenger"],
            vec!["wheelchair_passenger"],
            vec!["seated_group"],
            vec!["seated_group"],
            vec!["arrival"],
        ]
    );
    assert_eq!(get_stop_layouts(&tour), vec![Some(1), Some(1), Some(0), Some(0), Some(0)]);
    assert_eq!(tour.capacity_layouts, Some(vec![span(1, "one_wheelchair", 0, 1), span(0, "all_seated", 2, 4)]));
}
