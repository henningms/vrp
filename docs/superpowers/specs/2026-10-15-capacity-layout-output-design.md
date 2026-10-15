# Capacity layout in the solution: design

Date: 2026-10-15
Status: approved in chat, awaiting spec review
Scope: piece 1 of 3 (report layouts). Pieces 2 and 3 are follow-ups, see below.

## Background

A vehicle type with `capacityConfigurations` has several alternative capacity layouts, e.g. a minibus with
`all-seated: [8, 0]`, `one-wheelchair: [4, 1]` and `two-wheelchairs: [2, 2]`. The solver accepts a load when it fits
any one of them, but the solution does not say which layout a tour uses or where it changes. Planners and drivers
need that: the bus has to be set up for it.

In the real world, layout changes come in two kinds:

- **Quick change**, e.g. folding or moving seats. It can happen during a tour, but it takes time.
- **Rebuild**, e.g. removing seats to go from 14 seats to 6 seats and 2 large wheelchairs. It needs a depot and is
  not done during a tour in practice, so the rebuilt layout is the one the vehicle starts its shift in.

This leads to three pieces of work:

1. **Report the layouts in the solution** (this spec). Output only, the solver is unchanged.
2. **Builds**: group layouts into builds which stay fixed for a shift. A planner can set the build for a shift, or
   the solver picks the build which gives the best plan overall.
3. **Quick-change time**: a reconfiguration activity with a duration when the layout changes within a build.

## Goal

Every tour of a vehicle type with `capacityConfigurations` reports the layout in use at each stop, and the stretches
of the tour (spans) in one layout. The output is designed so that pieces 2 and 3 fit later: a build on the tour, and
a reconfiguration activity at the stop where a span starts.

## Output format

```json
"tour": {
  "vehicleId": "minibus_1",
  "capacityLayouts": [
    { "index": 1, "name": "one-wheelchair", "fromStop": 0, "toStop": 2 },
    { "index": 0, "name": "all-seated", "fromStop": 3, "toStop": 5 }
  ],
  "stops": [
    { "load": [0, 1], "layout": { "index": 1, "name": "one-wheelchair" }, "...": "..." },
    { "load": [2, 1], "layout": { "index": 1, "name": "one-wheelchair" }, "...": "..." },
    { "load": [2, 0], "layout": { "index": 1, "name": "one-wheelchair" }, "...": "..." },
    { "load": [6, 0], "layout": { "index": 0, "name": "all-seated" }, "...": "..." },
    { "load": [0, 0], "layout": { "index": 0, "name": "all-seated" }, "...": "..." },
    { "load": [0, 0], "layout": { "index": 0, "name": "all-seated" }, "...": "..." }
  ]
}
```

- `layout` on a stop (`PointStop` and `TransitStop`) is the layout in use when the vehicle leaves the stop, the same
  moment `load` describes. Every stop has one: departure, stops with and without load, transit stops and arrival.
- `capacityLayouts` on the tour lists the spans in tour order. A span is a maximal stretch of consecutive stops with
  the same layout. `fromStop` and `toStop` are inclusive stop positions in `stops`, counted from 0. The spans cover
  all stops without gaps or overlaps.
- `index` is the position of the layout in the vehicle type's `capacityConfigurations`, counted from 0. `name` is
  the layout's `name` and is omitted when the input has none.
- A layout change is reported at the first stop whose `layout` shows the new layout. Where in the stretch the change
  happens follows from the rules in the next section.
- Only tours of vehicle types with `capacityConfigurations` get the fields. Tours of vehicle types with plain
  `capacity`, including those in a fleet which mixes both kinds, are unchanged.

Rust types, in `format/solution/model.rs`:

- `CapacityLayout { index: usize, name: Option<String> }`
- `CapacityLayoutSpan { index: usize, name: Option<String>, from_stop: usize, to_stop: usize }`
- `Tour.capacity_layouts: Option<Vec<CapacityLayoutSpan>>`
- `PointStop.layout` and `TransitStop.layout`: `Option<CapacityLayout>`

All new fields are serialized in camelCase, skipped when `None`, and default to `None` when a solution is parsed.

## Choosing the layouts

Several layouts often fit. For example, a stretch carrying `[2, 0]` fits both `all-seated` and `one-wheelchair`. The
writer picks them per tour as follows.

**Input**: the tour's activities in order, from departure to arrival, each with the vehicle load after it, and the
vehicle type's layouts in input order. These are the loads the writer already computes for `load`, so a layout is
chosen for every activity, not only for every stop. A stop's `layout` is the layout chosen for its last activity.

**Valid assignment**: every activity gets a layout which fits the load after it. A load fits a layout when no
dimension exceeds the layout's capacity (missing dimensions count as 0). Any layout fits an empty load.

**Switches**: a switch is a change of layout between two consecutive activities. It is an *empty switch* when the
load after the earlier activity is empty, and a *switch with passengers aboard* otherwise. So a switch in the middle
of a stop is empty when the vehicle is empty at that point of the stop, e.g. after the stop's deliveries and before
its pickups.

**Objective**, in order:

1. Fewest switches with passengers aboard.
2. Fewest switches in total.

**Ties**, which make the result deterministic:

- At departure, take the layout with the lowest index among those which allow the optimum.
- At every later activity, keep the current layout if it fits and keeping it still allows the optimum. So a switch
  happens as late as possible, e.g. an empty switch happens at the stop where the first passenger who needs the new
  layout boards.
