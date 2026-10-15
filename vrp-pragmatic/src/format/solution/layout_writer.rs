#[cfg(test)]
#[path = "../../../tests/unit/format/solution/layout_writer_test.rs"]
mod layout_writer_test;

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

    best.iter().zip(loads).zip(best.iter().skip(1)).try_fold(vec![first_idx], |mut chosen, ((prev, prev_load), next)| {
        let current_idx = *chosen.last()?;
        let target = prev[current_idx]?;
        let allows_optimum = |layout_idx: usize| {
            next[layout_idx].is_some_and(|rest| add(get_switches(prev_load, current_idx, layout_idx), rest) == target)
        };

        let next_idx = if allows_optimum(current_idx) {
            current_idx
        } else {
            (0..next.len()).find(|&layout_idx| allows_optimum(layout_idx))?
        };
        chosen.push(next_idx);

        Some(chosen)
    })
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
