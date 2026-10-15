# Capacity Layouts in the Solution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every tour of a vehicle type with `capacityConfigurations` reports the layout in use at each stop (`layout`)
and the spans of stops in one layout (`capacityLayouts`), and the checker verifies them.

**Architecture:** Output only, the solver is unchanged. The fleet reader marks vehicles which have layouts with a new
pragmatic dimension holding the layout names. The solution writer records the vehicle load after every activity while
it builds a tour, and a new pure module `layout_writer.rs` chooses a layout per activity (backward DP plus a forward
pass with tie rules) and writes the stop and tour fields. The checker validates the fields when a tour has them.

**Tech Stack:** Rust 2024 (workspace `rust-version` 1.88), serde, `vrp-pragmatic` crate.

**Spec:** `docs/superpowers/specs/2026-10-15-capacity-layout-output-design.md`. Read it before starting.

## Global Constraints

- No change to `vrp-core` or to the solver. Only `vrp-pragmatic` (and docs/CHANGELOG) change.
- JSON names, exactly: tour field `capacityLayouts`, span fields `index`, `name`, `fromStop`, `toStop`; stop field
  `layout` with `index` and `name`.
- Rust types in `vrp-pragmatic/src/format/solution/model.rs`: `CapacityLayout { index: usize, name: Option<String> }`,
  `CapacityLayoutSpan { index: usize, name: Option<String>, from_stop: usize, to_stop: usize }`,
  `Tour.capacity_layouts: Option<Vec<CapacityLayoutSpan>>`, `PointStop.layout` and `TransitStop.layout`:
  `Option<CapacityLayout>`.
- All new fields: serialized camelCase, skipped when `None`, `None` when absent on parse. `name` is omitted when the
  input layout has no name.
- `index` is the position in the vehicle type's `capacityConfigurations`, counted from 0. `fromStop`/`toStop` are
  inclusive positions in `stops`, counted from 0; spans cover all stops, in order, without gaps or overlaps.
- Only tours of vehicle types with `capacityConfigurations` get the fields; plain-`capacity` tours are unchanged, also
  in a mixed fleet. If some activity's load fits no layout, the writer leaves both fields out for that tour.
- At most 8 layouts (`MAX_CONFIGURATIONS`), so activities × layouts² is cheap.
- CI gates: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-features --tests --examples -- -D warnings`. `#![warn(missing_docs)]` is on: every
  new `pub` item and field needs a doc comment.
- Work on branch `feat/capacity-layout-output`. Conventional commit messages (`feat(pragmatic): ...`). No attribution
  lines in commits. Commits are signed automatically by the local git config.
- The version stays 1.39.2 in this plan. The 1.40.0 release (marked `breaking` in the CHANGELOG) happens after the PR
  merges, as its own step.
- Ferd (`apps/optimizer/src/feasibility/init_solution.rs`) is a separate repo: on its version bump it needs
  `layout: None` and `capacity_layouts: None`. Not part of this plan.

## Review Focus

1. A required break during travel becomes a transit stop between two point stops: it must keep the layout of the stop
   before it, and the spans must count it. Test: Task 3 `can_keep_layout_of_previous_stop_at_transit_stop`.
2. The departure stop also holds pickups at the depot location: its layout is the one after its last activity, and the
   recorded activity loads must line up with the stops. Test: Task 4
   `can_report_capacity_layout_of_departure_stop_with_pickup`.
3. A load fits no layout (the known solver gap, or an inconsistent route): both fields are left out for the tour, with
   no panic. Tests: Task 2 case `case09_load_fits_no_layout`, Task 3 `can_leave_layouts_out_when_load_fits_no_layout`.
4. A vehicle type with a single configuration (like the minibus in the mixed-fleet test): fields present, index 0 on
   every stop, one span. Tests: Task 3 `can_write_single_layout`, Task 4 mixed-fleet assertions.
5. A layout without a name: `name` is omitted on stops and spans, in Rust and in JSON. Tests: Task 1
   `can_serialize_capacity_layouts_in_camel_case`, Task 3 `can_keep_layout_of_previous_stop_at_transit_stop`.

## File Structure

- `vrp-pragmatic/src/format/solution/model.rs` (modify): new types, new fields, `Stop::layout()`/`layout_mut()`.
- `vrp-pragmatic/src/format/solution/layout_writer.rs` (create): choosing layouts (pure) and writing them on a `Tour`;
  also the span grouping shared with the checker.
- `vrp-pragmatic/src/format/solution/mod.rs` (modify): declare the module and its imports.
- `vrp-pragmatic/src/format/solution/solution_writer.rs` (modify): record activity loads, call the layout writer.
- `vrp-pragmatic/src/format/solution/break_writer.rs` (modify): `layout: None` on the transit stop literal.
- `vrp-pragmatic/src/format/dimensions.rs` (modify): `CapacityLayoutNames` dimension.
- `vrp-pragmatic/src/format/problem/fleet_reader.rs` (modify): set the dimension.
- `vrp-pragmatic/src/checker/capacity.rs` (modify): `check_capacity_layouts`.
- Tests: `vrp-pragmatic/tests/unit/format/solution/model_test.rs` (create),
  `vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs` (create),
  `vrp-pragmatic/tests/features/capacity/capacity_layout_test.rs` (create),
  `vrp-pragmatic/tests/features/capacity/mod.rs`, `.../configurable_capacity_test.rs`,
  `vrp-pragmatic/tests/unit/format/problem/reader_test.rs`, `vrp-pragmatic/tests/unit/checker/capacity_test.rs`,
  `vrp-pragmatic/tests/helpers/solution.rs` and the struct-literal sites listed in Task 1 (modify).
- Docs: `docs/src/concepts/pragmatic/solution/tour-list.md`, `CHANGELOG.md` (modify).

Test command for this crate: `cargo test -p vrp-pragmatic <filter>`. Unit and feature tests compile into the lib test
target, so a filter on the test name works for both.

