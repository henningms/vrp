use super::*;
use crate::format::problem::Location2D;
use crate::helpers::{
    SIMPLE_MATRIX, SIMPLE_PROBLEM, create_matrix, create_rush_hour_offset_profile, set_duration_offsets,
};
use std::fs::File;
use std::io::BufReader;

fn assert_time_windows(actual: &Option<Vec<Vec<String>>>, expected: (&str, &str)) {
    let actual = actual.as_ref().unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual.first().unwrap().len(), 2);
    assert_eq!(actual.first().unwrap().first().unwrap(), expected.0);
    assert_eq!(actual.first().unwrap().last().unwrap(), expected.1);
}

fn assert_location(actual: &Location, expected: (f64, f64)) {
    let (lat, lng) = actual.to_lat_lng();

    assert_eq!(lat, expected.0);
    assert_eq!(lng, expected.1);
}

fn assert_demand(actual: &Option<Vec<i32>>, expected: i32) {
    let actual = actual.as_ref().expect("Empty demand!");
    assert_eq!(actual.len(), 1);
    assert_eq!(*actual.first().unwrap(), expected);
}

#[test]
fn can_deserialize_problem() {
    let problem = deserialize_problem(BufReader::new(SIMPLE_PROBLEM.as_bytes())).ok().unwrap();

    assert_eq!(problem.plan.jobs.len(), 2);
    assert_eq!(problem.fleet.vehicles.len(), 1);
    assert!(problem.plan.relations.is_none());

    // validate jobs
    let job = problem.plan.jobs.first().unwrap();
    assert_eq!(job.id, "single_job");
    assert!(job.pickups.is_none());
    assert!(job.deliveries.is_some());
    assert!(job.skills.is_none());

    let deliveries = job.deliveries.as_ref().unwrap();
    assert_eq!(deliveries.len(), 1);
    let delivery = deliveries.first().unwrap();
    assert_demand(&delivery.demand, 1);
    assert!(delivery.places.first().unwrap().tag.is_none());

    assert_eq!(delivery.places.len(), 1);
    let place = delivery.places.first().unwrap();
    assert_eq!(place.duration, 240.);
    assert_location(&place.location, (52.5622847f64, 13.4023099f64));
    assert_time_windows(&place.times, ("2019-07-04T10:00:00Z", "2019-07-04T16:00:00Z"));

    let job = problem.plan.jobs.last().unwrap();
    assert_eq!(job.id, "multi_job");
    assert!(job.skills.is_none());
    assert_eq!(job.pickups.as_ref().unwrap().len(), 2);
    assert_eq!(job.deliveries.as_ref().unwrap().len(), 1);
}

#[test]
fn can_deserialize_matrix() {
    let matrix = deserialize_matrix(BufReader::new(SIMPLE_MATRIX.as_bytes())).ok().unwrap();

    assert_eq!(matrix.distances.len(), 16);
    assert_eq!(matrix.travel_times.len(), 16);
}

const PROBLEM_WITH_FERRY_CROSSING: &str = r#"
{
  "plan": { "jobs": [] },
  "fleet": { "vehicles": [], "profiles": [] },
  "ferryCrossings": [
    {
      "id": "crossing1",
      "quayA": { "lat": 1.0, "lng": 2.0 },
      "quayB": { "lat": 3.0, "lng": 4.0 },
      "crossingSec": 300.0,
      "boardingBufferSec": 600.0,
      "sailings": {
        "aToB": [{ "dep": 0.0, "arr": 900.0 }],
        "bToA": [{ "dep": 1800.0, "arr": 2700.0 }]
      }
    }
  ]
}
"#;

#[test]
fn can_deserialize_problem_with_ferry_crossings() {
    let problem = deserialize_problem(BufReader::new(PROBLEM_WITH_FERRY_CROSSING.as_bytes())).ok().unwrap();

    let crossings = problem.ferry_crossings.as_ref().expect("expected ferryCrossings to be present");
    assert_eq!(crossings.len(), 1);

    let crossing = crossings.first().unwrap();
    assert_eq!(crossing.id, "crossing1");
    assert_eq!(crossing.quay_a, Location2D { lat: 1.0, lng: 2.0 });
    assert_eq!(crossing.quay_b, Location2D { lat: 3.0, lng: 4.0 });
    assert_eq!(crossing.crossing_sec, 300.0);
    assert_eq!(crossing.boarding_buffer_sec, 600.0);
    assert_eq!(crossing.sailings.a_to_b.len(), 1);
    assert_eq!(crossing.sailings.a_to_b.first().unwrap().dep, 0.0);
    assert_eq!(crossing.sailings.b_to_a.first().unwrap().arr, 2700.0);
}

