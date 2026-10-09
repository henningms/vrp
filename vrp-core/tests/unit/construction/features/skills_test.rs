use super::*;

use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};

const VIOLATION_CODE: ViolationCode = ViolationCode(1);

fn create_job_with_skills(
    all_of: Option<Vec<&str>>,
    one_of: Option<Vec<Vec<&str>>>,
    none_of: Option<Vec<&str>>,
) -> Job {
    let mut builder = TestSingleBuilder::default();
    builder.dimens_mut().set_job_skills(JobSkills {
        all_of: all_of.map(|skills| skills.iter().map(|s| s.to_string()).collect()),
        one_of: one_of.map(|groups| groups.iter().map(|group| group.iter().map(|s| s.to_string()).collect()).collect()),
        none_of: none_of.map(|skills| skills.iter().map(|s| s.to_string()).collect()),
    });

    builder.build_as_job_ref()
}

fn create_vehicle_with_skills(skills: Option<Vec<&str>>) -> Vehicle {
    let mut builder = TestVehicleBuilder::default();

    if let Some(skills) = skills {
        let skills: HashSet<String> = HashSet::from_iter(skills.iter().map(|s| s.to_string()));
        builder.dimens_mut().set_vehicle_skills(skills);
    }

    builder.id("v1").build()
}

fn failure() -> Option<ConstraintViolation> {
    ConstraintViolation::fail(VIOLATION_CODE)
}

parameterized_test! {can_check_skills, (all_of, one_of, none_of, vehicle_skills, expected), {
    can_check_skills_impl(all_of, one_of, none_of, vehicle_skills, expected);
}}

can_check_skills! {
    case01: (None, None, None, None, None),

    case_all_of_01: (Some(vec!["s1"]), None, None, None, failure()),
    case_all_of_02: (Some(vec![]), None, None, None, None),
    case_all_of_03: (Some(vec!["s1"]), None, None, Some(vec!["s1"]), None),
    case_all_of_04: (Some(vec!["s1"]), None, None, Some(vec!["s2"]), failure()),
    case_all_of_05: (Some(vec!["s1", "s2"]), None, None, Some(vec!["s2"]), failure()),
    case_all_of_06: (Some(vec!["s1"]), None, None, Some(vec!["s1", "s2"]), None),

    case_one_of_01: (None, Some(vec![vec!["s1"]]), None, None, failure()),
    case_one_of_02: (None, Some(vec![]), None, None, None),
    case_one_of_03: (None, Some(vec![vec!["s1"]]), None, Some(vec!["s1"]), None),
    case_one_of_04: (None, Some(vec![vec!["s1"]]), None, Some(vec!["s2"]), failure()),
    case_one_of_05: (None, Some(vec![vec!["s1", "s2"]]), None, Some(vec!["s2"]), None),
    case_one_of_06: (None, Some(vec![vec!["s1"]]), None, Some(vec!["s1", "s2"]), None),
    case_one_of_07: (None, Some(vec![vec![]]), None, Some(vec!["s1"]), failure()),

    case_one_of_groups_01: (None, Some(vec![vec!["s1", "s2"], vec!["s3", "s4"]]), None, Some(vec!["s1", "s3"]), None),
    case_one_of_groups_02: (None, Some(vec![vec!["s1", "s2"], vec!["s3", "s4"]]), None, Some(vec!["s1", "s2"]), failure()),
    case_one_of_groups_03: (None, Some(vec![vec!["s1", "s2"], vec!["s3", "s4"]]), None, Some(vec!["s4"]), failure()),
    case_one_of_groups_04: (None, Some(vec![vec!["s1"], vec!["s1", "s2"]]), None, Some(vec!["s1"]), None),
    case_one_of_groups_05: (None, Some(vec![vec!["s1"], vec![]]), None, Some(vec!["s1"]), failure()),

    case_none_of_01: (None, None, Some(vec!["s1"]), None, None),
    case_none_of_02: (None, None, Some(vec![]), None, None),
    case_none_of_03: (None, None, Some(vec!["s1"]), Some(vec!["s1"]), failure()),
    case_none_of_04: (None, None, Some(vec!["s1"]), Some(vec!["s2"]), None),
    case_none_of_05: (None, None, Some(vec!["s1", "s2"]), Some(vec!["s2"]), failure()),
    case_none_of_06: (None, None, Some(vec!["s1"]), Some(vec!["s1", "s2"]), failure()),

    case_combine_01: (Some(vec!["s1"]), None, Some(vec!["s2"]), Some(vec!["s1", "s2"]), failure()),
    case_combine_02: (None, Some(vec![vec!["s1"]]), Some(vec!["s2"]), Some(vec!["s1", "s2"]), failure()),
    case_combine_03: (Some(vec!["s1"]), Some(vec![vec!["s2"]]), None, Some(vec!["s1", "s2"]), None),
    case_combine_04: (Some(vec!["s1"]), Some(vec![vec!["s2", "s3"]]), None, Some(vec!["s1", "s2"]), None),
    case_combine_05: (Some(vec!["s1", "s2"]), Some(vec![vec!["s3"]]), None, Some(vec!["s1", "s2", "s3"]), None),
    case_combine_06: (Some(vec!["s1", "s2"]), Some(vec![vec!["s3"]]), None, Some(vec!["s1", "s2"]), failure()),
    case_combine_07: (Some(vec!["s1"]), Some(vec![vec!["s2"]]), Some(vec!["s3"]), Some(vec!["s1", "s2", "s3"]), failure()),
    case_combine_08: (Some(vec!["s1"]), Some(vec![vec!["s2", "s3"], vec!["s4"]]), None, Some(vec!["s1", "s3", "s4"]), None),
    case_combine_09: (None, Some(vec![vec!["s1"], vec!["s2"]]), Some(vec!["s3"]), Some(vec!["s1", "s2", "s3"]), failure()),
}