- When a switch is needed, take the layout with the lowest index among those which allow the optimum. A planner can
  therefore list the preferred layout first.

**Algorithm**: a backward pass computes, for each activity and layout, the fewest (with passengers aboard, total)
switches from that activity to the end. A forward pass then applies the tie rules above. The cost is
activities × layouts², with at most 8 layouts.

A valid assignment exists whenever every activity's load fits some layout, which the solver's capacity feature keeps
and the checker verifies per stop. If an activity's load fits no layout anyway, the writer leaves both fields out for
that tour instead of failing; the checker then reports the load.

### Switches with passengers aboard

The solver can produce a tour which needs one. Found on 2026-10-15 with the layouts `all-seated: [8, 0]` and
`one-wheelchair: [4, 1]`: a wheelchair delivery loaded at the depot plus two seated pickup-delivery jobs gave this
tour:

| Stop | Load | Fits |
|---|---|---|
| 0 departure | `[0, 1]` | one-wheelchair |
| 1 pickup seated A | `[2, 1]` | one-wheelchair |
| 2 delivery wheelchair | `[2, 0]` | both |
| 3 pickup seated B | `[6, 0]` | all-seated |

The vehicle is not empty from departure to stop 3, and no single layout fits all of it, so the output switches at
stop 3 with two passengers aboard. Such a tour would not work in real life. The cause is in the solver: the capacity
feature checks that one layout fits each stretch with load aboard only for pickup-delivery jobs, not for jobs loaded
at or brought back to the depot. Fixing that is a follow-up. Until then, the output reports such tours as they are.

## Wiring

- **Fleet reader** (`format/problem/fleet_reader.rs`): for a vehicle type with `capacityConfigurations`, set a new
  pragmatic vehicle dimension with the layout names, e.g. `custom_dimension!(pub CapacityLayoutNames typeof
  Vec<Option<String>>)` in `format/dimensions.rs`. Its presence marks the vehicle as having layouts. It is needed
  because the core model has no layout names, and because vehicles with plain `capacity` in a mixed fleet also carry
  a `ConfigurableLoad` (with a single layout) since 1.39.2.
- **Layout writer** (new `format/solution/layout_writer.rs`): the algorithm above as a pure function over activity
  loads and layout capacities, plus a function which sets `layout` on the stops and `capacity_layouts` on the tour.
- **Solution writer** (`format/solution/solution_writer.rs`): at the end of `create_tour`, after breaks are inserted
  as stops, call the layout writer for vehicles with the new dimension. The layout capacities come from the vehicle's
  `ConfigurableLoad` (`configurations_as_vec`), whose order is the input order.

## Checker

In `checker/capacity.rs`, only for tours which have the fields:

- The tour's vehicle type has `capacityConfigurations`.
- Every `index` is in range, and every `name` matches the layout's name (both absent, or equal).
- Every stop's `load` fits its `layout`.
- The spans in `capacityLayouts` cover all stops without gaps or overlaps, in order, and agree with the stops'
  `layout`.
- A tour has both fields or neither.

A tour without the fields is not checked, so older solutions and solutions built by callers (e.g. Ferd's initial
solutions) still pass. The checker checks that the layouts are valid, not that they are the writer's choice.

## Compatibility

- **JSON**: additive. The fields are optional on input, so older solutions parse. The initial solution reader ignores
  them, since the writer computes them again.
- **Rust**: `Tour`, `PointStop` and `TransitStop` get new public fields, which breaks code constructing them with
  struct literals. The CHANGELOG marks it as `breaking`, and the release is a minor version (1.40.0).
- **Ferd**: `apps/optimizer/src/feasibility/init_solution.rs` constructs `PointStop` and `Tour` literals and needs
  `layout: None` and `capacity_layouts: None` on the version bump. Ferd's own HTTP schema (`apps/optimizer/src/
  schemas.rs`) is a separate type, so Ferd shows layouts only once it maps the new fields.

## Docs

- `docs/src/concepts/pragmatic/solution/tour-list.md`: the new fields and the rules for choosing layouts.
- CHANGELOG under `[Unreleased]`: `### Added` for the fields, with the Rust breaking change marked.
- The JSON schema (`schema` feature) gets the new types through their derives.

## Testing

- Layout writer unit tests (pure function):
  - one layout;
  - several layouts fit, so the lowest index wins;
  - keeping the current layout wins over a lower index;
  - an empty switch happens at the stop where the passenger needing the new layout boards;
  - a switch in the middle of a stop where the vehicle becomes empty counts as empty;
  - an empty switch earlier is preferred over a switch with passengers aboard later;
  - the solver-gap tour above, with the switch at stop 3;
  - a tour with no load at all gets the lowest index everywhere.
- Feature test: solve the wheelchair minibus example and assert the stops' `layout` and the tour's
  `capacityLayouts`.
- Feature test: in a mixed fleet, the tour of the vehicle type with plain `capacity` has no layout fields.
- Checker unit tests: each rule above fails on a broken solution, and a solution without the fields passes.
- Parsing test: a solution without the fields parses.

## Out of scope (follow-ups)

- Solver fix: one layout per stretch with load aboard also for jobs loaded at or brought back to the depot, so the
  solver no longer produces tours which need a switch with passengers aboard.
- Piece 2: builds, set by a planner per shift or picked by the solver.
- Piece 3: quick-change time as a reconfiguration activity.
- Layouts in the GeoJSON output.
- Ferd: map the new fields into its HTTP schema and UI.
