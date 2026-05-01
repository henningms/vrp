use super::*;
use crate::helpers::models::solution::RouteContextBuilder;
use crate::helpers::solver::generate_matrix_routes_with_defaults;
use crate::helpers::utils::random::FakeRandom;
use crate::models::common::Cost;
use std::sync::Arc;

mod unassigned_job_selector {
    use super::*;
    use crate::construction::heuristics::{InsertionContext, JobSelector, UnassignedJobSelector, UnassignmentInfo};
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::helpers::models::problem::TestSingleBuilder;

    #[test]
    fn select_returns_only_unassigned_jobs() {
        let assigned = TestSingleBuilder::default().id("assigned").build_as_job_ref();
        let unassigned1 = TestSingleBuilder::default().id("unassigned1").build_as_job_ref();
        let unassigned2 = TestSingleBuilder::default().id("unassigned2").build_as_job_ref();

        let mut ctx: InsertionContext = TestInsertionContextBuilder::default().build();
        ctx.solution.required = vec![assigned.clone(), unassigned1.clone(), unassigned2.clone()];
        ctx.solution.unassigned.insert(unassigned1.clone(), UnassignmentInfo::Unknown);
        ctx.solution.unassigned.insert(unassigned2.clone(), UnassignmentInfo::Unknown);

        let selector = UnassignedJobSelector::default();
        let selected: Vec<_> = selector.select(&ctx).cloned().collect();

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|job| job == &unassigned1));
        assert!(selected.iter().any(|job| job == &unassigned2));
        assert!(selected.iter().all(|job| job != &assigned));
    }
}

mod any_feasible_result_selector {
    use super::*;
    use crate::construction::heuristics::{AnyFeasibleResultSelector, ResultSelector};
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::helpers::models::problem::TestSingleBuilder;

    fn make_success(id: &str, cost: Cost) -> InsertionResult {
        InsertionResult::make_success(
            InsertionCost::new(&[cost]),
            TestSingleBuilder::default().id(id).build_as_job_ref(),
            vec![],
            &RouteContextBuilder::default().build(),
        )
    }

