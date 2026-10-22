use super::*;
use crate::helpers::*;

#[test]
fn can_detect_duplicates() {
    let problem = Problem {
        fleet: Fleet {
            profiles: vec![
                MatrixProfile { name: "my_vehicle".to_string(), speed: None },
                MatrixProfile { name: "my_vehicle".to_string(), speed: None },
            ],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, None, &coord_index);

    let result = check_e1500_duplicated_profiles(&ctx);

    assert_eq!(result.err().map(|err| err.code), Some("E1500".to_string()));
}

#[test]
fn can_detect_empty_profiles() {
    let problem = Problem { fleet: Fleet { profiles: vec![], ..create_default_fleet() }, ..create_empty_problem() };
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, None, &coord_index);

    let result = check_e1501_empty_profiles(&ctx);

    assert_eq!(result.err().map(|err| err.code), Some("E1501".to_string()));
}

#[test]
fn can_detect_mixed_locations() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job_with_index("job1", 0), create_delivery_job("job2", (1., 0.))],
            relations: None,
            ..create_empty_plan()
        },
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, None, &coord_index);
    let location_types = (ctx.coord_index.has_coordinates(), ctx.coord_index.has_indices());

    let result = check_e1502_no_location_type_mix(&ctx, location_types);

    assert_eq!(result.err().map(|err| err.code), Some("E1502".to_string()));
}

#[test]
fn can_detect_missing_matrix_when_indices_used() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job_with_index("job1", 0)], ..create_empty_plan() },
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, None, &coord_index);
    let location_types = (ctx.coord_index.has_coordinates(), ctx.coord_index.has_indices());

    let result = check_e1503_no_matrix_when_indices_used(&ctx, location_types);

    assert_eq!(result.err().map(|err| err.code), Some("E1503".to_string()));
}

#[test]
fn can_detect_index_mismatch() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job_with_index("job1", 0),
                create_delivery_job_with_index("job2", 1),
                create_delivery_job_with_index("job3", 2),
            ],
            ..create_empty_plan()
        },
        ..create_empty_problem()
    };
    let matrices = vec![Matrix {
        profile: Some("car".to_owned()),
        timestamp: None,
        travel_times: vec![1; 4],
        distances: vec![1; 4],
        error_codes: None,
        opposite_side_arrivals: None,
        duration_offsets: None,
    }];
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, Some(&matrices), &coord_index);

    let result = check_e1504_index_size_mismatch(&ctx);

    assert_eq!(result.err().map(|err| err.code), Some("E1504".to_string()));
}

