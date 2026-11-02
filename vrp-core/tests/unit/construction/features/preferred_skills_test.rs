use super::*;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::models::problem::Single;
use std::sync::Arc;

fn to_groups(groups: Vec<Vec<&str>>) -> Vec<Vec<String>> {
    groups.iter().map(|group| group.iter().map(|s| s.to_string()).collect()).collect()
}

fn create_single(one_of: Option<Vec<Vec<&str>>>) -> Arc<Single> {
    let mut builder = TestSingleBuilder::default();
    if let Some(one_of) = one_of {
        builder.dimens_mut().set_job_preferred_skills(JobPreferredSkills::new(to_groups(one_of)));
    }

    builder.build_shared()
}

fn create_route_ctx(vehicle_skills: Option<Vec<&str>>, singles: &[Arc<Single>]) -> RouteContext {
    let mut vehicle = TestVehicleBuilder::default();
    if let Some(skills) = vehicle_skills {
        vehicle.dimens_mut().set_vehicle_skills(skills.iter().map(|s| s.to_string()).collect::<HashSet<_>>());
    }
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle.id("v1").build()).build();

    let mut route = RouteBuilder::default();
    route.with_vehicle(&fleet, "v1");
    singles.iter().for_each(|single| {
        route.add_activity(ActivityBuilder::with_location(1).job(Some(single.clone())).build());
    });

    RouteContextBuilder::default().with_route(route.build()).build()
}

fn create_objective() -> Arc<dyn FeatureObjective> {
    create_preferred_skills_feature("preferred_skills").unwrap().objective.unwrap()
}

parameterized_test! {can_estimate_unmet_preferred_skills, (one_of, vehicle_skills, expected), {
    can_estimate_unmet_preferred_skills_impl(one_of, vehicle_skills, expected);
}}

can_estimate_unmet_preferred_skills! {
    case_no_preference: (None, Some(vec!["s1"]), 0.),
    case_met: (Some(vec![vec!["s1"]]), Some(vec!["s1"]), 0.),
    case_met_by_any_skill_of_group: (Some(vec![vec!["s1", "s2"]]), Some(vec!["s2"]), 0.),
    case_unmet: (Some(vec![vec!["s1"]]), Some(vec!["s2"]), 1.),
    case_unmet_without_vehicle_skills: (Some(vec![vec!["s1"]]), None, 1.),
    case_counts_every_unmet_group: (Some(vec![vec!["s1"], vec!["s2"], vec!["s3"]]), Some(vec!["s2"]), 2.),
}

fn can_estimate_unmet_preferred_skills_impl(
    one_of: Option<Vec<Vec<&str>>>,
    vehicle_skills: Option<Vec<&str>>,
    expected: Cost,
) {
    let route_ctx = create_route_ctx(vehicle_skills, &[]);
    let solution_ctx = TestInsertionContextBuilder::default().build().solution;
    let job = Job::Single(create_single(one_of));

    let estimate = create_objective().estimate(&MoveContext::route(&solution_ctx, &route_ctx, &job));

    assert_eq!(estimate, expected);
}

#[test]
fn can_sum_unmet_preferred_skills_over_routes() {
    let met = create_single(Some(vec![vec!["s1"]]));
    let unmet = create_single(Some(vec![vec!["s2"], vec!["s3"]]));
    let without = create_single(None);
    let other_unmet = create_single(Some(vec![vec!["s1"]]));
    let insertion_ctx = TestInsertionContextBuilder::default()
        .with_routes(vec![
            create_route_ctx(Some(vec!["s1"]), &[met, unmet, without]),
            create_route_ctx(Some(vec!["s2"]), &[other_unmet]),
        ])
        .build();

    assert_eq!(create_objective().fitness(&insertion_ctx), 3.);
}

#[test]
fn can_ignore_empty_groups() {
    let preferred_skills = JobPreferredSkills::new(to_groups(vec![vec![], vec!["s1"]]));

    assert_eq!(preferred_skills.count_unmet(None), 1);
}
