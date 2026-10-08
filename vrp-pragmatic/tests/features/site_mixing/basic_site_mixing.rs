use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;
use std::collections::{HashMap, HashSet};

/// Two neighbourhoods, each with one rider for school A and one for school B; the schools are next
/// to each other. Mixing schools is the cheapest plan, keeping each run to one school is not.
fn create_problem(objectives: Option<Vec<Objective>>) -> Problem {
    let job = |id: &str, home: (f64, f64), school: (f64, f64), site: &str| Job {
        site: Some(site.to_string()),
        ..create_pickup_delivery_job(id, home, school)
    };
    let (school_a, school_b) = ((10., 0.), (10., 1.));

    Problem {
        plan: Plan {
            jobs: vec![
                job("north_a", (2., 5.), school_a, "A"),
                job("north_b", (2., 5.), school_b, "B"),
                job("south_a", (2., -5.), school_a, "A"),
                job("south_b", (2., -5.), school_b, "B"),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: vec!["v1".to_string(), "v2".to_string()],
                capacity: Some(vec![2]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        objectives,
        ..create_empty_problem()
    }
}

/// Returns the sites served by each run (stretch with riders on board) of every tour.
fn get_run_sites(problem: &Problem, solution: &Solution) -> Vec<HashSet<String>> {
    let site_of =
        problem.plan.jobs.iter().map(|job| (job.id.clone(), job.site.clone().unwrap())).collect::<HashMap<_, _>>();

    solution
        .tours
        .iter()
        .flat_map(|tour| {
            let mut runs = vec![HashSet::new()];
            for stop in &tour.stops {
                for activity in stop.activities() {
                    if let Some(site) = site_of.get(&activity.job_id) {
                        runs.last_mut().unwrap().insert(site.clone());
                    }
                }
                if stop.load().iter().all(|load| *load == 0) {
                    runs.push(HashSet::new());
                }
            }
            runs.into_iter().filter(|sites| !sites.is_empty())
        })
        .collect()
}

#[test]
fn mixes_sites_when_only_cost_is_minimized() {
    let problem = create_problem(None);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert!(get_run_sites(&problem, &solution).iter().any(|sites| sites.len() > 1));
}

#[test]
fn keeps_each_run_to_one_site_when_site_mixing_is_minimized() {
    let problem = create_problem(Some(vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MinimizeSiteMixing,
        Objective::MinimizeCost,
    ]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert!(get_run_sites(&problem, &solution).iter().all(|sites| sites.len() == 1));
}

/// Two riders of the same school live on opposite sides of it, each next to a vehicle without an end
/// depot. Two short runs are the cheapest plan; one run serving the school once is not.
fn create_split_problem(objectives: Option<Vec<Objective>>) -> Problem {
    let job = |id: &str, home: (f64, f64)| Job {
        site: Some("S".to_string()),
        ..create_pickup_delivery_job(id, home, (10., 0.))
    };
    let vehicle = |id: &str, start: (f64, f64)| VehicleType {
        shifts: vec![VehicleShift {
            start: ShiftStart { location: start.to_loc(), ..create_default_open_vehicle_shift().start },
            ..create_default_open_vehicle_shift()
        }],
        ..create_default_vehicle(id)
    };

    Problem {
        plan: Plan { jobs: vec![job("west", (1., 0.)), job("east", (19., 0.))], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![vehicle("v_west", (0., 0.)), vehicle("v_east", (20., 0.))],
            ..create_default_fleet()
        },
        objectives,
        ..create_empty_problem()
    }
}

#[test]
fn splits_same_site_riders_when_only_cost_is_minimized() {
    let problem = create_split_problem(None);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(get_run_sites(&problem, &solution).len(), 2);
}

#[test]
fn serves_site_with_one_run_when_site_visits_are_minimized() {
    let problem = create_split_problem(Some(vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MinimizeSiteVisits,
        Objective::MinimizeCost,
    ]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(get_run_sites(&problem, &solution).len(), 1);
}

/// Three neighbourhoods, each with one rider for school A and one for school B; nothing sets `site`.
fn create_problem_without_sites(objectives: Option<Vec<Objective>>) -> Problem {
    let (school_a, school_b) = ((10., 0.), (10., 1.));
    let jobs = [(2., 5.), (2., 0.), (2., -5.)]
        .into_iter()
        .enumerate()
        .flat_map(|(idx, home)| {
            vec![
                create_pickup_delivery_job(&format!("a{idx}"), home, school_a),
                create_pickup_delivery_job(&format!("b{idx}"), home, school_b),
            ]
        })
        .collect();

    Problem {
        plan: Plan { jobs, ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: vec!["v1".to_string(), "v2".to_string(), "v3".to_string()],
                capacity: Some(vec![2]),
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        objectives,
        ..create_empty_problem()
    }
}

/// Schools of riders without `site`, taken from their delivery location.
fn get_run_schools(problem: &Problem, solution: &Solution) -> Vec<HashSet<String>> {
    let problem = Problem {
        plan: Plan {
            jobs: problem
                .plan
                .jobs
                .iter()
                .map(|job| Job { site: Some(job.id[..1].to_string()), ..job.clone() })
                .collect(),
            ..problem.plan.clone()
        },
        ..problem.clone()
    };

    get_run_sites(&problem, solution)
}

#[test]
fn mixes_schools_without_sites_when_only_cost_is_minimized() {
    let problem = create_problem_without_sites(None);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert!(get_run_schools(&problem, &solution).iter().any(|schools| schools.len() > 1));
}

#[test]
fn keeps_each_run_to_one_school_with_inferred_sites() {
    let problem = create_problem_without_sites(Some(vec![
        Objective::MinimizeUnassigned { breaks: None },
        Objective::MinimizeSiteMixing,
        Objective::MinimizeCost,
    ]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem.clone(), Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert!(get_run_schools(&problem, &solution).iter().all(|schools| schools.len() == 1));
}
