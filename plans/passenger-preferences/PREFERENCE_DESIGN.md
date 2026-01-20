# Generic Preference System Design

This document describes how to implement a generic preference system for VRP, inspired by the Skills feature but as a **soft constraint** instead of a hard constraint.

## Comparison: Skills vs Preferences

### Skills (Hard Constraint)
- **Purpose:** Vehicle MUST have required skills to serve a job
- **Behavior:** Job is **rejected** if vehicle lacks skills
- **Use case:** Driver must have forklift certification to deliver pallets

### Preferences (Soft Constraint)
- **Purpose:** Job PREFERS certain vehicle attributes but can be served by others
- **Behavior:** Job can be assigned to any vehicle, but non-preferred assignments incur a **penalty cost**
- **Use case:** Passenger prefers driver A or B, but can ride with driver C if needed

## How Skills Work

### Data Structure
```rust
pub struct JobSkills {
    pub all_of: Option<HashSet<String>>,   // Vehicle must have ALL of these
    pub one_of: Option<HashSet<String>>,   // Vehicle must have AT LEAST ONE
    pub none_of: Option<HashSet<String>>,  // Vehicle must have NONE of these
}
```

### JSON Format
```json
{
  "id": "job1",
  "skills": {
    "allOf": ["fridge"],           // Vehicle MUST have "fridge" skill
    "noneOf": ["washing_machine"]  // Vehicle MUST NOT have "washing_machine" skill
  }
}
```

### Implementation
- **FeatureConstraint** - Returns `ConstraintViolation::fail()` if skills don't match
- **No FeatureObjective** - Not a cost, just yes/no
- **No FeatureState** - No caching needed

## Proposed: Generic Preference System

### Data Structure (Core)
```rust
// vrp-core/src/construction/features/preferences.rs

/// Job preferences for vehicle attributes.
/// Similar to JobSkills but used for soft constraints (penalties) instead of hard constraints.
pub struct JobPreferences {
    /// Prefer vehicles with ALL of these attributes.
    /// Cost penalty applied if any attribute is missing.
    pub all_of: Option<HashSet<String>>,

    /// Prefer vehicles with AT LEAST ONE of these attributes.
    /// Cost penalty applied if none match.
    pub one_of: Option<HashSet<String>>,

    /// Prefer vehicles with NONE of these attributes.
    /// Cost penalty applied if any attribute is present.
    pub none_of: Option<HashSet<String>>,
}

impl JobPreferences {
    pub fn new(
        all_of: Option<Vec<String>>,
        one_of: Option<Vec<String>>,
        none_of: Option<Vec<String>>
    ) -> Self {
        let map: fn(Option<Vec<_>>) -> Option<HashSet<_>> = |prefs| {
            prefs.and_then(|v| if v.is_empty() { None } else { Some(v.into_iter().collect()) })
        };

        Self {
            all_of: map(all_of),
            one_of: map(one_of),
            none_of: map(none_of),
        }
    }
}

// Define custom dimensions
custom_dimension!(pub JobPreferences typeof JobPreferences);
custom_dimension!(pub VehicleAttributes typeof HashSet<String>);
```

### JSON Format (Pragmatic)
```json
{
  "plan": {
    "jobs": [
      {
        "id": "passenger1",
        "deliveries": [...],
        "preferences": {
          "oneOf": ["driver:alice", "driver:bob"],     // Prefers Alice or Bob
          "noneOf": ["vehicle:old_van"]                // Prefers not to ride in old van
        }
      },
      {
        "id": "passenger2",
        "deliveries": [...],
        "preferences": {
          "allOf": ["driver:alice", "vehicle:suv"]     // Wants Alice AND an SUV
        }
      }
    ]
  },
  "fleet": {
    "vehicles": [
      {
        "typeId": "alice_suv",
        "vehicleIds": ["alice_vehicle_1"],
        "attributes": ["driver:alice", "vehicle:suv"]
      },
      {
        "typeId": "bob_sedan",
        "vehicleIds": ["bob_vehicle_1"],
        "attributes": ["driver:bob", "vehicle:sedan"]
      }
    ]
  }
}
```

### Implementation (Soft Constraint)

