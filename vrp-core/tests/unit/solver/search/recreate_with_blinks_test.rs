use super::*;
use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
use crate::helpers::models::domain::TestGoalContextBuilder;
use crate::helpers::models::problem::TestSingleBuilder;
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