#[test]
fn serializing_problem_without_ferry_crossings_emits_no_key() {
    // "empty means unchanged": a problem with no ferry crossings must serialize exactly as it
    // did before ferry crossings existed.
    let problem = deserialize_problem(BufReader::new(SIMPLE_PROBLEM.as_bytes())).ok().unwrap();
    assert!(problem.ferry_crossings.is_none());

    let serialized = serde_json::to_string(&problem).unwrap();

    assert!(!serialized.contains("ferryCrossings"), "serialized problem unexpectedly has a ferryCrossings key");
}

#[test]
fn can_deserialize_one_of_skill_groups() {
    let skills: JobSkills =
        serde_json::from_str(r#"{ "oneOf": [["driver:a", "driver:b"], ["ramp", "lift"]] }"#).unwrap();

    assert_eq!(
        skills.one_of,
        Some(vec![vec!["driver:a".to_string(), "driver:b".to_string()], vec!["ramp".to_string(), "lift".to_string()]])
    );
}

#[test]
fn can_deserialize_one_of_flat_skill_list_as_one_group() {
    let skills: JobSkills = serde_json::from_str(r#"{ "oneOf": ["ramp", "lift"] }"#).unwrap();

    assert_eq!(skills.one_of, Some(vec![vec!["ramp".to_string(), "lift".to_string()]]));
}

#[test]
fn can_deserialize_missing_null_and_empty_one_of() {
    let parse = |json: &str| serde_json::from_str::<JobSkills>(json).unwrap().one_of;

    assert_eq!(parse(r#"{ "allOf": ["fridge"] }"#), None);
    assert_eq!(parse(r#"{ "oneOf": null }"#), None);
    assert_eq!(parse(r#"{ "oneOf": [] }"#), Some(vec![]));
    assert_eq!(parse(r#"{ "oneOf": [[]] }"#), Some(vec![vec![]]));
}

#[test]
fn can_reject_one_of_mixing_skills_and_groups() {
    let result = serde_json::from_str::<JobSkills>(r#"{ "oneOf": ["ramp", ["lift"]] }"#);

    assert!(result.unwrap_err().to_string().contains("a list of skill groups or a list of skills"));
}

#[test]
fn can_serialize_flat_one_of_as_one_group() {
    let skills: JobSkills = serde_json::from_str(r#"{ "oneOf": ["ramp", "lift"] }"#).unwrap();

    assert_eq!(serde_json::to_string(&skills).unwrap(), r#"{"oneOf":[["ramp","lift"]]}"#);
}

/// The documented example; cargo runs the tests from the crate directory.
const DOCUMENTED_EXAMPLE: &str = "../examples/data/pragmatic/basics/duration-offsets.basic.matrix.json";

#[test]
fn can_deserialize_matrix_with_duration_offsets() {
    let matrix = deserialize_matrix(BufReader::new(File::open(DOCUMENTED_EXAMPLE).unwrap())).ok().unwrap();

    let offsets = matrix.duration_offsets.expect("duration offsets");
    assert_eq!(offsets.cells, vec![0, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(offsets.profiles[0].points.len(), 4);
    assert_eq!(offsets.profiles[0].points[1].time, "2026-10-22T07:00:00Z");
    assert_eq!(offsets.profiles[0].points[1].offset, 480.);
}

#[test]
fn serializing_matrix_without_duration_offsets_emits_no_key() {
    let matrix = deserialize_matrix(BufReader::new(SIMPLE_MATRIX.as_bytes())).ok().unwrap();

    assert!(!serde_json::to_string(&matrix).unwrap().contains("durationOffsets"));
}

#[test]
fn refuses_a_duration_offset_cell_beyond_sixteen_bits() {
    let json =
        r#"{"travelTimes":[0,1,1,0],"distances":[0,1,1,0],"durationOffsets":{"profiles":[],"cells":[0,65536,0,0]}}"#;

    let result = deserialize_matrix(BufReader::new(json.as_bytes()));

    assert_eq!(result.err().map(|err| err.errors[0].code.clone()), Some("E0001".to_string()));
}

#[test]
fn serializes_duration_offsets_built_by_the_test_helpers() {
    let matrix =
        set_duration_offsets(create_matrix(vec![0, 1, 1, 0]), vec![create_rush_hour_offset_profile()], &[(0, 1, 1)]);

    let json = serde_json::to_string(&matrix).unwrap();

    assert!(
        json.contains(r#""durationOffsets":{"profiles":[{"points":[{"time":"1970-01-01T06:52:00Z","offset":0.0}"#),
        "{json}"
    );
    assert!(json.contains(r#""cells":[0,1,0,0]"#), "{json}");
}
