use crate::format::problem::*;
use crate::helpers::*;

fn create_problem_with_vehicle_skills(vehicles: &[(&str, &[&str])]) -> Problem {
    let skills = JobSkills {
        all_of: None,
        one_of: Some(vec![to_strings(vec!["driver:a", "driver:b"]), to_strings(vec!["ramp", "lift"])]),
        none_of: None,
    };

    Problem {
        plan: Plan { jobs: vec![create_delivery_job_with_skills("job1", (1., 0.), skills)], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vehicles
                .iter()
                .map(|(id, skills)| VehicleType {
                    skills: Some(to_strings(skills.to_vec())),
                    ..create_default_vehicle(id)
                })
                .collect(),
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

#[test]
fn can_assign_job_only_to_vehicle_meeting_every_one_of_group() {
    let problem = create_problem_with_vehicle_skills(&[
        ("driver_only", &["driver:a"]),
        ("lift_only", &["lift"]),
        ("driver_and_lift", &["driver:b", "lift"]),
    ]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.iter().map(|tour| tour.type_id.as_str()).collect::<Vec<_>>(), vec!["driver_and_lift"]);
}

#[test]
fn can_have_unassigned_when_no_vehicle_meets_every_one_of_group() {
    let problem = create_problem_with_vehicle_skills(&[("driver_only", &["driver:a"]), ("lift_only", &["lift"])]);
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.tours.is_empty());
    assert_eq!(
        solution.unassigned.map(|jobs| jobs
            .into_iter()
            .flat_map(|job| job.reasons)
            .map(|r| r.code)
            .collect::<Vec<_>>()),
        Some(vec!["SKILL_CONSTRAINT".to_string()])
    );
}