---

### Task 1: Solution model fields

**Files:**
- Modify: `vrp-pragmatic/src/format/solution/model.rs`
- Modify (struct literals): `vrp-pragmatic/src/format/solution/solution_writer.rs`,
  `vrp-pragmatic/src/format/solution/break_writer.rs`, `vrp-pragmatic/tests/helpers/solution.rs`,
  `vrp-pragmatic/tests/features/clustering/mod.rs`, `vrp-pragmatic/tests/unit/checker/assignment_test.rs`,
  `vrp-pragmatic/tests/unit/checker/limits_test.rs`, `vrp-pragmatic/tests/unit/format/solution/geo_serializer_test.rs`,
  `vrp-pragmatic/tests/unit/format/solution/initial_reader_test.rs`
- Create test: `vrp-pragmatic/tests/unit/format/solution/model_test.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub struct CapacityLayout { pub index: usize, pub name: Option<String> }` (Clone, Eq, PartialEq, Debug, serde)
  - `pub struct CapacityLayoutSpan { pub index: usize, pub name: Option<String>, pub from_stop: usize, pub to_stop: usize }`
  - `Tour.capacity_layouts: Option<Vec<CapacityLayoutSpan>>`, `PointStop.layout`, `TransitStop.layout: Option<CapacityLayout>`
  - `Stop::layout(&self) -> Option<&CapacityLayout>`, `Stop::layout_mut(&mut self) -> &mut Option<CapacityLayout>`
  - Test helpers: `StopBuilder::layout(self, index: usize, name: Option<&str>) -> Self`,
    `TourBuilder::capacity_layouts(self, spans: Vec<CapacityLayoutSpan>) -> Self`

- [ ] **Step 1: Write the failing tests**

Create `vrp-pragmatic/tests/unit/format/solution/model_test.rs`:

```rust
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
```

Register it at the very top of `vrp-pragmatic/src/format/solution/model.rs`, before `use super::FeatureCollection;`:

```rust
#[cfg(test)]
#[path = "../../../tests/unit/format/solution/model_test.rs"]
mod model_test;

```

Add the two builder methods to `vrp-pragmatic/tests/helpers/solution.rs`.

In `impl StopBuilder`, after `pub fn load(...)`:

```rust
    pub fn layout(mut self, index: usize, name: Option<&str>) -> Self {
        *self.stop.layout_mut() = Some(CapacityLayout { index, name: name.map(str::to_string) });

        self
    }
```

In `impl TourBuilder`, after `pub fn stops(...)`:

```rust
    pub fn capacity_layouts(mut self, spans: Vec<CapacityLayoutSpan>) -> Self {
        self.tour.capacity_layouts = Some(spans);

        self
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vrp-pragmatic capacity_layouts`
Expected: compile error, e.g. `cannot find struct, variant or union type CapacityLayoutSpan` and
`no method named layout_mut found for enum Stop`.

- [ ] **Step 3: Add the types and fields**

In `vrp-pragmatic/src/format/solution/model.rs`, add right before `/// A stop is a place where vehicle is supposed to do
some work.`:

```rust
/// A capacity layout of a vehicle: one of its vehicle type's `capacityConfigurations`.
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CapacityLayout {
    /// Position of the layout in the vehicle type's `capacityConfigurations`, counted from 0.
    pub index: usize,
    /// Name of the layout. Omitted when the layout has no name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A span of consecutive stops of a tour in which the vehicle uses the same capacity layout.
#[derive(Clone, Deserialize, Serialize, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CapacityLayoutSpan {
    /// Position of the layout in the vehicle type's `capacityConfigurations`, counted from 0.
    pub index: usize,
    /// Name of the layout. Omitted when the layout has no name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Position of the first stop of the span in the tour's stops, counted from 0.
    pub from_stop: usize,
    /// Position of the last stop of the span in the tour's stops, inclusive.
    pub to_stop: usize,
}
```

In `impl Stop`, after `load_mut`:

```rust
    /// Returns stop's capacity layout.
    pub fn layout(&self) -> Option<&CapacityLayout> {
        match self {
            Self::Transit(transit) => transit.layout.as_ref(),
            Self::Point(point) => point.layout.as_ref(),
        }
    }

    /// Returns stop's capacity layout as mutable.
    pub fn layout_mut(&mut self) -> &mut Option<CapacityLayout> {
        match self {
            Self::Transit(transit) => &mut transit.layout,
            Self::Point(point) => &mut point.layout,
        }
    }
```

In both `TransitStop` and `PointStop`, right after the `load` field:

```rust
    /// Capacity layout of the vehicle after departure from this stop. Set only for vehicle types with
    /// `capacityConfigurations`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<CapacityLayout>,
```

In `Tour`, right after the `shift_index` field:

```rust
    /// Spans of consecutive stops in the same capacity layout, in tour order. Set only for vehicle types with
    /// `capacityConfigurations`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_layouts: Option<Vec<CapacityLayoutSpan>>,
```

- [ ] **Step 4: Fix every struct literal**

Add the new field to each literal. The writer fills the fields later (Task 4), so all of them start as `None`:

- `vrp-pragmatic/src/format/solution/solution_writer.rs`, `create_tour`:
  - the `let mut tour = Tour { ... }` literal: after `shift_index: ...unwrap(),` add `capacity_layouts: None,`;
  - the departure `PointStop`: after `load: start_delivery.as_vec(),` add `layout: None,`;
  - the new-stop `PointStop` (inside `if is_new_stop`): after `load: prev_load.as_vec(),` add `layout: None,`.
- `vrp-pragmatic/src/format/solution/break_writer.rs`, the `TransitStop` literal: after `load,` add `layout: None,`.
- `vrp-pragmatic/tests/helpers/solution.rs`: `StopBuilder::new_transit` (`TransitStop`) and `Default for StopBuilder`
  (`PointStop`): after `load: vec![],` add `layout: None,`; `Default for TourBuilder` (`Tour`): after
  `shift_index: 0,` add `capacity_layouts: None,`.
