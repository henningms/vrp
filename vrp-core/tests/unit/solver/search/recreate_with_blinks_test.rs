use super::*;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::domain::TestGoalContextBuilder;
use crate::helpers::models::problem::{TestSingleBuilder, test_multi_with_id};
use crate::helpers::models::solution::{ActivityBuilder, RouteBuilder, RouteContextBuilder};
use crate::helpers::utils::random::FakeRandom;
use crate::models::{ConstraintViolation, FeatureBuilder, FeatureConstraint, ViolationCode};
use std::sync::atomic::{AtomicUsize, Ordering};

struct StopAfterFirstPosition {
    activity_checks: Arc<AtomicUsize>,
}

impl FeatureConstraint for StopAfterFirstPosition {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Route { .. } => None,
            MoveContext::Activity { .. } => {
                self.activity_checks.fetch_add(1, Ordering::Relaxed);
                ConstraintViolation::fail(ViolationCode(42))
            }
        }
    }
}

#[test]
fn stops_evaluating_route_legs_after_terminal_failure() {
    let activity_checks = Arc::new(AtomicUsize::new(0));
    let stop_feature = FeatureBuilder::default()
        .with_name("stop-after-first-position")
        .with_constraint(StopAfterFirstPosition { activity_checks: activity_checks.clone() })
        .build()
        .unwrap();
    let goal = TestGoalContextBuilder::default().add_feature(stop_feature).build();
    let route = RouteBuilder::default().add_activities((0..3).map(|_| ActivityBuilder::default().build())).build();
    let route_ctx = RouteContextBuilder::default().with_route(route).build();
    let insertion_ctx = TestInsertionContextBuilder::default().with_goal(goal).with_routes(vec![route_ctx]).build();
    let job = TestSingleBuilder::default().build_as_job_ref();
    let routes = insertion_ctx.solution.routes.iter().collect::<Vec<_>>();
    let random = Arc::new(FakeRandom::new(vec![], vec![1.; 8]));
    let evaluator = BlinkInsertionEvaluator::new(0.01, random);

    let result = evaluator.evaluate_all(
        &insertion_ctx,
        &[&job],
        &routes,
        &LegSelection::Exhaustive,
        &BestResultSelector::default(),
    );

    assert!(matches!(result, InsertionResult::Failure(InsertionFailure { stopped: true, .. })));
    assert_eq!(activity_checks.load(Ordering::Relaxed), 1);
}

#[test]
fn evaluates_only_first_non_blinked_suffix_for_exhaustive_multi_job() {
    let goal = TestGoalContextBuilder::with_transport_feature().build();
    let route = RouteBuilder::default()
        .add_activities([5, 10, 15].into_iter().map(|location| ActivityBuilder::with_location(location).build()))
        .build();
    let route_ctx = RouteContextBuilder::default().with_route(route).build();
    let insertion_ctx = TestInsertionContextBuilder::default().with_goal(goal).with_routes(vec![route_ctx]).build();
    let job = Job::Multi(test_multi_with_id(
        "multi",
        vec![
            TestSingleBuilder::default().location(Some(3)).build_shared(),
            TestSingleBuilder::default().location(Some(7)).build_shared(),
        ],
    ));
    let routes = insertion_ctx.solution.routes.iter().collect::<Vec<_>>();
    let leg_selection = LegSelection::Exhaustive;
    let result_selector = BestResultSelector::default();
    let eval_ctx = EvaluationContext {
        goal: &insertion_ctx.problem.goal,
        job: &job,
        leg_selection: &leg_selection,
        result_selector: &result_selector,
    };
    let route_ctx = routes[0];
    let route_costs = eval_ctx.goal.estimate(&MoveContext::route(&insertion_ctx.solution, route_ctx, &job));
    let expected = eval_job_constraint_in_route(
        &eval_ctx,
        &insertion_ctx.solution,
        route_ctx,
        InsertionPosition::Concrete(1),
        route_costs,
        None,
    );
    // Blink past position zero, then evaluate position one's exhaustive suffix.
    // Exactly two random values prove the outer loop stops at that first evaluated
    // suffix; before this optimization, later suffixes exhaust the fake distribution.
    let random = Arc::new(FakeRandom::new(vec![], vec![0., 1.]));
    let evaluator = BlinkInsertionEvaluator::new(0.01, random);

    let result = evaluator.evaluate_all(&insertion_ctx, &[&job], &routes, &leg_selection, &result_selector);

    let success = result.as_success().expect("expected a feasible multi-job insertion");
    let expected = expected.as_success().expect("expected the first suffix to be feasible");
    let summarize = |success: &InsertionSuccess| {
        success
            .activities
            .iter()
            .map(|(activity, index)| (*index, activity.place.idx, activity.place.location))
            .collect::<Vec<_>>()
    };
    assert_eq!(success.cost, expected.cost);
    assert_eq!(summarize(success), summarize(expected));
    assert_eq!(success.activities.len(), 2);
}