```rust
/// Creates a preference feature as soft constraint (objective).
pub fn create_preferences_feature(
    name: &str,
    penalty: PreferencePenalty,
) -> Result<Feature, GenericError> {
    FeatureBuilder::default()
        .with_name(name)
        .with_objective(PreferencesObjective { penalty })
        .with_state(PreferencesState { penalty })
        .build()
}

/// Configurable penalty structure
pub struct PreferencePenalty {
    /// Penalty per missing attribute in allOf
    pub all_of_penalty: Cost,
    /// Penalty if none of oneOf attributes match
    pub one_of_penalty: Cost,
    /// Penalty per unwanted attribute in noneOf
    pub none_of_penalty: Cost,
}

impl Default for PreferencePenalty {
    fn default() -> Self {
        Self {
            all_of_penalty: 100.0,   // High penalty per missing required attribute
            one_of_penalty: 50.0,    // Medium penalty if no preferred option available
            none_of_penalty: 75.0,   // High penalty per unwanted attribute present
        }
    }
}

struct PreferencesObjective {
    penalty: PreferencePenalty,
}

impl FeatureObjective for PreferencesObjective {
    fn fitness(&self, solution: &InsertionContext) -> Cost {
        // Get cached solution-level fitness
        solution.solution.state
            .get_value::<PreferenceFitnessKey, Cost>()
            .copied()
            .unwrap_or_else(|| calculate_solution_fitness(&self.penalty, &solution.solution))
    }

    fn estimate(&self, move_ctx: &MoveContext<'_>) -> Cost {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => {
                calculate_job_penalty(&self.penalty, job, route_ctx)
            }
            MoveContext::Activity { .. } => 0.0,
        }
    }
}

struct PreferencesState {
    penalty: PreferencePenalty,
}

impl FeatureState for PreferencesState {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, _job: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
    }

    fn accept_route_state(&self, route_ctx: &mut RouteContext) {
        let penalty = calculate_route_penalty(&self.penalty, route_ctx);
        route_ctx.state_mut().set_tour_state::<PreferenceRouteKey, _>(penalty);
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        let total_penalty = calculate_solution_fitness(&self.penalty, solution_ctx);
        solution_ctx.state.set_value::<PreferenceFitnessKey, _>(total_penalty);
    }
}

// State keys
struct PreferenceFitnessKey;
struct PreferenceRouteKey;
```

### Penalty Calculation Logic

```rust
/// Calculate penalty for assigning a job to a route
fn calculate_job_penalty(
    penalty_config: &PreferencePenalty,
    job: &Job,
    route_ctx: &RouteContext,
) -> Cost {
    let preferences = match job.dimens().get_job_preferences() {
        Some(prefs) => prefs,
        None => return 0.0,  // No preferences = no penalty
    };

    let vehicle_attrs = route_ctx.route().actor.vehicle.dimens.get_vehicle_attributes();

    let mut total_penalty = 0.0;

    // Check allOf: all required attributes must be present
    if let Some(all_of) = &preferences.all_of {
        let missing_count = match vehicle_attrs {
            Some(attrs) => all_of.iter().filter(|attr| !attrs.contains(*attr)).count(),
            None => all_of.len(),  // Vehicle has no attributes, all are missing
        };
        total_penalty += (missing_count as Cost) * penalty_config.all_of_penalty;
    }

    // Check oneOf: at least one preferred attribute must be present
    if let Some(one_of) = &preferences.one_of {
        let has_any_match = vehicle_attrs
            .map(|attrs| one_of.iter().any(|attr| attrs.contains(attr)))
            .unwrap_or(false);

        if !has_any_match {
            total_penalty += penalty_config.one_of_penalty;
        }
    }

    // Check noneOf: none of these attributes should be present
    if let Some(none_of) = &preferences.none_of {
        let unwanted_count = vehicle_attrs
            .map(|attrs| none_of.iter().filter(|attr| attrs.contains(*attr)).count())
            .unwrap_or(0);
        total_penalty += (unwanted_count as Cost) * penalty_config.none_of_penalty;
    }

    total_penalty
}

/// Calculate total penalty for all jobs in a route
fn calculate_route_penalty(
    penalty_config: &PreferencePenalty,
    route_ctx: &RouteContext,
) -> Cost {
    route_ctx.route().tour.jobs()
        .map(|job| calculate_job_penalty(penalty_config, job, route_ctx))
        .sum()
}

/// Calculate total penalty across entire solution
fn calculate_solution_fitness(
    penalty_config: &PreferencePenalty,
    solution_ctx: &SolutionContext,
) -> Cost {
    solution_ctx.routes.iter()
        .map(|route_ctx| calculate_route_penalty(penalty_config, route_ctx))
        .sum()
}
```

## Usage Example (Core API)

```rust
use vrp_core::prelude::*;
use std::collections::HashSet;

// Define problem with preferences
let job = SingleBuilder::default()
    .id("passenger1")
    .demand(Demand::delivery(1))
    .location(1)?
    .dimension(|dimens| {
        dimens.set_job_preferences(JobPreferences::new(
            None,                                    // allOf
            Some(vec!["driver:alice".to_string()]), // oneOf - prefers Alice
            None,                                    // noneOf
        ));
    })
    .build_as_job()?;

let vehicle = VehicleBuilder::default()
    .id("alice_vehicle")
    .dimension(|dimens| {
        dimens.set_vehicle_attributes(
            vec!["driver:alice".to_string(), "vehicle:suv".to_string()]
                .into_iter()
                .collect::<HashSet<_>>()
        );
    })
    .capacity(SingleDimLoad::new(4))
    .build()?;

// Create goal with preferences
let preferences_feature = create_preferences_feature(
    "preferences",
    PreferencePenalty::default(),
)?;

let goal = GoalContextBuilder::with_features(&[
    minimize_unassigned,
    capacity_feature,
    transport_feature,
    preferences_feature,  // Add preferences to optimization goal
])?
.build()?;

// Build and solve
let problem = ProblemBuilder::default()
    .add_jobs(vec![job].into_iter())
    .add_vehicles(vec![vehicle].into_iter())
    .with_goal(goal)
    .build()?;

let solution = Solver::new(problem, config).solve()?;
```

