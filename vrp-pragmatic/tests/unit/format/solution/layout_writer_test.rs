use super::*;
use crate::helpers::*;

/// all_seated: [8, 0], one_wheelchair: [4, 1]
fn minibus() -> Vec<Vec<i32>> {
    vec![vec![8, 0], vec![4, 1]]
}

/// all_seated: [8, 0], one_wheelchair: [4, 1], two_wheelchairs: [2, 2]
fn three_layouts() -> Vec<Vec<i32>> {
    vec![vec![8, 0], vec![4, 1], vec![2, 2]]
}

parameterized_test! {can_choose_layouts, (loads, layouts, expected), {
    can_choose_layouts_impl(loads, layouts, expected);
}}

can_choose_layouts! {
    case01_one_layout: (vec![vec![0], vec![2], vec![0]], vec![vec![4]], Some(vec![0, 0, 0])),
    case02_lowest_index_when_several_fit: (vec![vec![0, 0], vec![2, 0], vec![0, 0]], minibus(), Some(vec![0, 0, 0])),
    case03_keep_current_over_lower_index: (vec![vec![0, 1], vec![2, 0], vec![0, 0]], minibus(), Some(vec![1, 1, 1])),
    case04_switch_when_passenger_needing_layout_boards: (
        vec![vec![0, 0], vec![0, 1], vec![0, 0], vec![2, 0], vec![0, 0], vec![6, 0], vec![0, 0]],
        minibus(),
        Some(vec![1, 1, 1, 1, 1, 0, 0]),
    ),
    case05_switch_after_activity_which_empties_vehicle: (
        vec![vec![0, 1], vec![0, 0], vec![6, 0], vec![0, 0]],
        minibus(),
        Some(vec![1, 1, 0, 0]),
    ),
    case06_prefer_empty_switches_over_fewer_switches: (
        vec![vec![0, 0], vec![6, 0], vec![0, 0], vec![3, 0], vec![2, 1], vec![0, 0], vec![0, 2], vec![0, 0]],
        three_layouts(),
        Some(vec![0, 0, 0, 1, 1, 1, 2, 2]),
    ),
    case07_switch_with_passengers_aboard_when_needed: (
        vec![vec![0, 1], vec![2, 1], vec![2, 0], vec![6, 0], vec![4, 0], vec![0, 0], vec![0, 0]],
        minibus(),
        Some(vec![1, 1, 1, 0, 0, 0, 0]),
    ),
    case08_no_load: (vec![vec![0], vec![0, 0], vec![0]], three_layouts(), Some(vec![0, 0, 0])),
    case09_load_fits_no_layout: (vec![vec![0, 0], vec![9, 0], vec![0, 0]], minibus(), None),
    case10_extra_dimension_has_no_capacity: (vec![vec![0, 0, 1]], minibus(), None),
    case11_zero_extra_dimension_fits: (vec![vec![2, 0, 0]], minibus(), Some(vec![0])),
    case12_no_activities: (vec![], minibus(), Some(vec![])),
}

fn can_choose_layouts_impl(loads: Vec<Vec<i32>>, layouts: Vec<Vec<i32>>, expected: Option<Vec<usize>>) {
    let result = choose_layouts(&loads, &layouts);

    assert_eq!(result, expected);
}

fn layout(index: usize, name: Option<&str>) -> Option<CapacityLayout> {
    Some(CapacityLayout { index, name: name.map(str::to_string) })
}

fn span(index: usize, name: Option<&str>, from_stop: usize, to_stop: usize) -> CapacityLayoutSpan {
    CapacityLayoutSpan { index, name: name.map(str::to_string), from_stop, to_stop }
}

fn get_layouts(tour: &Tour) -> Vec<Option<CapacityLayout>> {
    tour.stops.iter().map(|stop| stop.layout().cloned()).collect()
}

fn minibus_names() -> Vec<Option<String>> {
    vec![Some("all_seated".to_string()), Some("one_wheelchair".to_string())]
}

