#[cfg(test)]
#[path = "../../../tests/unit/format/solution/layout_writer_test.rs"]
mod layout_writer_test;

use super::*;

/// Writes the capacity layouts of a tour: `layout` on every stop and `capacity_layouts` on the tour.
///
/// `activity_loads` holds, for every point stop in tour order, the vehicle load after each of its activities, and
/// `layouts` and `names` the capacities and names of the vehicle's layouts in input order. Leaves the tour unchanged
/// when some load fits no layout.
pub(super) fn write_capacity_layouts(
    tour: &mut Tour,
    activity_loads: &[Vec<Vec<i32>>],
    layouts: &[Vec<i32>],
    names: &[Option<String>],
) {
    let Some(stop_layouts) = get_stop_layouts(&tour.stops, activity_loads, layouts) else {
        return;
    };

    let stop_layouts = stop_layouts
        .into_iter()
        .map(|index| CapacityLayout { index, name: names.get(index).cloned().flatten() })
        .collect::<Vec<_>>();

    tour.capacity_layouts = Some(create_capacity_layout_spans(&stop_layouts));
    tour.stops.iter_mut().zip(stop_layouts).for_each(|(stop, layout)| *stop.layout_mut() = Some(layout));
}

/// Groups consecutive stops with the same capacity layout into spans. `layouts` holds the layout of every stop of a
/// tour, in tour order.
pub(crate) fn create_capacity_layout_spans(layouts: &[CapacityLayout]) -> Vec<CapacityLayoutSpan> {
    layouts.iter().enumerate().fold(Vec::<CapacityLayoutSpan>::new(), |mut spans, (stop_idx, layout)| {
        match spans.last_mut() {
            Some(span) if span.index == layout.index && span.name == layout.name => span.to_stop = stop_idx,
            _ => spans.push(CapacityLayoutSpan {
                index: layout.index,
                name: layout.name.clone(),
                from_stop: stop_idx,
                to_stop: stop_idx,
            }),
        }

        spans
    })
}

/// Returns the layout index of every stop, or `None` when some load fits no layout.
fn get_stop_layouts(stops: &[Stop], activity_loads: &[Vec<Vec<i32>>], layouts: &[Vec<i32>]) -> Option<Vec<usize>> {
    let point_stops = stops.iter().filter(|stop| matches!(stop, Stop::Point(_))).count();
    debug_assert_eq!(point_stops, activity_loads.len(), "expected activity loads for every point stop");
    if point_stops != activity_loads.len() {
        return None;
    }

    let loads = activity_loads.iter().flatten().cloned().collect::<Vec<_>>();
    let chosen = choose_layouts(&loads, layouts)?;

    // NOTE a point stop gets the layout chosen for its last activity
    let mut last_activity_idx = 0;
    let mut point_layouts = activity_loads.iter().map(|stop_loads| {
        last_activity_idx += stop_loads.len();
        last_activity_idx.checked_sub(1).map(|activity_idx| chosen[activity_idx])
    });

    // NOTE a transit stop is a break on the way to the next stop, so it keeps the layout of the stop before it
    let mut current = None;
    stops
        .iter()
        .map(|stop| {
            if matches!(stop, Stop::Point(_)) {
                current = point_layouts.next().flatten();
            }
            current
        })
        .collect()
}

/// Layout switches between consecutive activities: with passengers aboard first, then in total.
type Switches = (usize, usize);

/// Chooses a capacity layout for every activity of a tour.
///
/// `loads` holds the vehicle load after each activity, from departure to arrival, and `layouts` the capacities of the
/// vehicle's layouts in input order. A switch after an activity is made with passengers aboard unless the vehicle is
/// empty after that activity.
///
/// The choice makes the fewest switches with passengers aboard, then the fewest switches in total. At departure, ties
/// go to the lowest index. Later, the current layout is kept while it still allows the optimum, so a switch happens as
/// late as possible, and a switch takes the lowest index which allows the optimum.
///
/// Returns the layout index of every activity, or `None` when some load fits no layout.
fn choose_layouts(loads: &[Vec<i32>], layouts: &[Vec<i32>]) -> Option<Vec<usize>> {
    if loads.is_empty() {
        return Some(vec![]);
    }

    // NOTE backward pass: the fewest switches from each activity to arrival when the activity uses a given layout
    let mut best: Vec<Vec<Option<Switches>>> = vec![vec![None; layouts.len()]; loads.len()];
    for (activity_idx, load) in loads.iter().enumerate().rev() {
        for layout_idx in (0..layouts.len()).filter(|&layout_idx| fits(load, &layouts[layout_idx])) {
            let value = match best.get(activity_idx + 1) {
                None => Some((0, 0)),
                Some(next) => next
                    .iter()
                    .enumerate()
                    .filter_map(|(next_idx, rest)| rest.map(|rest| add(get_switches(load, layout_idx, next_idx), rest)))
                    .min(),
            };
            best[activity_idx][layout_idx] = value;
        }
    }

    // NOTE forward pass: start with the lowest index which allows the optimum, then keep the current layout while it
    // still allows the optimum and switch to the lowest such index otherwise
    let (_, first_idx) =
        best[0].iter().enumerate().filter_map(|(layout_idx, value)| value.map(|value| (value, layout_idx))).min()?;

    best.iter().zip(loads).zip(best.iter().skip(1)).try_fold(
        vec![first_idx],
        |mut chosen, ((prev, prev_load), next)| {
            let current_idx = *chosen.last()?;
            let target = prev[current_idx]?;
            let allows_optimum = |layout_idx: usize| {
                next[layout_idx]
                    .is_some_and(|rest| add(get_switches(prev_load, current_idx, layout_idx), rest) == target)
            };

            let next_idx = if allows_optimum(current_idx) {
                current_idx
            } else {
                (0..next.len()).find(|&layout_idx| allows_optimum(layout_idx))?
            };
            chosen.push(next_idx);

            Some(chosen)
        },
    )
}

/// Returns true when no dimension of the load exceeds the layout's capacity. Missing dimensions count as 0.
fn fits(load: &[i32], layout: &[i32]) -> bool {
    load.iter().enumerate().all(|(dim, value)| *value <= layout.get(dim).copied().unwrap_or(0))
}

/// Returns the switches made by changing the layout `from` to `to` after an activity which leaves `load` aboard.
fn get_switches(load: &[i32], from: usize, to: usize) -> Switches {
    match (from == to, load.iter().all(|value| *value == 0)) {
        (true, _) => (0, 0),
        (false, true) => (0, 1),
        (false, false) => (1, 1),
    }
}

fn add(lhs: Switches, rhs: Switches) -> Switches {
    (lhs.0 + rhs.0, lhs.1 + rhs.1)
}
