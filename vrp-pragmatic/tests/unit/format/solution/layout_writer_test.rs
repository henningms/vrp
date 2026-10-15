use super::*;

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