    #[test]
    fn returns_first_success_regardless_of_cost() {
        // lhs is intentionally MORE expensive than rhs — Best would pick rhs;
        // AnyFeasible must pick lhs because it appears first.
        let lhs = make_success("lhs", 1000.);
        let rhs = make_success("rhs", 1.);
        let ctx = TestInsertionContextBuilder::default().build();

        let result = AnyFeasibleResultSelector::default().select_insertion(&ctx, lhs, rhs);

        match result {
            InsertionResult::Success(success) => assert_eq!(success.cost, InsertionCost::new(&[1000.])),
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn prefers_success_over_failure() {
        let success = make_success("ok", 100.);
        let failure = InsertionResult::make_failure();
        let ctx = TestInsertionContextBuilder::default().build();

        let result = AnyFeasibleResultSelector::default().select_insertion(&ctx, failure, success);

        match result {
            InsertionResult::Success(success) => assert_eq!(success.cost, InsertionCost::new(&[100.])),
            _ => panic!("expected Success"),
        }
    }
}

mod solo_aware_result_selector {
    use super::*;
    use crate::construction::features::JobSoloRidingDimension;
    use crate::construction::heuristics::{ResultSelector, SoloAwareResultSelector};
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::helpers::models::problem::TestSingleBuilder;

    fn solo_job(id: &str) -> Job {
        let mut builder = TestSingleBuilder::default();
        builder.id(id);
        builder.dimens_mut().set_job_solo_riding(true);
        Job::Single(builder.build_shared())
    }

    fn pooled_job(id: &str) -> Job {
        let mut builder = TestSingleBuilder::default();
        builder.id(id);
        Job::Single(builder.build_shared())
    }

    fn success_for_job(job: Job, cost: Cost, route_ctx: &RouteContext) -> InsertionResult {
        InsertionResult::make_success(InsertionCost::new(&[cost]), job, vec![], route_ctx)
    }

    #[test]
    fn defers_to_best_when_jobs_differ() {
        // Different jobs — empty-route preference is incoherent, fall back to cost.
        let lhs = success_for_job(solo_job("solo_a"), 100., &RouteContextBuilder::default().build());
        let rhs = success_for_job(solo_job("solo_b"), 1., &RouteContextBuilder::default().build());
        let ctx = TestInsertionContextBuilder::default().build();

        let result = SoloAwareResultSelector::default().select_insertion(&ctx, lhs, rhs);

        match result {
            InsertionResult::Success(success) => {
                assert_eq!(success.cost, InsertionCost::new(&[1.]), "should pick cost-cheaper across different jobs");
            }
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn defers_to_best_for_non_solo_jobs() {
        let job = pooled_job("pooled");
        let lhs = success_for_job(job.clone(), 100., &RouteContextBuilder::default().build());
        let rhs = success_for_job(job, 1., &RouteContextBuilder::default().build());
        let ctx = TestInsertionContextBuilder::default().build();

        let result = SoloAwareResultSelector::default().select_insertion(&ctx, lhs, rhs);

        match result {
            InsertionResult::Success(success) => {
                assert_eq!(success.cost, InsertionCost::new(&[1.]), "non-solo: should pick cost-cheaper");
            }
            _ => panic!("expected Success"),
        }
    }

    // Note: the empty-vs-in-use preference relies on `routes.iter().any(|rc|
    // rc.route().actor == lhs.actor && rc.route().tour.job_count() > 0)`. That
    // behaviour is exercised end-to-end by the integration test in
    // `vrp-pragmatic` and by the existing solo_riding feature tests; building
    // a stand-alone unit here would require fabricating a fully-populated
    // InsertionContext + Actor identity that matches across the assertion. Left
    // as integration-level coverage on purpose.
    //
    // The two negative tests above guard against the most common regression:
    // accidentally applying empty-route preference to non-solo jobs or across
    // different jobs — both of which would silently distort cost-optimal
    // placements.
    #[test]
    fn falls_through_when_no_route_is_in_use() {
        // Both target the same default-empty route, neither is in-use → defer
        // to BestResultSelector → cheaper wins.
        let job = solo_job("solo");
        let route_ctx = RouteContextBuilder::default().build();
        let lhs = success_for_job(job.clone(), 100., &route_ctx);
        let rhs = success_for_job(job, 1., &route_ctx);
        let ctx = TestInsertionContextBuilder::default().build();

        let result = SoloAwareResultSelector::default().select_insertion(&ctx, lhs, rhs);

        match result {
            InsertionResult::Success(success) => {
                assert_eq!(success.cost, InsertionCost::new(&[1.]));
            }
            _ => panic!("expected Success"),
        }
    }
}

mod noise_checks {
    use super::*;
    use crate::helpers::construction::heuristics::TestInsertionContextBuilder;
    use crate::helpers::models::problem::TestSingleBuilder;

    fn make_success(cost: Cost) -> InsertionResult {
        InsertionResult::make_success(
            InsertionCost::new(&[cost]),
            TestSingleBuilder::default().id("job1").build_as_job_ref(),
            vec![],
            &RouteContextBuilder::default().build(),
        )
    }

    parameterized_test! {can_compare_insertion_result_with_noise, (left, right, reals, expected_result), {
        can_compare_insertion_result_with_noise_impl(left, right, reals, expected_result);
    }}

    can_compare_insertion_result_with_noise! {
        case_01: (make_success(10.), make_success(11.), vec![0.05, 1.2, 0.05, 1.],  Some(11.)),
        case_02: (make_success(11.), make_success(10.), vec![0.05, 0.8, 0.05, 1.],  Some(11.)),
        case_03: (make_success(11.), make_success(10.), vec![0.05, 1., 0.2],  Some(10.)),

        case_04: (InsertionResult::make_failure(), make_success(11.), vec![],  Some(11.)),
        case_05: (make_success(10.), InsertionResult::make_failure(), vec![],  Some(10.)),
        case_06: (InsertionResult::make_failure(), InsertionResult::make_failure(), vec![],  None),
    }

    fn can_compare_insertion_result_with_noise_impl(
        left: InsertionResult,
        right: InsertionResult,
        reals: Vec<Float>,
        expected_result: Option<Float>,
    ) {
        let noise_probability = 0.1;
        let noise_range = (0.9, 1.2);
        let random = Arc::new(FakeRandom::new(vec![2], reals));
        let noise = Noise::new_with_ratio(noise_probability, noise_range, random);
        let insertion_ctx = TestInsertionContextBuilder::default().build();

        let actual_result = NoiseResultSelector::new(noise).select_insertion(&insertion_ctx, left, right);

        match (actual_result, expected_result) {
            (InsertionResult::Success(success), Some(cost)) => assert_eq!(success.cost, InsertionCost::new(&[cost])),
            (InsertionResult::Failure(_), None) => {}
            _ => unreachable!(),
        }
    }
}

mod iterators {
    use super::*;

    #[test]
    fn can_get_size_hint_for_tour_legs() {
        let (_, solution) = generate_matrix_routes_with_defaults(5, 1, false);

        assert_eq!(solution.routes[0].tour.legs().skip(2).size_hint().0, 4);
    }
}

mod selections {
    use super::*;
    use crate::helpers::models::problem::TestSingleBuilder;

    parameterized_test! {can_use_stochastic_selection_mode, (skip, activities, expected_threshold), {
        can_use_stochastic_selection_mode_impl(skip, activities, expected_threshold);
    }}

    can_use_stochastic_selection_mode! {
        case_01: (0, 1000, 100),
        case_02: (991, 1000, 11),
    }

    fn can_use_stochastic_selection_mode_impl(skip: usize, activities: usize, expected_threshold: usize) {
        let target = 10;
        let selection_mode = LegSelection::Stochastic(Environment::default().random);
        let (_, solution) = generate_matrix_routes_with_defaults(activities, 1, false);
        let route_ctx = RouteContext::new_with_state(solution.routes.into_iter().next().unwrap(), Default::default());
        let mut counter = 0;

        let _ = selection_mode.sample_best(
            &route_ctx,
            &TestSingleBuilder::default().build_as_job_ref(),
            skip,
            -1,
            &mut |leg: Leg, _| {
                counter += 1;
                ControlFlow::Continue(leg.1 as i32)
            },
            |lhs: &i32, rhs: &i32| {
                match (*lhs % 2 == 0, *rhs % 2 == 0) {
                    (true, false) => return true,
                    (false, true) => return false,
                    _ => {}
                }
                match (*lhs, *rhs) {
                    (_, rhs) if rhs == target => false,
                    (lhs, _) if lhs == target => true,
                    (lhs, rhs) => (lhs - target).abs() < (rhs - target).abs(),
                }
            },
        );

        assert!(counter < expected_threshold);
    }
}
