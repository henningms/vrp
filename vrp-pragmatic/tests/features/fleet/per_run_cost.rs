use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

const PER_RUN: f64 = 1000.;

/// Two riders on one line in front of a vehicle without an end depot: one rides from 1 to 5, the
/// other from 6 to 10. Serving them back to back is the shortest plan; carrying both between 6 and 5
/// drives two units more but needs one run instead of two.
fn create_problem(per_run: Option<f64>) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![
                create_pickup_delivery_job("first", (1., 0.), (5., 0.)),
                create_pickup_delivery_job("second", (6., 0.), (10., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                costs: VehicleCosts { per_run, ..create_default_vehicle_costs() },
                shifts: vec![create_default_open_vehicle_shift()],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

/// Counts the runs of a tour: every time the vehicle gets empty again after carrying riders.
fn count_runs(tour: &Tour) -> usize {
    let (runs, _) = tour.stops.iter().fold((0, false), |(runs, was_loaded), stop| {
        let is_loaded = stop.load().iter().any(|load| *load > 0);
        (if was_loaded && !is_loaded { runs + 1 } else { runs }, is_loaded)
    });

    runs
}

/// The default vehicle has a fixed cost of 10 and costs 1 per distance and time unit.
fn get_expected_cost(tour: &Tour, per_run: f64) -> f64 {
    10. + tour.statistic.distance as f64 + tour.statistic.duration as f64 + per_run * count_runs(tour) as f64
}

#[test]
fn serves_riders_back_to_back_without_per_run_cost() {
    let problem = create_problem(None);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(count_runs(tour), 2);
    assert_eq!(tour.statistic.distance, 10);
    assert_eq!(tour.statistic.cost, get_expected_cost(tour, 0.));
}

#[test]
fn shares_one_run_when_runs_are_expensive() {
    let problem = create_problem(Some(PER_RUN));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    let tour = &solution.tours[0];
    assert_eq!(count_runs(tour), 1);
    assert_eq!(tour.statistic.distance, 12);
    assert_eq!(tour.statistic.cost, get_expected_cost(tour, PER_RUN));
    assert_eq!(solution.statistic.cost, tour.statistic.cost);
}

#[test]
fn zero_per_run_cost_is_the_same_as_none() {
    let solve = |per_run| {
        let problem = create_problem(per_run);
        let matrix = create_matrix_from_problem(&problem);
        solve_with_metaheuristic(problem, Some(vec![matrix]))
    };

    assert_eq!(solve(Some(0.)), solve(None));
}

#[test]
fn per_run_cost_is_read_and_written_as_per_run() {
    let costs: VehicleCosts =
        serde_json::from_str(r#"{"fixed": 10, "distance": 1, "time": 1, "perRun": 50}"#).expect("cannot read costs");
    assert_eq!(costs.per_run, Some(50.));
    assert!(serde_json::to_string(&costs).unwrap().contains(r#""perRun":50.0"#));

    let costs: VehicleCosts = serde_json::from_str(r#"{"distance": 1, "time": 1}"#).expect("cannot read costs");
    assert_eq!(costs.per_run, None);
    assert!(!serde_json::to_string(&costs).unwrap().contains("perRun"));
}