- `vrp-pragmatic/tests/features/clustering/mod.rs`, the `PointStop`: after `load: vec![stop.load],` add `layout: None,`.
- `vrp-pragmatic/tests/unit/checker/assignment_test.rs`, both `Tour` literals: after `shift_index,` add
  `capacity_layouts: None,`.
- `vrp-pragmatic/tests/unit/checker/limits_test.rs`, the `Tour`: after `shift_index: 0,` add `capacity_layouts: None,`.
- `vrp-pragmatic/tests/unit/format/solution/geo_serializer_test.rs`, the `PointStop`: after `load: vec![],` add
  `layout: None,`.
- `vrp-pragmatic/tests/unit/format/solution/initial_reader_test.rs`, the `Tour`: after `shift_index: 0,` add
  `capacity_layouts: None,`.

Then run `cargo test --workspace --no-run`. If the compiler reports `missing field` (E0063) anywhere else, add the
field there the same way.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p vrp-pragmatic capacity_layouts`
Expected: the 4 tests in `model_test` pass.

Run: `cargo test -p vrp-pragmatic`
Expected: all pass (the fields are `None` everywhere and skipped in JSON, so no existing assertion changes).

- [ ] **Step 6: Commit**

```bash
git add vrp-pragmatic/src/format/solution/model.rs vrp-pragmatic/src/format/solution/solution_writer.rs \
  vrp-pragmatic/src/format/solution/break_writer.rs vrp-pragmatic/tests
git commit -m "feat(pragmatic): add capacity layout fields to the solution model"
```

---

### Task 2: Choosing layouts per activity

**Files:**
- Create: `vrp-pragmatic/src/format/solution/layout_writer.rs`
- Modify: `vrp-pragmatic/src/format/solution/mod.rs`
- Create test: `vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces (private to `layout_writer`, used by Task 3):
  `fn choose_layouts(loads: &[Vec<i32>], layouts: &[Vec<i32>]) -> Option<Vec<usize>>`, plus helper
  `fn fits(load: &[i32], layout: &[i32]) -> bool`.

Rules being implemented (from the spec, "Choosing the layouts"): a load fits a layout when no dimension exceeds the
layout's capacity (missing dimensions count as 0). A switch between activity `i` and `i + 1` is *empty* when the load
after activity `i` is all zeros, otherwise it is *with passengers aboard*. Objective: fewest switches with passengers
aboard, then fewest switches in total. Ties: at departure the lowest index among optimal layouts; later keep the
current layout if it fits and still allows the optimum (so a switch happens as late as possible); when a switch is
needed, the lowest index which allows the optimum. Returns `None` when some load fits no layout.

- [ ] **Step 1: Write the failing tests**

Create `vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs`:

```rust
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
```

What the cases pin:
- case03: `[2, 0]` fits both, but the vehicle is already in `one_wheelchair` (forced by `[0, 1]`), so it stays.
- case04: the switch to `all_seated` happens at `[6, 0]`, not at the first empty load: as late as possible.
- case06: going `all_seated → two_wheelchairs` at `[2, 1]` is one switch but with passengers aboard (`[3, 0]` is
  aboard). The expected answer makes two empty switches instead. A total-switches-first chooser would return
  `[0, 0, 0, 0, 2, 2, 2, 2]`, and a chooser which looks at the load after the *later* activity would too.
- case07: the solver-gap tour from the spec. The vehicle is never empty before `[6, 0]`, so the switch is aboard, and
  it happens at `[6, 0]` (index 3).

Create `vrp-pragmatic/src/format/solution/layout_writer.rs` with only the test registration for now:

```rust
#[cfg(test)]
#[path = "../../../tests/unit/format/solution/layout_writer_test.rs"]
mod layout_writer_test;
```

In `vrp-pragmatic/src/format/solution/mod.rs`, after the `mod initial_reader;` / `pub use ...read_init_solution;`
lines, add:

```rust
mod layout_writer;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vrp-pragmatic can_choose_layouts`
Expected: compile error `cannot find function choose_layouts in this scope`.

- [ ] **Step 3: Implement the chooser**

Append to `vrp-pragmatic/src/format/solution/layout_writer.rs`:

```rust
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
```

Notes for the implementer:
- `best[i][k]` is `None` when layout `k` does not fit activity `i`, or when nothing from `i` to arrival can be
  assigned. So one unfittable load makes all of `best[0]` `None`, and the `?` after `.min()` returns `None`.
- The zip in the forward pass walks `(best[i - 1], loads[i - 1], best[i])` for `i = 1..n`. The `?` on
  `prev[current_idx]` and on `.find(...)` never fire on an optimal path; they only avoid a panic.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p vrp-pragmatic can_choose_layouts`
Expected: 12 passed.

A plain `cargo build -p vrp-pragmatic` now warns that `choose_layouts` is never used. That is expected until Task 3
calls it; don't silence it.

- [ ] **Step 5: Commit**

```bash
git add vrp-pragmatic/src/format/solution/layout_writer.rs vrp-pragmatic/src/format/solution/mod.rs \
  vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs
git commit -m "feat(pragmatic): choose capacity layouts for tour activities"
```

---

### Task 3: Writing layouts on a tour

**Files:**
- Modify: `vrp-pragmatic/src/format/solution/layout_writer.rs`
- Modify test: `vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs`

**Interfaces:**
- Consumes: Task 1 types and `Stop::layout()`/`layout_mut()`, test builders `StopBuilder::layout`,
  `TourBuilder::capacity_layouts`; Task 2 `choose_layouts`.
- Produces:
  - `pub(super) fn write_capacity_layouts(tour: &mut Tour, activity_loads: &[Vec<Vec<i32>>], layouts: &[Vec<i32>], names: &[Option<String>])`
    — `activity_loads[p]` is the list of loads after each activity of the `p`-th **point** stop, in tour order
    (transit stops have no entry). Sets every stop's `layout` and the tour's `capacity_layouts`, or leaves both
    untouched when some load fits no layout.
  - `pub(crate) fn create_capacity_layout_spans(layouts: &[CapacityLayout]) -> Vec<CapacityLayoutSpan>` — groups
    the per-stop layouts into maximal spans (same `index` and `name`). Task 5's checker uses it.

- [ ] **Step 1: Write the failing tests**

Append to `vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs`, and add `use crate::helpers::*;` below the
existing `use super::*;`:

```rust
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
```

Why the first two tests expect what they do:
- `can_use_layout_of_last_activity_for_stop`: per activity the loads are `[0,0] [0,1] | [0,0] [6,0] | [0,0] | [0,0]`,
  so the chooser gives `1 1 | 1 0 | 0 | 0` (the switch after the wheelchair delivery is empty, in the middle of a
  stop). Each stop shows its last activity's layout: `1, 0, 0, 0`.
- `can_keep_layout_of_previous_stop_at_transit_stop`: the transit stop (index 1) has no entry in `activity_loads` and
  takes the layout of stop 0; the spans count it. Layout 1 has no name, so `name` is `None` on stops and spans.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vrp-pragmatic layout_writer_test`
Expected: compile error `cannot find function write_capacity_layouts in this scope`.

- [ ] **Step 3: Implement the writer**

In `vrp-pragmatic/src/format/solution/layout_writer.rs`, add `use super::*;` after the `mod layout_writer_test;`
block, and add these functions above `choose_layouts`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p vrp-pragmatic layout_writer_test`
Expected: the 12 chooser cases and the 4 new tests pass.

`cargo build -p vrp-pragmatic` warns that `write_capacity_layouts` is never used until Task 4. Expected.

- [ ] **Step 5: Commit**

```bash
git add vrp-pragmatic/src/format/solution/layout_writer.rs vrp-pragmatic/tests/unit/format/solution/layout_writer_test.rs
git commit -m "feat(pragmatic): write capacity layouts on tour stops"
```

---

### Task 4: Report layouts in the solution

**Files:**
- Modify: `vrp-pragmatic/src/format/dimensions.rs`
- Modify: `vrp-pragmatic/src/format/problem/fleet_reader.rs`
- Modify: `vrp-pragmatic/src/format/solution/mod.rs`
- Modify: `vrp-pragmatic/src/format/solution/solution_writer.rs`
- Modify test: `vrp-pragmatic/tests/unit/format/problem/reader_test.rs`
- Create test: `vrp-pragmatic/tests/features/capacity/capacity_layout_test.rs`
- Modify test: `vrp-pragmatic/tests/features/capacity/mod.rs`,
  `vrp-pragmatic/tests/features/capacity/configurable_capacity_test.rs`
- Modify docs: `docs/src/concepts/pragmatic/solution/tour-list.md`, `CHANGELOG.md`

**Interfaces:**
- Consumes: Task 3 `write_capacity_layouts`; Task 1 `Stop::layout()`, `CapacityLayoutSpan`.
- Produces: `custom_dimension!(pub CapacityLayoutNames typeof Vec<Option<String>>)`, i.e. trait
  `CapacityLayoutNamesDimension` with `get_capacity_layout_names()` / `set_capacity_layout_names(..)` on
  `Dimensions`. Set only on vehicles of types with `capacityConfigurations`. Solutions now carry the fields.

- [ ] **Step 1: Write the failing tests**

(a) Reader test. Append to `vrp-pragmatic/tests/unit/format/problem/reader_test.rs`, and add
`use crate::format::CapacityLayoutNamesDimension;` to its imports:

```rust
#[test]
fn can_read_capacity_layout_names_only_for_vehicle_types_with_configurations() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_pickup_delivery_job_with_params(
                "job1",
                vec![1, 0],
                ((1., 0.), 60., vec![]),
                ((2., 0.), 60., vec![]),
            )],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![
                VehicleType {
                    capacity: None,
                    capacity_configurations: Some(vec![
                        CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
                        CapacityConfiguration { name: None, capacities: vec![4, 1] },
                    ]),
                    ..create_vehicle_with_capacity("minibus", vec![])
                },
                create_vehicle_with_capacity("car", vec![2, 0]),
            ],
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
            ..create_default_fleet()
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let core_problem = (problem, vec![matrix]).read_pragmatic().unwrap();

    let get_names = |vehicle_id: &str| {
        core_problem
            .fleet
            .vehicles
            .iter()
            .find(|vehicle| vehicle.dimens.get_vehicle_id().is_some_and(|id| id == vehicle_id))
            .expect("vehicle should exist")
            .dimens
            .get_capacity_layout_names()
            .cloned()
    };
    assert_eq!(get_names("minibus_1"), Some(vec![Some("all_seated".to_string()), None]));
    assert_eq!(get_names("car_1"), None);
}
```

(b) Feature tests. Create `vrp-pragmatic/tests/features/capacity/capacity_layout_test.rs`:

```rust
use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

type Trip = ((f64, f64), (f64, f64));

/// A minibus with layouts all_seated [8, 0] and one_wheelchair [4, 1], depot at (0, 0), and two trips: a wheelchair
/// passenger [0, 1] and a group of six seated passengers [6, 0], each from pickup to delivery location.
fn create_minibus_problem(wheelchair: Trip, seated: Trip) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![
                create_pickup_delivery_job_with_params(
                    "wheelchair_passenger",
                    vec![0, 1],
                    (wheelchair.0, 60., vec![]),
                    (wheelchair.1, 60., vec![]),
                ),
                create_pickup_delivery_job_with_params(
                    "seated_group",
                    vec![6, 0],
                    (seated.0, 60., vec![]),
                    (seated.1, 60., vec![]),
                ),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                capacity: None,
                capacity_configurations: Some(vec![
                    CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
                    CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![4, 1] },
                ]),
                ..create_vehicle_with_capacity("minibus", vec![])
            }],
            profiles: create_default_matrix_profiles(),
            resources: None,
            capacity_dimensions: Some(vec!["seated".to_string(), "wheelchair".to_string()]),
        },
        ..create_empty_problem()
    }
}

