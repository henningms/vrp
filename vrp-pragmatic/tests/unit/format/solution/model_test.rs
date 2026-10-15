use super::*;
use crate::helpers::*;
use serde_json::json;
use std::io::BufReader;

fn create_tour_with_layouts() -> Tour {
    TourBuilder::default()
        .stops(vec![
            StopBuilder::default().load(vec![0, 1]).layout(1, Some("one_wheelchair")).build_departure(),
            StopBuilder::default().load(vec![0, 0]).layout(0, None).build_arrival(),
        ])
        .capacity_layouts(vec![
            CapacityLayoutSpan { index: 1, name: Some("one_wheelchair".to_string()), from_stop: 0, to_stop: 0 },
            CapacityLayoutSpan { index: 0, name: None, from_stop: 1, to_stop: 1 },
        ])
        .build()
}

#[test]
fn can_serialize_capacity_layouts_in_camel_case() {
    let value = serde_json::to_value(create_tour_with_layouts()).unwrap();

    assert_eq!(
        value["capacityLayouts"],
        json!([
            { "index": 1, "name": "one_wheelchair", "fromStop": 0, "toStop": 0 },
            { "index": 0, "fromStop": 1, "toStop": 1 },
        ])
    );
    assert_eq!(value["stops"][0]["layout"], json!({ "index": 1, "name": "one_wheelchair" }));
    assert_eq!(value["stops"][1]["layout"], json!({ "index": 0 }));
}

#[test]
fn can_skip_capacity_layouts_when_absent() {
    let tour = TourBuilder::default().stops(vec![StopBuilder::default().load(vec![0]).build_departure()]).build();

    let value = serde_json::to_value(tour).unwrap();

    assert!(value.get("capacityLayouts").is_none());
    assert!(value["stops"][0].get("layout").is_none());
}

#[test]
fn can_deserialize_capacity_layouts() {
    let tour = create_tour_with_layouts();

    let parsed: Tour = serde_json::from_value(serde_json::to_value(&tour).unwrap()).unwrap();

    assert_eq!(parsed, tour);
}

#[test]
fn can_deserialize_solution_without_capacity_layouts() {
    let json = r#"{
      "statistic": { "cost": 0, "distance": 0, "duration": 0,
                     "times": { "driving": 0, "serving": 0, "waiting": 0, "break": 0 } },
      "tours": [{
        "vehicleId": "my_vehicle_1",
        "typeId": "my_vehicle",
        "shiftIndex": 0,
        "stops": [{
          "location": { "lat": 0, "lng": 0 },
          "time": { "arrival": "1970-01-01T00:00:00Z", "departure": "1970-01-01T00:00:00Z" },
          "distance": 0,
          "load": [0],
          "activities": [{ "jobId": "departure", "type": "departure" }]
        }],
        "statistic": { "cost": 0, "distance": 0, "duration": 0,
                       "times": { "driving": 0, "serving": 0, "waiting": 0, "break": 0 } }
      }]
    }"#;

    let solution = deserialize_solution(BufReader::new(json.as_bytes())).expect("solution should parse");

    let tour = solution.tours.first().unwrap();
    assert_eq!(tour.capacity_layouts, None);
    assert_eq!(tour.stops.first().unwrap().layout(), None);
}