#[test]
fn can_detect_missing_profile() {
    let problem = Problem {
        fleet: Fleet {
            vehicles: vec![
                VehicleType { profile: create_vehicle_profile_with_name("car"), ..create_default_vehicle_type() },
                VehicleType { profile: create_vehicle_profile_with_name("truck"), ..create_default_vehicle_type() },
            ],
            profiles: vec![MatrixProfile { name: "car".to_string(), speed: None }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let coord_index = CoordIndex::new(&problem);
    let ctx = ValidationContext::new(&problem, None, &coord_index);

    let result = check_e1505_profiles_exist(&ctx);

    assert_eq!(result.err().map(|err| err.code), Some("E1505".to_string()));
}

fn matrix_with_opposite_side_arrivals(timestamp: Option<&str>, cells: Vec<usize>) -> Vec<Matrix> {
    vec![Matrix {
        profile: Some("car".to_owned()),
        timestamp: timestamp.map(|t| t.to_string()),
        travel_times: vec![1; 4],
        distances: vec![1; 4],
        error_codes: None,
        opposite_side_arrivals: Some(cells),
        duration_offsets: None,
    }]
}

#[test]
fn can_detect_opposite_side_arrival_outside_the_matrix() {
    let problem = create_empty_problem();
    let coord_index = CoordIndex::new(&problem);
    let check = |cells| {
        let matrices = matrix_with_opposite_side_arrivals(None, cells);
        check_e1506_opposite_side_arrivals_in_matrix(&ValidationContext::new(&problem, Some(&matrices), &coord_index))
            .err()
            .map(|err| err.code)
    };

    assert_eq!(check(vec![1, 3]), None);
    assert_eq!(check(vec![1, 4]), Some("E1506".to_string()));
}

#[test]
fn can_detect_opposite_side_arrivals_on_a_time_dependent_matrix() {
    let problem = create_empty_problem();
    let coord_index = CoordIndex::new(&problem);
    let check = |timestamp| {
        let matrices = matrix_with_opposite_side_arrivals(timestamp, vec![1]);
        check_e1507_opposite_side_arrivals_without_timestamp(&ValidationContext::new(
            &problem,
            Some(&matrices),
            &coord_index,
        ))
        .err()
        .map(|err| err.code)
    };

    assert_eq!(check(None), None);
    assert_eq!(check(Some("1970-01-01T00:00:00Z")), Some("E1507".to_string()));
}

const NINE: &str = "1970-01-01T09:00:00Z";
const NINE_EIGHT: &str = "1970-01-01T09:08:00Z";

fn matrix_with_duration_offsets(timestamp: Option<&str>, points: Vec<(&str, Float)>, cells: Vec<u16>) -> Vec<Matrix> {
    vec![Matrix {
        profile: Some("car".to_owned()),
        timestamp: timestamp.map(|t| t.to_string()),
        travel_times: vec![1; 4],
        distances: vec![1; 4],
        error_codes: None,
        opposite_side_arrivals: None,
        duration_offsets: Some(MatrixDurationOffsets {
            profiles: vec![DurationOffsetProfile {
                points: points
                    .into_iter()
                    .map(|(time, offset)| DurationOffsetPoint { time: time.to_string(), offset })
                    .collect(),
            }],
            cells,
        }),
    }]
}

fn check_duration_offsets(
    matrices: &[Matrix],
    problem: &Problem,
    check: fn(&ValidationContext) -> Result<(), FormatError>,
) -> Option<FormatError> {
    let matrices = matrices.to_vec();
    let coord_index = CoordIndex::new(problem);
    check(&ValidationContext::new(problem, Some(&matrices), &coord_index)).err()
}

fn required() -> VehicleBreak {
    VehicleBreak::Required { time: VehicleRequiredBreakTime::OffsetTime { earliest: 10., latest: 10. }, duration: 5. }
}

fn optional() -> VehicleBreak {
    VehicleBreak::Optional {
        time: VehicleOptionalBreakTime::TimeOffset(vec![10., 20.]),
        places: vec![VehicleOptionalBreakPlace { duration: 2.0, location: None, tag: None }],
        policy: None,
    }
}

parameterized_test! {can_detect_duration_offset_cells_which_do_not_match_the_matrix, (cells, expected), {
    can_detect_duration_offset_cells_which_do_not_match_the_matrix_impl(cells, expected);
}}

fn can_detect_duration_offset_cells_which_do_not_match_the_matrix_impl(cells: Vec<u16>, expected: Option<String>) {
    let matrices = matrix_with_duration_offsets(None, vec![(NINE, 0.)], cells);

    let result = check_duration_offsets(&matrices, &create_empty_problem(), check_e9501_duration_offset_cells);

    assert_eq!(result.map(|err| err.code), expected);
}

can_detect_duration_offset_cells_which_do_not_match_the_matrix! {
    case01_valid: (vec![0, 1, 0, 0], None),
    case02_too_few: (vec![0, 1, 0], Some("E9501".to_string())),
    case03_unknown_profile: (vec![0, 2, 0, 0], Some("E9501".to_string())),
    case04_all_zero_but_too_few: (vec![0, 0, 0], Some("E9501".to_string())),
}

#[test]
fn names_the_cell_and_profile_when_a_duration_offset_cell_is_unknown() {
    let matrices = matrix_with_duration_offsets(None, vec![(NINE, 0.)], vec![0, 2, 0, 0]);

    let err = check_duration_offsets(&matrices, &create_empty_problem(), check_e9501_duration_offset_cells).unwrap();

    assert!(err.action.contains("'car' cell 1 names profile 2 of 1"), "{}", err.action);
}

parameterized_test! {can_detect_an_invalid_duration_offset_profile, (points, expected), {
    can_detect_an_invalid_duration_offset_profile_impl(points, expected);
}}

fn can_detect_an_invalid_duration_offset_profile_impl(points: Vec<(&str, Float)>, expected: Option<String>) {
    let matrices = matrix_with_duration_offsets(None, points, vec![0, 1, 0, 0]);

    let result = check_duration_offsets(&matrices, &create_empty_problem(), check_e9502_duration_offset_profiles);

    assert_eq!(result.map(|err| err.code), expected);
}

can_detect_an_invalid_duration_offset_profile! {
    case01_valid: (vec![(NINE, 480.), (NINE_EIGHT, 0.)], None),
    case02_no_points: (vec![], Some("E9502".to_string())),
    case03_not_rfc3339: (vec![("09:00", 0.)], Some("E9502".to_string())),
    case04_decreasing_times: (vec![(NINE_EIGHT, 0.), (NINE, 0.)], Some("E9502".to_string())),
    case05_repeated_time: (vec![(NINE, 0.), (NINE, 10.)], Some("E9502".to_string())),
    case06_negative_offset: (vec![(NINE, -1.)], Some("E9502".to_string())),
    case07_offset_not_finite: (vec![(NINE, Float::NAN)], Some("E9502".to_string())),
}

parameterized_test! {can_detect_a_duration_offset_profile_which_falls_faster_than_time, (end_offset, expected), {
    can_detect_a_duration_offset_profile_which_falls_faster_than_time_impl(end_offset, expected);
}}

fn can_detect_a_duration_offset_profile_which_falls_faster_than_time_impl(end_offset: Float, expected: Option<String>) {
    let matrices = matrix_with_duration_offsets(None, vec![(NINE, end_offset), (NINE_EIGHT, 0.)], vec![0, 1, 0, 0]);

    let result = check_duration_offsets(
        &matrices,
        &create_empty_problem(),
        check_e9503_duration_offsets_fall_at_most_one_second_per_second,
    );

    assert_eq!(result.map(|err| err.code), expected);
}

can_detect_a_duration_offset_profile_which_falls_faster_than_time! {
    case01_one_second_per_second: (480., None),
    case02_faster: (480.048, Some("E9503".to_string())),
}

#[test]
fn names_the_profile_and_both_points_when_an_offset_falls_faster_than_time() {
    let matrices = matrix_with_duration_offsets(None, vec![(NINE, 600.), (NINE_EIGHT, 0.)], vec![0, 1, 0, 0]);

    let err = check_duration_offsets(
        &matrices,
        &create_empty_problem(),
        check_e9503_duration_offsets_fall_at_most_one_second_per_second,
    )
    .unwrap();

    assert!(err.action.contains("'car' profile 1"), "{}", err.action);
    assert!(err.action.contains(NINE) && err.action.contains(NINE_EIGHT), "{}", err.action);
}

#[test]
fn refuses_a_fall_a_rounding_error_faster_than_time_and_asks_for_whole_seconds() {
    // 65.18 - 5.18 is 60.00000000000001 in floating point, a hair more than the 60 s between the points
    let matrices =
        matrix_with_duration_offsets(None, vec![(NINE, 65.18), ("1970-01-01T09:01:00Z", 5.18)], vec![0, 1, 0, 0]);

    let err = check_duration_offsets(
        &matrices,
        &create_empty_problem(),
        check_e9503_duration_offsets_fall_at_most_one_second_per_second,
    )
    .unwrap();

    assert_eq!(err.code, "E9503");
    assert!(err.action.contains("checked exactly (whole seconds are exact)"), "{}", err.action);
}

parameterized_test! {can_detect_duration_offsets_on_a_time_dependent_matrix, (timestamp, cells, expected), {
    can_detect_duration_offsets_on_a_time_dependent_matrix_impl(timestamp, cells, expected);
}}

fn can_detect_duration_offsets_on_a_time_dependent_matrix_impl(
    timestamp: Option<&str>,
    cells: Vec<u16>,
    expected: Option<String>,
) {
    let matrices = matrix_with_duration_offsets(timestamp, vec![(NINE, 0.)], cells);

    let result =
        check_duration_offsets(&matrices, &create_empty_problem(), check_e9504_duration_offsets_without_timestamp);

    assert_eq!(result.map(|err| err.code), expected);
}

can_detect_duration_offsets_on_a_time_dependent_matrix! {
    case01_timestamp_and_offsets: (Some(NINE), vec![0, 1, 0, 0], Some("E9504".to_string())),
    case02_no_timestamp: (None, vec![0, 1, 0, 0], None),
    case03_zero_cells_with_a_timestamp: (Some(NINE), vec![0, 0, 0, 0], None),
}

parameterized_test! {can_detect_duration_offsets_with_required_breaks, (vehicle_break, cells, expected), {
    can_detect_duration_offsets_with_required_breaks_impl(vehicle_break, cells, expected);
}}

fn can_detect_duration_offsets_with_required_breaks_impl(
    vehicle_break: Option<VehicleBreak>,
    cells: Vec<u16>,
    expected: Option<String>,
) {
    let problem = Problem {
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift {
                    breaks: vehicle_break.map(|vehicle_break| vec![vehicle_break]),
                    ..create_default_vehicle_shift()
                }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrices = matrix_with_duration_offsets(None, vec![(NINE, 0.)], cells);

    let result = check_duration_offsets(&matrices, &problem, check_e9505_duration_offsets_without_required_breaks);

    assert_eq!(result.map(|err| err.code), expected);
}

can_detect_duration_offsets_with_required_breaks! {
    case01_required_break_and_offsets: (Some(required()), vec![0, 1, 0, 0], Some("E9505".to_string())),
    case02_optional_break_and_offsets: (Some(optional()), vec![0, 1, 0, 0], None),
    case03_zero_cells_with_a_required_break: (Some(required()), vec![0, 0, 0, 0], None),
    case04_no_break: (None, vec![0, 1, 0, 0], None),
}