fn solve_minibus_problem(problem: Problem) -> Tour {
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none(), "expected all jobs to be assigned");
    solution.tours.into_iter().next().expect("expected a tour")
}

fn get_stop_layouts(tour: &Tour) -> Vec<Option<usize>> {
    tour.stops.iter().map(|stop| stop.layout().map(|layout| layout.index)).collect()
}

fn span(index: usize, name: &str, from_stop: usize, to_stop: usize) -> CapacityLayoutSpan {
    CapacityLayoutSpan { index, name: Some(name.to_string()), from_stop, to_stop }
}

/// The shortest feasible tour is (0,0) → (1,0) → (2,0) → (3,0) → (4,0) → (0,0): carrying the wheelchair passenger
/// past (3,0) would need [6, 1], which fits no layout. The vehicle is empty after (2,0), so it changes to all_seated
/// at (3,0) without passengers aboard.
#[test]
fn can_report_capacity_layouts_of_tour() {
    let problem = create_minibus_problem(((1., 0.), (2., 0.)), ((3., 0.), (4., 0.)));

    let tour = solve_minibus_problem(problem);

    assert_eq!(
        get_ids_from_tour(&tour),
        vec![
            vec!["departure"],
            vec!["wheelchair_passenger"],
            vec!["wheelchair_passenger"],
            vec!["seated_group"],
            vec!["seated_group"],
            vec!["arrival"],
        ]
    );
    assert_eq!(get_stop_layouts(&tour), vec![Some(1), Some(1), Some(1), Some(0), Some(0), Some(0)]);
    assert_eq!(tour.capacity_layouts, Some(vec![span(1, "one_wheelchair", 0, 2), span(0, "all_seated", 3, 5)]));
}

/// The wheelchair passenger boards at the depot, so the departure stop has the departure and the pickup, and it
/// leaves in one_wheelchair.
#[test]
fn can_report_capacity_layout_of_departure_stop_with_pickup() {
    let problem = create_minibus_problem(((0., 0.), (1., 0.)), ((2., 0.), (3., 0.)));

    let tour = solve_minibus_problem(problem);

    assert_eq!(
        get_ids_from_tour(&tour),
        vec![
            vec!["departure", "wheelchair_passenger"],
            vec!["wheelchair_passenger"],
            vec!["seated_group"],
            vec!["seated_group"],
            vec!["arrival"],
        ]
    );
    assert_eq!(get_stop_layouts(&tour), vec![Some(1), Some(1), Some(0), Some(0), Some(0)]);
    assert_eq!(tour.capacity_layouts, Some(vec![span(1, "one_wheelchair", 0, 1), span(0, "all_seated", 2, 4)]));
}
```

Register it in `vrp-pragmatic/tests/features/capacity/mod.rs`:

```rust
mod capacity_layout_test;
mod configurable_capacity_test;
mod simple_capacity_test;
```

(c) Mixed fleet. At the end of `can_use_plain_capacity_in_fleet_with_configurable_capacity` in
`vrp-pragmatic/tests/features/capacity/configurable_capacity_test.rs`, after the `unassigned` assertion, add:

```rust
    let get_tour = |vehicle_id: &str| solution.tours.iter().find(|tour| tour.vehicle_id == vehicle_id).unwrap();
    let car = get_tour("car_1");
    assert_eq!(car.capacity_layouts, None);
    assert!(car.stops.iter().all(|stop| stop.layout().is_none()));
    let minibus = get_tour("minibus_1");
    assert_eq!(
        minibus.capacity_layouts,
        Some(vec![CapacityLayoutSpan {
            index: 0,
            name: Some("one_wheelchair".to_string()),
            from_stop: 0,
            to_stop: minibus.stops.len() - 1,
        }])
    );
```

That file imports only `crate::format::problem::*` and `crate::helpers::*`, so add `use crate::format::solution::*;`
between them for `CapacityLayoutSpan` (other feature tests import both globs the same way).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vrp-pragmatic capacity_layout`
Expected: compile error `no method named get_capacity_layout_names` from the reader test. After Step 3 the reader
test passes and the feature tests still fail on their assertions (`left: [None, None, ...]`) until Step 4.

- [ ] **Step 3: Add the dimension and set it in the fleet reader**

In `vrp-pragmatic/src/format/dimensions.rs`, after `custom_dimension!(pub TourSize typeof usize);`:

```rust
// NOTE names of the vehicle's capacity layouts in input order, set only for vehicle types with capacityConfigurations
custom_dimension!(pub CapacityLayoutNames typeof Vec<Option<String>>);
```

In `vrp-pragmatic/src/format/problem/fleet_reader.rs`, inside `if props.has_configurable_capacity {`, after the
`if let Some(configurations) = configurations { dimens.set_vehicle_capacity(...); }` block:

```rust
                    // NOTE the names mark the vehicle as one with layouts, which the solution reports per stop
                    if let Some(configs) = vehicle.capacity_configurations.as_ref() {
                        dimens.set_capacity_layout_names(configs.iter().map(|config| config.name.clone()).collect());
                    }
```

Run: `cargo test -p vrp-pragmatic can_read_capacity_layout_names`
Expected: PASS.

- [ ] **Step 4: Record activity loads and call the layout writer**

In `vrp-pragmatic/src/format/solution/mod.rs`, under the `mod layout_writer;` line from Task 2, add:

```rust
use self::layout_writer::write_capacity_layouts;
```

In `vrp-pragmatic/src/format/solution/solution_writer.rs`:

1. Change the import `use vrp_core::construction::features::JobDemandDimension;` to
   `use vrp_core::construction::features::{JobDemandDimension, VehicleCapacityDimension};`.