## Advantages of Generic Attributes

Using generic string attributes (like `"driver:alice"`, `"vehicle:suv"`, `"shift:morning"`) provides:

1. **Flexibility:** No need to create separate features for each preference type
2. **Extensibility:** Users can define any attributes without code changes
3. **Composability:** Combine multiple preference types (driver + vehicle + shift)
4. **Future-proof:** Easy to add new preference dimensions

## Comparison: Skills vs Preferences

| Aspect | Skills (Hard) | Preferences (Soft) |
|--------|--------------|-------------------|
| **Trait** | FeatureConstraint | FeatureObjective |
| **Returns** | ConstraintViolation or None | Cost (penalty) |
| **Behavior** | Reject assignment | Penalize assignment |
| **JSON field** | `"skills"` | `"preferences"` |
| **Vehicle field** | `"skills"` | `"attributes"` |
| **Use case** | "Must have forklift cert" | "Prefers driver Alice" |
| **Override** | Cannot override | Can override if needed |

## Tuning Penalty Values

The penalty values determine how strongly preferences influence routing decisions:

```rust
// Strong preferences - rarely violated
PreferencePenalty {
    all_of_penalty: 1000.0,   // Very high cost per missing attribute
    one_of_penalty: 500.0,
    none_of_penalty: 750.0,
}

// Weak preferences - easily overridden for efficiency
PreferencePenalty {
    all_of_penalty: 10.0,     // Low cost per missing attribute
    one_of_penalty: 5.0,
    none_of_penalty: 7.5,
}

// Balanced (default)
PreferencePenalty {
    all_of_penalty: 100.0,
    one_of_penalty: 50.0,
    none_of_penalty: 75.0,
}
```

**Rule of thumb:** Penalty should be comparable to distance cost. If serving a passenger 1 km away costs ~1.0, then:
- Penalty of 10.0 = willing to drive 10 km extra to honor preference
- Penalty of 100.0 = willing to drive 100 km extra to honor preference

## Integration with Pragmatic Format

### In `vrp-pragmatic/src/format/problem/model.rs`:

```rust
/// Job preferences for vehicle attributes (soft constraint).
#[derive(Clone, Deserialize, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPreferences {
    /// Prefer vehicles with all of these attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<String>>,
    /// Prefer vehicles with at least one of these attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<String>>,
    /// Prefer vehicles with none of these attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none_of: Option<Vec<String>>,
}

// Add to Job struct
pub struct Job {
    // ... existing fields

    /// Job preferences (soft constraint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<JobPreferences>,
}

// Add to VehicleType struct
pub struct VehicleType {
    // ... existing fields

    /// Vehicle attributes for preference matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<String>>,
}
```

### Configuration

Allow users to configure penalty values:

```json
{
  "plan": { ... },
  "fleet": { ... },
  "objectives": {
    "primary": [
      {
        "type": "minimize-unassigned"
      },
      {
        "type": "minimize-preferences-violations",
        "options": {
          "allOfPenalty": 100.0,
          "oneOfPenalty": 50.0,
          "noneOfPenalty": 75.0
        }
      },
      {
        "type": "minimize-distance"
      }
    ]
  }
}
```

## Implementation Roadmap

1. **Phase 1: Core Feature (vrp-core)**
   - Create `vrp-core/src/construction/features/preferences.rs`
   - Define `JobPreferences` struct
   - Implement `PreferencesObjective`
   - Implement `PreferencesState`
   - Add tests

2. **Phase 2: Simple Example**
   - Create `vrp-core/examples/passenger_preferences.rs`
   - Test with 2 drivers, 4 passengers
   - Verify penalties are calculated correctly

3. **Phase 3: Pragmatic Integration**
   - Add JSON models in `vrp-pragmatic/src/format/problem/model.rs`
   - Add parsing in job reader
   - Add vehicle attributes parsing
   - Add to objective configuration

4. **Phase 4: Testing & Refinement**
   - Add comprehensive unit tests
   - Test with realistic scenarios
   - Tune default penalty values
   - Add documentation

## Next Steps

Would you like me to:
1. **Implement the core feature** in `vrp-core/src/construction/features/preferences.rs`?
2. **Create a simple example** in `vrp-core/examples/passenger_preferences.rs` first?
3. **Walk through the penalty calculation logic** in more detail?
4. **Show how to integrate with pragmatic format** step-by-step?

This design gives you a production-ready, generic preference system that mirrors the Skills feature but as a soft constraint!
