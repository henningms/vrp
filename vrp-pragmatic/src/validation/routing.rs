#[cfg(test)]
#[path = "../../tests/unit/validation/routing_test.rs"]
mod routing_test;

use super::*;
use crate::parse_time_safe;
use crate::utils::combine_error_results;
use std::collections::HashSet;
use vrp_core::prelude::Float;

/// Checks that no duplicated profile names specified.
fn check_e1500_duplicated_profiles(ctx: &ValidationContext) -> Result<(), FormatError> {
    get_duplicates(ctx.problem.fleet.profiles.iter().map(|p| &p.name)).map_or(Ok(()), |names| {
        Err(FormatError::new(
            "E1500".to_string(),
            "duplicated profile names".to_string(),
            format!("remove duplicates of profiles with the names: '{}'", names.join(", ")),
        ))
    })
}

/// Checks that profiles collection is not empty.
fn check_e1501_empty_profiles(ctx: &ValidationContext) -> Result<(), FormatError> {
    if ctx.problem.fleet.profiles.is_empty() {
        Err(FormatError::new(
            "E1501".to_string(),
            "empty profile collection".to_string(),
            "specify at least one profile".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Checks that only one type of location is used.
fn check_e1502_no_location_type_mix(_ctx: &ValidationContext, location_types: (bool, bool)) -> Result<(), FormatError> {
    let (has_coordinates, has_indices) = location_types;

    if has_coordinates && has_indices {
        Err(FormatError::new(
            "E1502".to_string(),
            "mixing different location types".to_string(),
            "use either coordinates or indices for all locations".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Checks that routing matrix is supplied when location indices are used.
fn check_e1503_no_matrix_when_indices_used(
    ctx: &ValidationContext,
    location_types: (bool, bool),
) -> Result<(), FormatError> {
    let (_, has_indices) = location_types;

    if has_indices && ctx.matrices.is_none_or(|matrices| matrices.is_empty()) {
        Err(FormatError::new(
            "E1503".to_string(),
            "location indices requires routing matrix to be specified".to_string(),
            "either use coordinates everywhere or specify routing matrix".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Checks that coord index has a proper maximum index for
fn check_e1504_index_size_mismatch(ctx: &ValidationContext) -> Result<(), FormatError> {
    let max_index = ctx.coord_index.max_matrix_index();

    let (matrix_size, is_correct_index) = ctx
        .matrices
        .and_then(|matrices| matrices.first())
        .map(|matrix| (matrix.distances.len() as Float).sqrt().round() as usize)
        .map_or((0_usize, true), |matrix_size| (matrix_size, max_index + 1 == matrix_size));

    if !is_correct_index {
        Err(FormatError::new(
            "E1504".to_string(),
            "amount of locations does not match matrix dimension".to_string(),
            format!(
                "check matrix size: max location index '{max_index}' + 1 should be equal to matrix size ('{matrix_size}')"
            ),
        ))
    } else {
        Ok(())
    }
}

/// Checks that no duplicated profile names specified.
fn check_e1505_profiles_exist(ctx: &ValidationContext) -> Result<(), FormatError> {
    let known_matrix_profiles = ctx.problem.fleet.profiles.iter().map(|p| p.name.clone()).collect::<HashSet<_>>();

    let unknown_vehicle_profiles = ctx
        .problem
        .fleet
        .vehicles
        .iter()
        .map(|vehicle| vehicle.profile.matrix.clone())
        .chain(ctx.problem.plan.clustering.iter().map(|clustering| match clustering {
            Clustering::Vicinity { profile, .. } => profile.matrix.clone(),
        }))
        .filter(|matrix| !known_matrix_profiles.contains(matrix))
        .collect::<HashSet<_>>();

    if unknown_vehicle_profiles.is_empty() {
        Ok(())
    } else {
        let unknown_profiles = unknown_vehicle_profiles.into_iter().collect::<Vec<_>>();
        Err(FormatError::new(
            "E1505".to_string(),
            "unknown matrix profile name in vehicle or vicinity clustering profile".to_string(),
            format!("ensure that matrix profiles '{}' are defined in profiles", unknown_profiles.join(", ")),
        ))
    }
}

/// Checks that opposite-side arrivals mark cells of their matrix.
fn check_e1506_opposite_side_arrivals_in_matrix(ctx: &ValidationContext) -> Result<(), FormatError> {
    let outside = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter())
        .filter_map(|matrix| {
            let side = (matrix.travel_times.len() as Float).sqrt().round() as usize;
            let cells = side * side;
            let cell = matrix.opposite_side_arrivals.as_ref()?.iter().find(|&&cell| cell >= cells)?;
            Some(match &matrix.profile {
                Some(profile) => format!("{cell} of {cells} cells in profile '{profile}'"),
                None => format!("{cell} of {cells} cells"),
            })
        })
        .collect::<Vec<_>>();

    if outside.is_empty() {
        Ok(())
    } else {
        Err(FormatError::new(
            "E1506".to_string(),
            "opposite-side arrival outside the matrix".to_string(),
            format!("check opposite-side arrival cells: {}", outside.join(", ")),
        ))
    }
}

/// Checks that opposite-side arrivals are set on matrices without a timestamp.
fn check_e1507_opposite_side_arrivals_without_timestamp(ctx: &ValidationContext) -> Result<(), FormatError> {
    let has_time_dependent_arrivals = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter())
        .any(|matrix| matrix.timestamp.is_some() && matrix.opposite_side_arrivals.is_some());

    if has_time_dependent_arrivals {
        Err(FormatError::new(
            "E1507".to_string(),
            "opposite-side arrivals on a time-dependent matrix".to_string(),
            "remove opposite-side arrivals from matrices with a timestamp".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Names a matrix by its profile, or by its position when it has none.
fn matrix_name(index: usize, matrix: &Matrix) -> String {
    matrix.profile.clone().unwrap_or_else(|| format!("#{index}"))
}

fn has_non_zero_duration_offset(matrix: &Matrix) -> bool {
    matrix.duration_offsets.as_ref().is_some_and(|offsets| offsets.cells.iter().any(|&cell| cell != 0))
}

/// Parses the times of an offset profile when it is a valid one: points, RFC3339 times in strictly increasing
/// order, and finite non-negative offsets.
fn parse_offset_profile(profile: &DurationOffsetProfile) -> Option<Vec<(Float, Float)>> {
    let points = profile
        .points
        .iter()
        .map(|point| parse_time_safe(&point.time).ok().map(|time| (time, point.offset)))
        .collect::<Option<Vec<_>>>()?;

    let is_valid = !points.is_empty()
        && points.iter().all(|&(_, offset)| offset.is_finite() && offset >= 0.)
        && points.windows(2).all(|pair| pair[0].0 < pair[1].0);

    is_valid.then_some(points)
}

/// Checks that the cells of duration offsets match their matrix and name existing profiles.
fn check_e9501_duration_offset_cells(ctx: &ValidationContext) -> Result<(), FormatError> {
    let details = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter().enumerate())
        .filter_map(|(index, matrix)| matrix.duration_offsets.as_ref().map(|offsets| (index, matrix, offsets)))
        .filter_map(|(index, matrix, offsets)| {
            let name = matrix_name(index, matrix);
            if offsets.cells.len() != matrix.travel_times.len() {
                return Some(format!(
                    "'{name}' has {} cells for {} travel times",
                    offsets.cells.len(),
                    matrix.travel_times.len()
                ));
            }

            let count = offsets.profiles.len();
            offsets
                .cells
                .iter()
                .enumerate()
                .find(|&(_, &cell)| cell as usize > count)
                .map(|(cell, &profile)| format!("'{name}' cell {cell} names profile {profile} of {count}"))
        })
        .collect::<Vec<_>>();

    if details.is_empty() {
        Ok(())
    } else {
        Err(FormatError::new(
            "E9501".to_string(),
            "duration offset cells do not match the matrix".to_string(),
            format!("check durationOffsets.cells of matrices: {}", details.join(", ")),
        ))
    }
}

/// Checks that every duration offset profile has valid points.
fn check_e9502_duration_offset_profiles(ctx: &ValidationContext) -> Result<(), FormatError> {
    let details = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter().enumerate())
        .filter_map(|(index, matrix)| matrix.duration_offsets.as_ref().map(|offsets| (index, matrix, offsets)))
        .flat_map(|(index, matrix, offsets)| {
            let name = matrix_name(index, matrix);
            offsets.profiles.iter().enumerate().filter_map(move |(profile, value)| {
                let k = profile + 1;
                if value.points.is_empty() {
                    Some(format!("'{name}' profile {k} has no points"))
                } else if parse_offset_profile(value).is_none() {
                    Some(format!("'{name}' profile {k} has an invalid time or offset"))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();

    if details.is_empty() {
        Ok(())
    } else {
        Err(FormatError::new(
            "E9502".to_string(),
            "invalid duration offset profile".to_string(),
            format!("use RFC3339 times in increasing order and finite, non-negative offsets: {}", details.join(", ")),
        ))
    }
}

/// Checks that no valid duration offset profile falls faster than time passes.
fn check_e9503_duration_offsets_fall_at_most_one_second_per_second(ctx: &ValidationContext) -> Result<(), FormatError> {
    let details = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter().enumerate())
        .filter_map(|(index, matrix)| matrix.duration_offsets.as_ref().map(|offsets| (index, matrix, offsets)))
        .flat_map(|(index, matrix, offsets)| {
            let name = matrix_name(index, matrix);
            offsets.profiles.iter().enumerate().filter_map(move |(profile, value)| {
                let points = parse_offset_profile(value)?;
                let (at, drop) = points
                    .windows(2)
                    .enumerate()
                    .map(|(at, pair)| (at, pair[0].1 - pair[1].1))
                    .find(|&(at, drop)| drop > points[at + 1].0 - points[at].0)?;

                Some(format!(
                    "'{name}' profile {} falls {drop}s between '{}' and '{}'",
                    profile + 1,
                    value.points[at].time,
                    value.points[at + 1].time
                ))
            })
        })
        .collect::<Vec<_>>();

    if details.is_empty() {
        Ok(())
    } else {
        Err(FormatError::new(
            "E9503".to_string(),
            "duration offset profile falls faster than time".to_string(),
            format!("let each offset fall at most one second per second: {}", details.join(", ")),
        ))
    }
}

/// Checks that duration offsets are set on matrices without a timestamp.
fn check_e9504_duration_offsets_without_timestamp(ctx: &ValidationContext) -> Result<(), FormatError> {
    let has_time_dependent_offsets = ctx
        .matrices
        .iter()
        .flat_map(|matrices| matrices.iter())
        .any(|matrix| matrix.timestamp.is_some() && has_non_zero_duration_offset(matrix));

    if has_time_dependent_offsets {
        Err(FormatError::new(
            "E9504".to_string(),
            "duration offsets on a time-dependent matrix".to_string(),
            "remove durationOffsets from matrices with a timestamp".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Checks that duration offsets are not combined with required breaks.
fn check_e9505_duration_offsets_without_required_breaks(ctx: &ValidationContext) -> Result<(), FormatError> {
    let has_offsets = ctx.matrices.iter().flat_map(|matrices| matrices.iter()).any(has_non_zero_duration_offset);
    let has_required_break = ctx
        .problem
        .fleet
        .vehicles
        .iter()
        .flat_map(|vehicle| vehicle.shifts.iter())
        .filter_map(|shift| shift.breaks.as_ref())
        .flat_map(|breaks| breaks.iter())
        .any(|vehicle_break| matches!(vehicle_break, VehicleBreak::Required { .. }));

    if has_offsets && has_required_break {
        Err(FormatError::new(
            "E9505".to_string(),
            "duration offsets with required breaks".to_string(),
            "remove durationOffsets from the matrices or required breaks from the vehicle shifts: they cannot be combined"
                .to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Validates routing rules.
pub fn validate_routing(ctx: &ValidationContext) -> Result<(), MultiFormatError> {
    let location_types = (ctx.coord_index.has_coordinates(), ctx.coord_index.has_indices());

    combine_error_results(&[
        check_e1500_duplicated_profiles(ctx),
        check_e1501_empty_profiles(ctx),
        check_e1502_no_location_type_mix(ctx, location_types),
        check_e1503_no_matrix_when_indices_used(ctx, location_types),
        check_e1504_index_size_mismatch(ctx),
        check_e1505_profiles_exist(ctx),
        check_e1506_opposite_side_arrivals_in_matrix(ctx),
        check_e1507_opposite_side_arrivals_without_timestamp(ctx),
        check_e9501_duration_offset_cells(ctx),
        check_e9502_duration_offset_profiles(ctx),
        check_e9503_duration_offsets_fall_at_most_one_second_per_second(ctx),
        check_e9504_duration_offsets_without_timestamp(ctx),
        check_e9505_duration_offsets_without_required_breaks(ctx),
    ])
    .map_err(From::from)
}