2. Right after `let mut departures: Vec<Timestamp> = Vec::new();` in `create_tour`, add:

```rust
    // the vehicle load after each activity, one list per point stop pushed below, in the same order as `departures`.
    // Capacity layouts are chosen per activity, so the layout writer needs the loads within a stop too.
    let mut activity_loads: Vec<Vec<Vec<i32>>> = Vec::new();
```

3. In the departure branch, after `departures.push(start.schedule.departure);`, add:

```rust
            activity_loads.push(vec![start_delivery.as_vec()]);
```

4. Inside `if is_new_stop { ... }`, after `departures.push(act.schedule.departure);`, add:

```rust
                    activity_loads.push(vec![]);
```

5. After `last.load = load.as_vec();`, add:

```rust
                activity_loads[last_idx].push(load.as_vec());
```

6. After the `// NOTE remove redundant info from single activity on the stop` pass (the `.for_each(...)` that clears
   `activity.time` and `activity.location`) and before `tour.vehicle_id.clone_from(...)`, add:

```rust
    // NOTE after breaks are inserted, so transit stops get a layout and spans use the final stop positions
    if let Some(names) = vehicle.dimens.get_capacity_layout_names() {
        let layouts = vehicle
            .dimens
            .get_vehicle_capacity::<ConfigurableLoad>()
            .map(|capacity| capacity.configurations_as_vec())
            .unwrap_or_default();

        write_capacity_layouts(&mut tour, &activity_loads, &layouts, names);
    }
```

