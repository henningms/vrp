use super::*;

use crate::helpers::models::problem::{FleetBuilder, TestSingleBuilder, TestVehicleBuilder, test_driver};
use crate::helpers::models::solution::{RouteBuilder, RouteContextBuilder};
use std::collections::HashSet;

fn create_job_with_preferences(
    id: &str,
    preferred: Option<Vec<&str>>,
    acceptable: Option<Vec<&str>>,
    avoid: Option<Vec<&str>>,
) -> Job {
    let mut builder = TestSingleBuilder::default();
    builder.id(id).dimens_mut().set_job_preferences(JobPreferences::new(
        preferred.map(|v| v.iter().map(|s| s.to_string()).collect()),
        acceptable.map(|v| v.iter().map(|s| s.to_string()).collect()),
        avoid.map(|v| v.iter().map(|s| s.to_string()).collect()),
    ));

    builder.build_as_job_ref()
}

fn create_vehicle_with_attributes(id: &str, attributes: Vec<&str>) -> Vehicle {
    let mut builder = TestVehicleBuilder::default();
    let attrs: HashSet<String> = attributes.iter().map(|s| s.to_string()).collect();
    builder.id(id).dimens_mut().set_vehicle_attributes(attrs);
    builder.build()
}

fn create_route_ctx_with_attributes(attributes: Vec<&str>) -> RouteContext {
    let vehicle = create_vehicle_with_attributes("vehicle1", attributes);
    let fleet = FleetBuilder::default().add_driver(test_driver()).add_vehicle(vehicle).build();
    RouteContextBuilder::default().with_route(RouteBuilder::default().with_vehicle(&fleet, "vehicle1").build()).build()
}

#[test]
fn test_job_preferences_new() {
    let prefs = JobPreferences::new(
        Some(vec!["driver:alice".to_string()]),
        Some(vec!["driver:bob".to_string()]),
        Some(vec!["shift:night".to_string()]),
    );

    assert!(prefs.preferred.is_some());
    assert!(prefs.acceptable.is_some());
    assert!(prefs.avoid.is_some());
    assert_eq!(prefs.preferred.as_ref().unwrap().len(), 1);
}

#[test]
fn test_job_preferences_empty_lists() {
    let prefs = JobPreferences::new(Some(vec![]), Some(vec![]), Some(vec![]));

    assert!(prefs.preferred.is_none());
    assert!(prefs.acceptable.is_none());
    assert!(prefs.avoid.is_none());
}

#[test]
fn test_has_preferred_match() {
    let prefs = JobPreferences::new(Some(vec!["driver:alice".to_string(), "driver:bob".to_string()]), None, None);

    let attrs_alice: HashSet<String> = vec!["driver:alice".to_string()].into_iter().collect();
    let attrs_charlie: HashSet<String> = vec!["driver:charlie".to_string()].into_iter().collect();

    assert!(prefs.has_preferred_match(Some(&attrs_alice)));
    assert!(!prefs.has_preferred_match(Some(&attrs_charlie)));
}

#[test]
fn test_has_acceptable_match() {
    let prefs = JobPreferences::new(None, Some(vec!["driver:bob".to_string()]), None);

    let attrs_bob: HashSet<String> = vec!["driver:bob".to_string()].into_iter().collect();
    let attrs_charlie: HashSet<String> = vec!["driver:charlie".to_string()].into_iter().collect();

    assert!(prefs.has_acceptable_match(Some(&attrs_bob)));
    assert!(!prefs.has_acceptable_match(Some(&attrs_charlie)));
}

#[test]
fn test_count_avoided() {
    let prefs = JobPreferences::new(None, None, Some(vec!["shift:night".to_string(), "vehicle:old".to_string()]));

    let attrs_night: HashSet<String> = vec!["shift:night".to_string()].into_iter().collect();
    let attrs_both: HashSet<String> =
        vec!["shift:night".to_string(), "vehicle:old".to_string()].into_iter().collect();
    let attrs_none: HashSet<String> = vec!["shift:day".to_string()].into_iter().collect();

    assert_eq!(prefs.count_avoided(Some(&attrs_night)), 1);
    assert_eq!(prefs.count_avoided(Some(&attrs_both)), 2);
    assert_eq!(prefs.count_avoided(Some(&attrs_none)), 0);
}

#[test]
fn test_penalty_no_preferences() {
    let job = create_job_with_preferences("job1", None, None, None);
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:alice"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, 0.0);
}

#[test]
fn test_penalty_preferred_match() {
    let job = create_job_with_preferences("job1", Some(vec!["driver:alice"]), None, None);
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:alice"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, 0.0);
}

#[test]
fn test_penalty_no_preferred_match() {
    let job = create_job_with_preferences("job1", Some(vec!["driver:alice"]), None, None);
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:bob"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, penalty_config.no_preferred_match);
}

#[test]
fn test_penalty_acceptable_match() {
    let job = create_job_with_preferences("job1", Some(vec!["driver:alice"]), Some(vec!["driver:bob"]), None);
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:bob"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    // No preferred match, but has acceptable, so only no_preferred_match penalty
    assert_eq!(penalty, penalty_config.no_preferred_match);
}

#[test]
fn test_penalty_no_acceptable_match() {
    let job = create_job_with_preferences("job1", Some(vec!["driver:alice"]), Some(vec!["driver:bob"]), None);
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:charlie"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    // No preferred AND no acceptable match
    assert_eq!(penalty, penalty_config.no_preferred_match + penalty_config.no_acceptable_match);
}

#[test]
fn test_penalty_avoided_present() {
    let job = create_job_with_preferences("job1", None, None, Some(vec!["shift:night"]));
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:alice", "shift:night"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, penalty_config.per_avoided_present);
}

#[test]
fn test_penalty_multiple_avoided_present() {
    let job = create_job_with_preferences("job1", None, None, Some(vec!["shift:night", "vehicle:old"]));
    let route_ctx = create_route_ctx_with_attributes(vec!["shift:night", "vehicle:old"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, 2.0 * penalty_config.per_avoided_present);
}

#[test]
fn test_penalty_combined() {
    let job = create_job_with_preferences("job1", Some(vec!["driver:alice"]), None, Some(vec!["shift:night"]));
    // Vehicle is Bob with night shift (not preferred, and has avoided attribute)
    let route_ctx = create_route_ctx_with_attributes(vec!["driver:bob", "shift:night"]);

    let penalty_config = PreferencePenalty::default();
    let penalty = calculate_job_penalty(&penalty_config, &job, &route_ctx);

    assert_eq!(penalty, penalty_config.no_preferred_match + penalty_config.per_avoided_present);
}

// Make the helper functions visible for testing
use super::super::super::super::construction::features::preferences::calculate_job_penalty;
