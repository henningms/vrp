use crate::format::problem::PragmaticProblem;
use crate::format::problem::*;
use crate::format::{CoordIndex, Location};
use crate::helpers::*;

const NEAR: (f64, f64) = (1., 0.);
const FAR: (f64, f64) = (2., 0.);

fn delivery(id: &str, location: (f64, f64), curb_side: Option<CurbSide>) -> Job {
    let mut job = create_delivery_job(id, location);
    job.deliveries.as_mut().unwrap()[0].places[0].curb_side = curb_side;
    job
}

/// An open tour from the depot past `near` to `far`: serving `near` first is the cheaper order. The
/// route from `near` to `far` arrives with `far` on the opposite side.
fn create_problem(far_curb_side: Option<CurbSide>, objectives: Vec<Objective>) -> (Problem, Matrix) {
    let problem = Problem {
        plan: Plan {
            jobs: vec![delivery("near", NEAR, Some(CurbSide::Prefer)), delivery("far", FAR, far_curb_side)],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![create_default_open_vehicle_shift()],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        objectives: Some(objectives),
        ..create_empty_problem()
    };

    let coord_index = CoordIndex::new(&problem);
    let index = |location: (f64, f64)| coord_index.get_by_loc(&location.to_loc()).unwrap();
    let size = coord_index.unique().len();
    let matrix = Matrix {
        opposite_side_arrivals: Some(vec![index(NEAR) * size + index(FAR)]),
        ..create_matrix_from_problem(&problem)
    };

    (problem, matrix)
}

fn lexicographic() -> Vec<Objective> {
    vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MinimizeOppositeSideArrivals,
        Objective::MinimizeCost,
    ]
}

fn weighted(weight: f64) -> Vec<Objective> {
    vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MultiObjective {
            strategy: MultiStrategy::WeightedSum { weights: vec![1., weight] },
            objectives: vec![Objective::MinimizeCost, Objective::MinimizeOppositeSideArrivals],
        },
    ]
}

fn served_order(problem: Problem, matrix: Matrix) -> Vec<String> {
    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);
    solution.tours[0]
        .stops
        .iter()
        .flat_map(|stop| stop.activities())
        .filter(|activity| activity.activity_type == "delivery")
        .map(|activity| activity.job_id.clone())
        .collect()
}

#[test]
fn serves_the_curb_side_order_when_opposite_side_arrivals_are_minimized() {
    let (problem, matrix) = create_problem(Some(CurbSide::Prefer), lexicographic());

    assert_eq!(served_order(problem, matrix), vec!["far", "near"]);
}

#[test]
fn counts_a_marked_cell_into_a_place_without_curb_side() {
    // the matrix builder decides which cells to mark, e.g. for a candidate job the problem lacks
    let (problem, matrix) = create_problem(None, lexicographic());

    assert_eq!(served_order(problem, matrix), vec!["far", "near"]);
}

#[test]
fn ignores_opposite_side_arrivals_without_the_objective() {
    let (problem, matrix) = create_problem(
        Some(CurbSide::Prefer),
        vec![Objective::MinimizeUnassigned { breaks: None }, Objective::MinimizeCost],
    );

    assert_eq!(served_order(problem, matrix), vec!["near", "far"]);
}

#[test]
fn prices_an_opposite_side_arrival_by_its_weight() {
    // serving `far` first drives one more unit of distance and time, which costs 2
    let (cheap, matrix) = create_problem(Some(CurbSide::Prefer), weighted(1.));
    assert_eq!(served_order(cheap, matrix), vec!["near", "far"]);

    let (dear, matrix) = create_problem(Some(CurbSide::Prefer), weighted(5.));
    assert_eq!(served_order(dear, matrix), vec!["far", "near"]);
}

#[test]
fn reads_curb_side_and_opposite_side_arrivals_from_json() {
    let place: JobPlace =
        serde_json::from_str(r#"{"location":{"lat":1,"lng":0},"duration":1,"curbSide":"prefer"}"#).unwrap();
    let matrix: Matrix =
        serde_json::from_str(r#"{"travelTimes":[0,1,1,0],"distances":[0,1,1,0],"oppositeSideArrivals":[1]}"#).unwrap();

    assert_eq!(place.curb_side, Some(CurbSide::Prefer));
    assert!(matches!(place.location, Location::Coordinate { .. }));
    assert_eq!(matrix.opposite_side_arrivals, Some(vec![1]));
    assert!(!serde_json::to_string(&create_job_place((1., 0.), None)).unwrap().contains("curbSide"));
}

#[test]
fn rejects_an_opposite_side_arrival_outside_the_matrix_when_reading() {
    let (problem, mut matrix) = create_problem(Some(CurbSide::Prefer), lexicographic());
    matrix.opposite_side_arrivals = Some(vec![matrix.travel_times.len()]);

    let Err(errors) = (problem, vec![matrix]).read_pragmatic() else { panic!("an invalid problem") };

    assert!(errors.errors.iter().any(|error| error.code == "E1506"), "{errors:?}");
}

#[test]
fn counts_an_arrival_at_a_shared_location_whichever_job_is_served_first() {
    // a second job at `far` without a curb side cannot hide the arrival of the curb-side one
    let (mut problem, matrix) = create_problem(Some(CurbSide::Prefer), lexicographic());
    problem.plan.jobs.push(delivery("far_too", FAR, None));

    let order = served_order(problem, matrix);

    assert_eq!(order.last().map(String::as_str), Some("near"), "{order:?}");
}

#[test]
fn counts_opposite_side_arrivals_on_a_matrix_with_duration_offsets() {
    // a constant second on depot -> near puts the transport behind the offsets without changing which order is cheaper
    let create_problem_with_offsets = |objectives| {
        let (problem, matrix) = create_problem(Some(CurbSide::Prefer), objectives);
        let coord_index = CoordIndex::new(&problem);
        let index = |location: (f64, f64)| coord_index.get_by_loc(&location.to_loc()).unwrap();
        let cells = [(index((0., 0.)), index(NEAR), 1)];
        let matrix = set_duration_offsets(matrix, vec![create_duration_offset_profile(&[(0., 1.)])], &cells);

        (problem, matrix)
    };

    let (problem, matrix) = create_problem_with_offsets(lexicographic());
    assert_eq!(served_order(problem, matrix), vec!["far", "near"]);

    let (problem, matrix) =
        create_problem_with_offsets(vec![Objective::MinimizeUnassigned { breaks: None }, Objective::MinimizeCost]);
    assert_eq!(served_order(problem, matrix), vec!["near", "far"]);
}
