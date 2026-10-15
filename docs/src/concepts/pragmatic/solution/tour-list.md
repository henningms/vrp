# Tour list

List of tours is essentially individual vehicle routes. Each tour consists of the following properties:

* **typeId**: id of vehicle type
    ```json
    {{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:18}}
    ```
* **vehicleId**: id of used vehicle. Id of the vehicle is generated from the tour using pattern `$typeId_sequenceIndex`:
    ```json
    {{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:17}}
    ```
* **shiftIndex**: vehicle's shift index:
    ```json
    {{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:19}}
    ```
* **stops**: list of stops. See stop structure below
* **capacityLayouts** (optional): spans of consecutive stops in the same capacity layout. Only set for vehicle types
    with `capacityConfigurations`, see [capacity layouts](#capacity-layouts) below.
* **statistic**: statistic of the tour.
    ```json
    {{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:144:155}}
    ```

## Stop structure

Stop represents a location vehicle has to visit within activities to be performed. It has the following properties:

* **location**: a stop location
* **time** (required): arrival and departure time from the stop
* **distance**: distance traveled since departure from start location
* **load**: (required) vehicle capacity after departure from the stop
* **layout** (optional): capacity layout of the vehicle after departure from the stop: its `index` in the vehicle
    type's `capacityConfigurations`, counted from 0, and its `name`, if any. Only set for vehicle types with
    `capacityConfigurations`.
* **parking** (optional): parking time. Used only with vicinity clustering.
* **activities** (required): list of activities to be performed at the stop. Each stop can have more than one activity.
    See activity structure below.

Please note, that `location` and `distance` are not required: they are omitted in case of the stop for a required break
which during traveling.

Please check examples [here](../../../examples/pragmatic/basics/break.md).


## Activity structure

An activity specifies work to be done and has the following structure:

* **jobId** (required): id of the job or special id (`departure`, `arrival`, `break`, `reload`)
* **type** (required):  activity type: `departure`, `arrival`, `break`, `reload`, `pickup` or `delivery`
* **location** (optional): activity location. Omitted if stop list has one activity
* **time** (optional): start and end time of activity. Omitted if stop list has one activity
* **jobTag** (optional): a job place tag
* **commute** (optional): commute information. Used only with vicinity clustering.

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

## Examples

An example of stop with one activity:
```json
{{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:21:40}}
```

An example of stop with two activities:
```json
{{#include ../../../../../examples/data/pragmatic/simple.basic.solution.json:61:101}}
```