`ConfigurableLoad` is already in scope through `vrp_core::models::common::*`, and `get_capacity_layout_names` through
`crate::format::solution::*` (which re-imports `crate::format::*`). `configurations_as_vec` returns the layouts in
input order.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p vrp-pragmatic capacity`
Expected: the reader test, both feature tests, the mixed-fleet test and the existing capacity tests pass.

Run: `cargo test -p vrp-pragmatic`
Expected: all pass.

- [ ] **Step 6: Document the fields**

In `docs/src/concepts/pragmatic/solution/tour-list.md`:

After the `* **stops**: list of stops. See stop structure below` bullet, add:

```markdown
* **capacityLayouts** (optional): spans of consecutive stops in the same capacity layout. Only set for vehicle types
    with `capacityConfigurations`, see [capacity layouts](#capacity-layouts) below.
```

In the stop structure list, after the `* **load**: ...` bullet, add:

```markdown
* **layout** (optional): capacity layout of the vehicle after departure from the stop: its `index` in the vehicle
    type's `capacityConfigurations`, counted from 0, and its `name`, if any. Only set for vehicle types with
    `capacityConfigurations`.
```

Before `## Examples`, add:

````markdown
## Capacity layouts

A vehicle type with `capacityConfigurations` has several alternative layouts, and the solver accepts a load when it
fits any one of them. The solution reports which layout the vehicle uses:

* every stop has a `layout`, the layout after departure from the stop, the same moment `load` describes
* the tour has `capacityLayouts`, the spans of consecutive stops in the same layout, in tour order. `fromStop` and
  `toStop` are inclusive stop positions in `stops`, counted from 0, and the spans cover all stops

```json
"capacityLayouts": [
  { "index": 1, "name": "one-wheelchair", "fromStop": 0, "toStop": 2 },
  { "index": 0, "name": "all-seated", "fromStop": 3, "toStop": 5 }
]
```

When more than one layout fits, the layouts of a tour are chosen as follows:

1. the fewest layout changes with passengers aboard. A change is empty when the vehicle is empty at that moment, which
   can be in the middle of a stop, e.g. after its deliveries and before its pickups
2. then the fewest layout changes in total
3. at departure, ties go to the layout listed first in `capacityConfigurations`. Later, the vehicle keeps its layout
   while it can, so a change happens as late as possible, and a change takes the layout listed first. List the
   preferred layout first.

A layout change takes no time yet. The solver does not yet keep one layout for every stretch with passengers aboard
when jobs are loaded at the depot or brought back to it, so such a tour can show a change with passengers aboard. If a
load fits no layout, the tour has neither field.
````

In `CHANGELOG.md`, under `## [Unreleased]`, add:

```markdown
### Added

* `breaking`: the solution reports the capacity layout of tours of vehicle types with `capacityConfigurations`. Every
  stop has a `layout` (`index` in `capacityConfigurations` and `name`) for when the vehicle leaves it, and the tour has
  `capacityLayouts`, the spans of consecutive stops in the same layout. When several layouts fit, the fewest layout
  changes with passengers aboard win, then the fewest changes, and ties go to the layout listed first. The json format
  only gains optional fields. In Rust, `Tour`, `PointStop` and `TransitStop` have new public fields, so code which
  builds them with struct literals needs `capacity_layouts: None` and `layout: None`
```

- [ ] **Step 7: Commit**

```bash
git add vrp-pragmatic/src vrp-pragmatic/tests docs/src/concepts/pragmatic/solution/tour-list.md CHANGELOG.md
git commit -m "feat(pragmatic): report capacity layouts in the solution"
```

---

### Task 5: Check layouts in the solution checker

**Files:**
- Modify: `vrp-pragmatic/src/checker/capacity.rs`
- Modify: `vrp-pragmatic/src/format/solution/mod.rs`
- Modify test: `vrp-pragmatic/tests/unit/checker/capacity_test.rs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: Task 3 `create_capacity_layout_spans`; Task 1 types, `Stop::layout()`, test builders.
- Produces: `fn check_capacity_layouts(context: &CheckerContext) -> GenericResult<()>`, run from
  `check_vehicle_load`, so `CheckerContext::check()` covers it.

Rules, only for tours which have the fields (spec, "Checker"), checked in this order and reported as the first error:
1. a tour has `capacityLayouts` and a `layout` on every stop, or none of them;
2. its vehicle type has `capacityConfigurations`;
3. per stop: the `index` is in range, the `name` equals the configuration's name (both absent counts as equal), and the
   stop's `load` fits the layout (missing dimensions count as 0);
4. `capacityLayouts` equals the spans built from the stops' layouts. This covers order, gaps, overlaps, agreement with
   the stops and that spans are maximal, as the spec defines them.

- [ ] **Step 1: Write the failing tests**

Append to `vrp-pragmatic/tests/unit/checker/capacity_test.rs`:

```rust
fn create_layout_problem(vehicle: VehicleType) -> Problem {
    Problem {
        plan: create_empty_plan(),
        fleet: Fleet { vehicles: vec![vehicle], ..create_default_fleet() },
        ..create_empty_problem()
    }
}

fn create_minibus_with_layouts() -> VehicleType {
    VehicleType {
        capacity: None,
        capacity_configurations: Some(vec![
            CapacityConfiguration { name: Some("all_seated".to_string()), capacities: vec![8, 0] },
            CapacityConfiguration { name: Some("one_wheelchair".to_string()), capacities: vec![4, 1] },
        ]),
        ..create_vehicle_with_capacity("minibus", vec![])
    }
}

fn layout_span(index: usize, name: &str, from_stop: usize, to_stop: usize) -> CapacityLayoutSpan {
    CapacityLayoutSpan { index, name: Some(name.to_string()), from_stop, to_stop }
}

fn create_layout_tour() -> Tour {
    TourBuilder::default()
        .vehicle_id("minibus_1")
        .type_id("minibus")
        .stops(vec![
            StopBuilder::default().load(vec![0, 0]).layout(1, Some("one_wheelchair")).build_departure(),
            StopBuilder::default().load(vec![0, 1]).layout(1, Some("one_wheelchair")).build_single("job1", "pickup"),
            StopBuilder::default().load(vec![0, 0]).layout(1, Some("one_wheelchair")).build_single("job1", "delivery"),
            StopBuilder::default().load(vec![6, 0]).layout(0, Some("all_seated")).build_single("job2", "pickup"),
            StopBuilder::default().load(vec![0, 0]).layout(0, Some("all_seated")).build_single("job2", "delivery"),
            StopBuilder::default().load(vec![0, 0]).layout(0, Some("all_seated")).build_arrival(),
        ])
        .capacity_layouts(vec![layout_span(1, "one_wheelchair", 0, 2), layout_span(0, "all_seated", 3, 5)])
        .build()
}

fn check_layout_tour(vehicle: VehicleType, tour: Tour) -> GenericResult<()> {
    let solution = SolutionBuilder::default().tour(tour).build();
    let ctx = CheckerContext::new(create_example_problem(), create_layout_problem(vehicle), None, solution).unwrap();

    check_capacity_layouts(&ctx)
}

const BOTH_OR_NEITHER: &str =
    "capacity layouts in tour 'minibus_1' must be set on the tour and on every stop, or not at all";
const SPANS_MISMATCH: &str = "capacity layout spans do not match the stops' layouts in tour 'minibus_1'";

parameterized_test! {can_check_capacity_layouts, (modify, expected), {
    can_check_capacity_layouts_impl(modify, expected);
}}

can_check_capacity_layouts! {
    case01_valid: (|_: &mut Tour| {}, Ok(())),
    case02_no_layouts: (|tour: &mut Tour| {
        tour.capacity_layouts = None;
        tour.stops.iter_mut().for_each(|stop| *stop.layout_mut() = None);
    }, Ok(())),
    case03_stop_without_layout: (|tour: &mut Tour| *tour.stops[1].layout_mut() = None, Err(BOTH_OR_NEITHER.into())),
    case04_no_spans: (|tour: &mut Tour| tour.capacity_layouts = None, Err(BOTH_OR_NEITHER.into())),
    case05_spans_without_stop_layouts: (|tour: &mut Tour| {
        tour.stops.iter_mut().for_each(|stop| *stop.layout_mut() = None);
    }, Err(BOTH_OR_NEITHER.into())),
    case06_index_out_of_range: (
        |tour: &mut Tour| *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 5, name: None }),
        Err("capacity layout 5 at stop 3 in tour 'minibus_1' is not in capacityConfigurations".into()),
    ),
    case07_name_mismatch: (
        |tour: &mut Tour| {
            *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 0, name: Some("one_wheelchair".to_string()) })
        },
        Err("capacity layout 0 at stop 3 in tour 'minibus_1' has another name than in capacityConfigurations".into()),
    ),
    case08_load_does_not_fit: (
        |tour: &mut Tour| {
            *tour.stops[3].layout_mut() = Some(CapacityLayout { index: 1, name: Some("one_wheelchair".to_string()) })
        },
        Err("load does not fit capacity layout 1 at stop 3 in tour 'minibus_1'".into()),
    ),
    case09_span_gap: (
        |tour: &mut Tour| {
            tour.capacity_layouts =
                Some(vec![layout_span(1, "one_wheelchair", 0, 1), layout_span(0, "all_seated", 3, 5)])
        },
        Err(SPANS_MISMATCH.into()),
    ),
    case10_span_not_maximal: (
        |tour: &mut Tour| {
            tour.capacity_layouts = Some(vec![
                layout_span(1, "one_wheelchair", 0, 2),
                layout_span(0, "all_seated", 3, 4),
                layout_span(0, "all_seated", 5, 5),
            ])
        },
        Err(SPANS_MISMATCH.into()),
    ),
}

fn can_check_capacity_layouts_impl(modify: impl FnOnce(&mut Tour), expected: GenericResult<()>) {
    let mut tour = create_layout_tour();
    modify(&mut tour);

    let result = check_layout_tour(create_minibus_with_layouts(), tour);

    assert_eq!(result, expected);
}