fn can_check_skills_impl(
    all_of: Option<Vec<&str>>,
    one_of: Option<Vec<Vec<&str>>>,
    none_of: Option<Vec<&str>>,
    vehicle_skills: Option<Vec<&str>>,
    expected: Option<ConstraintViolation>,
) {
    let fleet = FleetBuilder::default()
        .add_driver(test_driver())
        .add_vehicle(create_vehicle_with_skills(vehicle_skills))
        .build();
    let route_ctx =
        RouteContextBuilder::default().with_route(RouteBuilder::default().with_vehicle(&fleet, "v1").build()).build();

    let constraint = create_skills_feature("skills", VIOLATION_CODE).unwrap().constraint.unwrap();

    let actual = constraint.evaluate(&MoveContext::route(
        &TestInsertionContextBuilder::default().build().solution,
        &route_ctx,
        &create_job_with_skills(all_of, one_of, none_of),
    ));

    assert_eq!(actual, expected)
}

parameterized_test! {can_merge_skills, (source, candidate, expected), {
    can_merge_skills_impl(source, candidate, expected);
}}

can_merge_skills! {
    case_01: (create_job_with_skills(None, None, None), create_job_with_skills(None, None, None), Ok(())),

    case_02: (create_job_with_skills(Some(vec!["skill"]), None, None), create_job_with_skills(None, None, None), Ok(())),
    case_03: (create_job_with_skills(None, Some(vec![vec!["skill"]]), None), create_job_with_skills(None, None, None), Ok(())),
    case_04: (create_job_with_skills(None, None, Some(vec!["skill"])), create_job_with_skills(None, None, None), Ok(())),

    case_05: (create_job_with_skills(None, None, None), create_job_with_skills(Some(vec!["skill"]), None, None), Err(VIOLATION_CODE)),
    case_06: (create_job_with_skills(None, None, None), create_job_with_skills(None, Some(vec![vec!["skill"]]), None), Err(VIOLATION_CODE)),
    case_07: (create_job_with_skills(None, None, None), create_job_with_skills(None, None, Some(vec!["skill"])), Err(VIOLATION_CODE)),

    case_08: (create_job_with_skills(Some(vec!["skill"]), None, None), create_job_with_skills(Some(vec!["skill"]), None, None), Ok(())),
    case_09: (create_job_with_skills(Some(vec!["skill"]), None, None), create_job_with_skills(None, Some(vec![vec!["skill"]]), None), Ok(())),
    case_10: (create_job_with_skills(Some(vec!["skill1", "skill2"]), None, None), create_job_with_skills(Some(vec!["skill1"]), None, None), Ok(())),
    case_11: (create_job_with_skills(Some(vec!["skill1"]), None, None), create_job_with_skills(Some(vec!["skill1", "skill2"]), None, None), Err(VIOLATION_CODE)),

    case_12: (create_job_with_skills(None, Some(vec![vec!["skill1", "skill2"]]), None), create_job_with_skills(None, Some(vec![vec!["skill1"]]), None), Err(VIOLATION_CODE)),
    case_13: (create_job_with_skills(None, Some(vec![vec!["skill1"]]), None), create_job_with_skills(None, Some(vec![vec!["skill1", "skill2"]]), None), Ok(())),
    case_14: (create_job_with_skills(Some(vec!["skill2"]), None, None), create_job_with_skills(None, Some(vec![vec!["skill1", "skill2"]]), None), Ok(())),
    case_15: (create_job_with_skills(None, Some(vec![vec!["skill1"], vec!["skill3"]]), None), create_job_with_skills(None, Some(vec![vec!["skill3", "skill4"]]), None), Ok(())),
    case_16: (create_job_with_skills(None, Some(vec![vec!["skill1"]]), None), create_job_with_skills(None, Some(vec![vec!["skill1"], vec!["skill3"]]), None), Err(VIOLATION_CODE)),
    case_17: (create_job_with_skills(None, None, Some(vec!["skill1", "skill2"])), create_job_with_skills(None, None, Some(vec!["skill1"])), Ok(())),
    case_18: (create_job_with_skills(None, None, Some(vec!["skill1"])), create_job_with_skills(None, None, Some(vec!["skill1", "skill2"])), Err(VIOLATION_CODE)),
}

fn can_merge_skills_impl(source: Job, candidate: Job, expected: Result<(), ViolationCode>) {
    let constraint = create_skills_feature("skills", VIOLATION_CODE).unwrap().constraint.unwrap();

    let result = constraint.merge(source, candidate).map(|_| ());

    assert_eq!(result, expected);
}

#[test]
fn can_create_empty_skills_as_none() {
    let skills = JobSkills::new(Some(vec![]), Some(vec![]), Some(vec![]));

    assert!(skills.all_of.is_none());
    assert!(skills.one_of.is_none());
    assert!(skills.none_of.is_none());
}

#[test]
fn can_keep_empty_one_of_group_as_unsatisfiable() {
    let skills = JobSkills::new(None, Some(vec![vec!["s1".to_string()], vec![]]), None);

    assert_eq!(skills.one_of.as_ref().map(|groups| groups.len()), Some(2));
    assert!(!skills.is_satisfied_by(Some(&HashSet::from(["s1".to_string()]))));
}
