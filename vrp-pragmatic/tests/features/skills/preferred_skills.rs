use crate::format::problem::*;
use crate::format::solution::Solution;
use crate::helpers::*;

/// A job close to the depot of `near` and far from the depot of `far`, so that cost alone picks `near`.
fn create_problem(job_preferred: &[&[&str]], near_skills: &[&str], far_skills: &[&str]) -> Problem {
    let vehicle = |id: &str, location: (f64, f64), skills: &[&str]| VehicleType {
        shifts: vec![create_default_vehicle_shift_with_locations(location, location)],
        skills: if skills.is_empty() { None } else { Some(to_strings(skills.to_vec())) },
        ..create_default_vehicle(id)
    };

    Problem {
        plan: Plan {
            jobs: vec![Job {
                preferred_skills: Some(JobPreferredSkills {
                    one_of: Some(job_preferred.iter().map(|group| to_strings(group.to_vec())).collect()),
                }),
                ..create_delivery_job("job1", (1., 0.))
            }],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![vehicle("near", (0., 0.), near_skills), vehicle("far", (20., 0.), far_skills)],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

fn get_type_ids(solution: &Solution) -> Vec<&str> {
    solution.tours.iter().map(|tour| tour.type_id.as_str()).collect()
}

#[test]
fn can_prefer_vehicle_with_preferred_skill_over_cheaper_one() {
    let problem = create_problem(&[&["vehicle:far"]], &["vehicle:near"], &["vehicle:far"]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(get_type_ids(&solution), vec!["far"]);
}

#[test]
fn can_assign_job_to_cheaper_vehicle_when_no_vehicle_has_preferred_skill() {
    let problem = create_problem(&[&["vehicle:other"]], &["vehicle:near"], &["vehicle:far"]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(get_type_ids(&solution), vec!["near"]);
}

#[test]
fn can_prefer_vehicle_meeting_more_groups() {
    let problem = create_problem(&[&["driver:a"], &["vehicle:far"]], &["driver:a"], &["driver:a", "vehicle:far"]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(get_type_ids(&solution), vec!["far"]);
}

#[test]
fn can_prefer_vehicle_with_preferred_skill_with_explicit_objectives() {
    let problem = Problem {
        objectives: Some(vec![
            Objective::MinimizeUnassigned { breaks: None },
            Objective::MinimizeUnmetPreferredSkills,
            Objective::MinimizeCost,
        ]),
        ..create_problem(&[&["vehicle:far"]], &["vehicle:near"], &["vehicle:far"])
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(get_type_ids(&solution), vec!["far"]);
}

#[test]
fn can_read_flat_one_of_from_json_as_one_group() {
    let problem = create_problem(&[&["vehicle:far"]], &["vehicle:near"], &["vehicle:far"]);
    let mut problem = serde_json::to_value(problem).unwrap();
    problem["plan"]["jobs"][0]["preferredSkills"]["oneOf"] = serde_json::json!(["vehicle:other", "vehicle:far"]);
    let problem: Problem = serde_json::from_value(problem).unwrap();

    assert_eq!(
        problem.plan.jobs[0].preferred_skills.as_ref().and_then(|skills| skills.one_of.clone()),
        Some(vec![to_strings(vec!["vehicle:other", "vehicle:far"])])
    );
}