#[test]
fn can_use_layout_of_last_activity_for_stop() {
    let mut tour = TourBuilder::default()
        .stops(vec![
            StopBuilder::default()
                .load(vec![0, 1])
                .activity(ActivityBuilder::default().job_id("departure").activity_type("departure").build())
                .activity(ActivityBuilder::pickup().job_id("wheelchair").build())
                .build(),
            StopBuilder::default()
                .load(vec![6, 0])
                .activity(ActivityBuilder::delivery().job_id("wheelchair").build())
                .activity(ActivityBuilder::pickup().job_id("seated").build())
                .build(),
            StopBuilder::default().load(vec![0, 0]).build_single("seated", "delivery"),
            StopBuilder::default().load(vec![0, 0]).build_arrival(),
        ])
        .build();
    let activity_loads =
        vec![vec![vec![0, 0], vec![0, 1]], vec![vec![0, 0], vec![6, 0]], vec![vec![0, 0]], vec![vec![0, 0]]];

    write_capacity_layouts(&mut tour, &activity_loads, &minibus(), &minibus_names());

    assert_eq!(
        get_layouts(&tour),
        vec![
            layout(1, Some("one_wheelchair")),
            layout(0, Some("all_seated")),
            layout(0, Some("all_seated")),
            layout(0, Some("all_seated")),
        ]
    );
    assert_eq!(
        tour.capacity_layouts,
        Some(vec![span(1, Some("one_wheelchair"), 0, 0), span(0, Some("all_seated"), 1, 3)])
    );
}

#[test]
fn can_keep_layout_of_previous_stop_at_transit_stop() {
    let mut tour = TourBuilder::default()
        .stops(vec![
            StopBuilder::default().load(vec![0, 1]).build_departure(),
            StopBuilder::new_transit().load(vec![0, 1]).activity(ActivityBuilder::break_type().build()).build(),
            StopBuilder::default().load(vec![0, 0]).build_single("wheelchair", "delivery"),
            StopBuilder::default().load(vec![6, 0]).build_single("seated", "pickup"),
            StopBuilder::default().load(vec![0, 0]).build_single("seated", "delivery"),
            StopBuilder::default().load(vec![0, 0]).build_arrival(),
        ])
        .build();
    let activity_loads = vec![vec![vec![0, 1]], vec![vec![0, 0]], vec![vec![6, 0]], vec![vec![0, 0]], vec![vec![0, 0]]];
    let names = vec![Some("all_seated".to_string()), None];

    write_capacity_layouts(&mut tour, &activity_loads, &minibus(), &names);

    assert_eq!(
        get_layouts(&tour),
        vec![
            layout(1, None),
            layout(1, None),
            layout(1, None),
            layout(0, Some("all_seated")),
            layout(0, Some("all_seated")),
            layout(0, Some("all_seated")),
        ]
    );
    assert_eq!(tour.capacity_layouts, Some(vec![span(1, None, 0, 2), span(0, Some("all_seated"), 3, 5)]));
}

#[test]
fn can_leave_layouts_out_when_load_fits_no_layout() {
    let mut tour = TourBuilder::default()
        .stops(vec![
            StopBuilder::default().load(vec![0, 0]).build_departure(),
            StopBuilder::default().load(vec![9, 0]).build_single("group", "pickup"),
            StopBuilder::default().load(vec![0, 0]).build_arrival(),
        ])
        .build();
    let activity_loads = vec![vec![vec![0, 0]], vec![vec![9, 0]], vec![vec![0, 0]]];

    write_capacity_layouts(&mut tour, &activity_loads, &minibus(), &minibus_names());

    assert_eq!(get_layouts(&tour), vec![None, None, None]);
    assert_eq!(tour.capacity_layouts, None);
}

#[test]
fn can_write_single_layout() {
    let mut tour = TourBuilder::default()
        .stops(vec![
            StopBuilder::default().load(vec![0]).build_departure(),
            StopBuilder::default().load(vec![0, 1]).build_single("wheelchair", "pickup"),
            StopBuilder::default().load(vec![0]).build_arrival(),
        ])
        .build();
    let activity_loads = vec![vec![vec![0]], vec![vec![0, 1]], vec![vec![0]]];

    write_capacity_layouts(&mut tour, &activity_loads, &[vec![0, 1]], &[Some("one_wheelchair".to_string())]);

    assert_eq!(get_layouts(&tour), vec![layout(0, Some("one_wheelchair")); 3]);
    assert_eq!(tour.capacity_layouts, Some(vec![span(0, Some("one_wheelchair"), 0, 2)]));
}