#[test]
fn can_check_capacity_layouts_of_vehicle_type_without_configurations() {
    let result = check_layout_tour(create_vehicle_with_capacity("minibus", vec![8, 1]), create_layout_tour());

    assert_eq!(
        result,
        Err("tour 'minibus_1' has capacity layouts, but its vehicle type 'minibus' has no capacityConfigurations".into())
    );
}
```

If `GenericResult` does not resolve in the test file, add `use vrp_core::prelude::GenericResult;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vrp-pragmatic can_check_capacity_layouts`
Expected: compile error `cannot find function check_capacity_layouts in this scope`.

- [ ] **Step 3: Implement the checker rule**

In `vrp-pragmatic/src/format/solution/mod.rs`, under `use self::layout_writer::write_capacity_layouts;`, add:

```rust
pub(crate) use self::layout_writer::create_capacity_layout_spans;
```

In `vrp-pragmatic/src/checker/capacity.rs`, replace `check_vehicle_load` with:

```rust
/// Checks that vehicle load is assigned correctly. The following rules are checked:
/// * max vehicle's capacity is not violated
/// * load change is correct
/// * capacity layouts are valid when a tour has them
pub fn check_vehicle_load(context: &CheckerContext) -> Result<(), Vec<GenericError>> {
    combine_error_results(&[
        check_vehicle_load_assignment(context),
        check_resource_consumption(context),
        check_capacity_layouts(context),
    ])
}
```

Add after `check_resource_consumption`:

```rust
/// Checks capacity layouts of tours which have them. The following rules are checked:
/// * a tour has `capacityLayouts` and a `layout` on every stop, or none of them
/// * the tour's vehicle type has `capacityConfigurations`
/// * every stop's layout is in `capacityConfigurations` with the same name, and the stop's load fits it
/// * `capacityLayouts` are the spans of consecutive stops with the same layout
fn check_capacity_layouts(context: &CheckerContext) -> GenericResult<()> {
    context.solution.tours.iter().try_for_each::<_, GenericResult<_>>(|tour| {
        let layouts = tour.stops.iter().filter_map(|stop| stop.layout().cloned()).collect::<Vec<_>>();

        let spans = match (layouts.len(), tour.capacity_layouts.as_ref()) {
            (0, None) => return Ok(()),
            (count, Some(spans)) if count == tour.stops.len() => spans,
            _ => {
                return Err(format!(
                    "capacity layouts in tour '{}' must be set on the tour and on every stop, or not at all",
                    tour.vehicle_id
                )
                .into());
            }
        };

        let vehicle = context.get_vehicle(&tour.vehicle_id)?;
        let configurations = vehicle.capacity_configurations.as_ref().ok_or_else(|| {
            GenericError::from(format!(
                "tour '{}' has capacity layouts, but its vehicle type '{}' has no capacityConfigurations",
                tour.vehicle_id, vehicle.type_id
            ))
        })?;

        tour.stops.iter().zip(layouts.iter()).enumerate().try_for_each::<_, GenericResult<_>>(
            |(stop_idx, (stop, layout))| {
                let configuration = configurations.get(layout.index).ok_or_else(|| {
                    GenericError::from(format!(
                        "capacity layout {} at stop {stop_idx} in tour '{}' is not in capacityConfigurations",
                        layout.index, tour.vehicle_id
                    ))
                })?;

                if configuration.name != layout.name {
                    return Err(format!(
                        "capacity layout {} at stop {stop_idx} in tour '{}' has another name than in \
                         capacityConfigurations",
                        layout.index, tour.vehicle_id
                    )
                    .into());
                }

                let fits = stop
                    .load()
                    .iter()
                    .enumerate()
                    .all(|(dim, value)| *value <= configuration.capacities.get(dim).copied().unwrap_or(0));
                if !fits {
                    return Err(format!(
                        "load does not fit capacity layout {} at stop {stop_idx} in tour '{}'",
                        layout.index, tour.vehicle_id
                    )
                    .into());
                }

                Ok(())
            },
        )?;

        if create_capacity_layout_spans(&layouts) != *spans {
            return Err(
                format!("capacity layout spans do not match the stops' layouts in tour '{}'", tour.vehicle_id).into()
            );
        }

        Ok(())
    })
}
```

`create_capacity_layout_spans` and the model types reach `capacity.rs` through `use super::*;` (the checker's
`use crate::format::solution::*;`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p vrp-pragmatic can_check_capacity_layouts`
Expected: 11 passed.

Run: `cargo test -p vrp-pragmatic`
Expected: all pass. The feature tests now also run the new rule on the writer's output, since the solver test helpers
run `CheckerContext::check()`.

- [ ] **Step 5: Note the checker in the CHANGELOG**

In `CHANGELOG.md`, under the `### Added` bullet from Task 4, add:

```markdown
* the solution checker checks the capacity layouts of tours which have them: both fields or neither, a vehicle type
  with `capacityConfigurations`, every stop's layout exists with the same name and fits the stop's load, and the spans
  match the stops. Tours without the fields are not checked
```

- [ ] **Step 6: Commit**

```bash
git add vrp-pragmatic/src/checker/capacity.rs vrp-pragmatic/src/format/solution/mod.rs \
  vrp-pragmatic/tests/unit/checker/capacity_test.rs CHANGELOG.md
git commit -m "feat(pragmatic): check capacity layouts in the solution checker"
```

---

## Final check

- [ ] Run `cargo fmt --all -- --check`. If it reports changes, run `cargo fmt --all` and commit them as
  `style: format capacity layout code`.
- [ ] Run `cargo clippy --workspace --all-features --tests --examples -- -D warnings`. Fix every warning. Don't
  allow-list. Commit fixes as `refactor(pragmatic): address clippy findings`.
- [ ] Run `cargo test --workspace`. Expected: all pass. This includes vrp-cli tests which solve the example problems,
  among them `examples/data/pragmatic/basics/capacity.configurable.problem.json`, and run the checker on the result.
- [ ] Check `git log --oneline master..` shows the plan commit plus one commit per task, and `git status` is clean.
