# Routing matrix format

In general, routing matrix has the following schema:

- `profile` (required for time dependent VRP) is name of vehicle profile
- `timestamp` (optional) a date in RFC3999 for which routing info is applicable. Can be used for time dependent VRP.
- `travelTimes` (required) is square matrix of durations in abstract time units represented via single dimensional array
- `distances` (required) is square matrix of distances in abstract distance unit represented via single dimensional array
- `errorCodes` (optional): must be present if there is no route between some locations. Non-zero value signalizes about
    routing error.
- `oppositeSideArrivals` (optional): indices into the single dimensional array of cells whose route arrives with its
    destination on the vehicle's opposite side, e.g. `[2]` for AC in the example below. Used by the
    `minimize-opposite-side-arrivals` objective, which ignores cells from a location to itself. Not allowed on a matrix
    with a `timestamp`.
- `durationOffsets` (optional): delays keyed by departure time on chosen cells, e.g. a rush-hour queue on a bridge; see
    [Duration offsets](#duration-offsets) below.

Both durations and distances are mapped to the list of unique locations generated from the problem definition. In this
list, locations are specified in the order they defined. For example, if you have two jobs with locations A and B, one
vehicle type with depot location C, then you have the following location list: A,B,C. It corresponds to the matrix (durations
or distances):

|    |    |    |
|----|----|----|
|  0 | AB | AC |
| BA |  0 | BC |
| CA | CB |  0 |


where
- `0`: zero duration or distance
- `XY`: distance or duration from X location to Y

As single dimensional array it looks like:

    [0,AB,AC,BA,0,BC,CA,CB,0]


`vrp-cli` command provides a helper command to get it as well as `pragmatic` lib exposes method to get the list
pragmatically:

```
vrp-cli solve pragmatic problem.json --get-locations -o locations.json
```

The output format is a simply array of unique geo locations:

```json
{{#include ../../../../../examples/data/pragmatic/simple.basic.locations.json}}
```

You can use it to get a routing matrix from any of routing services of your choice, but the order in resulting matrix
should be kept as expected.


Routing matrix example:

```json
{{#include ../../../../../examples/data/pragmatic/simple.basic.matrix.json}}
```

## Duration offsets

Some roads are slow only at certain times, e.g. a queue on a bridge or at a junction during rush hour. Modelling this
with one timestamped matrix per period repeats the whole matrix for a few slow cells. Instead, a single matrix can carry
`durationOffsets`: delays which depend on the departure time and apply only to the chosen cells.

- `profiles`: a list of delay profiles. A profile has `points`, each with an RFC3339 `time` and an `offset` in seconds.
    The offset is linear between points and held at the first (last) point's value before (after) it. A band on several
    dates is simply more points.
- `cells`: one unsigned 16-bit entry per matrix cell, in the same order as `travelTimes`. `0` means no offset, `k` means
    `profiles[k - 1]`. A cell costs two bytes whatever the number of points.

The rules:

- A leg takes its base travel time times the vehicle profile `scale`, plus the offset at the departure from the previous
    place, i.e. after the service there. The offset itself is not scaled.
- Lookups keyed by arrival are exact: the leg uses the latest departure which still arrives in time.
- Offsets are non-negative and may fall at most one second per second (E9503), so leaving later never means arriving
    earlier.
- Distances and time-independent estimates keep the base values.
- A cell into or out of a custom location gets no offset, and unreachable cells stay unreachable.
- With ferry crossings, each drive on the way takes the offset at its own departure.
- Duration offsets cannot be combined with a matrix `timestamp` (E9504) or with required breaks (E9505).
- A matrix without `durationOffsets`, or one where every cell is `0`, is solved as before.
- Tools which read `travelTimes` directly, such as the ride quality analysis, use the base durations.

In the example below, cell `0 -> 1` has a queue which builds up from 06:52 to 07:00 and eases out from 09:00 to 09:08.
Its 600 s base duration costs +240 s when leaving at 06:56, +480 s at 07:30, +240 s at 09:04 and nothing at 09:10:

```json
{{#include ../../../../../examples/data/pragmatic/basics/duration-offsets.basic.matrix.json}}
```

If you have already your routing matrix, you can use location indices instead of geocoordinates as described
[here](../routing/index.md#location-format).


## Experimental

Additionally, you can use a custom type of location with `type`=`unknown` to model a zero distance/duration to
any other location. This could be useful to model unknown location for vehicle start.