# Passenger Preference Feature - Implementation Summary

## Overview

Successfully implemented a job preference system as a soft constraint feature in the VRP solver. This allows jobs (e.g., passengers) to express preferences for vehicle attributes (e.g., preferred drivers) without making them hard requirements.

## Key Design Decisions

### 1. Soft Constraint Approach

**Decision**: Implemented as `FeatureObjective` (soft constraint) rather than `FeatureConstraint` (hard constraint).

**Rationale**:
- Preferences should guide the solver but not reject solutions
- Cost penalties encourage preferred assignments without guarantees
- Allows solver to find feasible solutions even when preferences conflict

### 2. Tiered Preference System

**Decision**: Three-tier system: `preferred`, `acceptable`, `avoid`

**Rationale**:
- More intuitive than `allOf/oneOf/noneOf` semantics for soft constraints
- `allOf/oneOf/noneOf` imply guarantees, which soft constraints cannot provide
- Tiered approach reflects real-world preference strength

**Penalty Structure** (defaults):
- `no_preferred_match`: 100.0 (high penalty if no preferred attributes match)
- `no_acceptable_match`: 30.0 (additional penalty if no acceptable match either)
- `per_avoided_present`: 75.0 (penalty for each unwanted attribute present)

### 3. Using Vehicle Skills as Attributes

**Decision**: Reuse vehicle `skills` field instead of separate `attributes` field.

**Rationale**:
- Same property can be hard constraint (via skills) or soft constraint (via preferences)
- Single source of truth - no duplication
- Simpler JSON format for users

Example:
```json
{
  "vehicles": [{
    "skills": ["driver:alice", "vehicle:suv", "shift:day"]
  }],
  "jobs": [{
    "skills": {"allOf": ["wheelchair"]},  // Hard: MUST have wheelchair access
    "preferences": {"preferred": ["driver:alice"]}  // Soft: PREFER driver Alice
  }]
}
```

### 4. Performance: No Route-Level Caching

**Decision**: Cache only at solution level, not route level.

**Rationale**:
- Simpler state management (no cache invalidation logic)
- Lower memory usage
- Preference calculation is lightweight (HashSet lookups only)
- Solution-level cache handles most cases efficiently
- For problems with 1000+ jobs, route-level caching could be added if profiling shows benefit

**Trade-offs documented in code**: See `vrp-core/src/construction/features/preferences.rs` lines 170-180.

## Implementation Details

### Core Feature

**Location**: `vrp-core/src/construction/features/preferences.rs`

**Key Components**:
- `JobPreferences`: Preference specification with three tiers
- `PreferencePenalty`: Configurable penalty values
- `create_preferences_feature()`: Feature factory function
- Automatic integration via `VehicleAttributesDimension` (set from skills)

### JSON Format Integration

**Model**: `vrp-pragmatic/src/format/problem/model.rs`
```json
{
  "jobs": [{
    "preferences": {
      "preferred": ["driver:alice", "vehicle:suv"],
      "acceptable": ["driver:bob"],
      "avoid": ["shift:night"]
    }
  }]
}
```

**Parsing**: Automatic preference detection in `problem_reader.rs` triggers feature creation.

### Testing

**Unit Tests**: 13 tests in `vrp-core/tests/unit/construction/features/preferences_test.rs`
- Preference matching logic
- Penalty calculation
- Combined scenarios

**Example**: `vrp-core/examples/passenger_preferences.rs`
- 4 passengers with different preferences
- 2 drivers (Alice with SUV, Bob with sedan)
- Demonstrates real-world ride-sharing scenario

**JSON Example**: `examples/data/pragmatic/basics/passenger-preferences.basic.problem.json`
- End-to-end test with vrp-cli
- Verified solver respects preferences

## Files Modified

### Core Implementation
- `vrp-core/src/construction/features/mod.rs` - Exports
- `vrp-core/src/construction/features/preferences.rs` - Feature implementation (new)
- `vrp-core/tests/unit/construction/features/preferences_test.rs` - Unit tests (new)
- `vrp-core/examples/passenger_preferences.rs` - Working example (new)

### Pragmatic Format Integration
- `vrp-pragmatic/src/format/problem/model.rs` - JSON model
- `vrp-pragmatic/src/format/problem/job_reader.rs` - Job parsing
- `vrp-pragmatic/src/format/problem/fleet_reader.rs` - Vehicle skills → attributes
- `vrp-pragmatic/src/format/problem/problem_reader.rs` - Feature detection
- `vrp-pragmatic/src/format/problem/goal_reader.rs` - Feature creation
- `vrp-pragmatic/src/checker/mod.rs` - Clippy allow for large enum

### CLI Support
- `vrp-cli/src/extensions/import/csv.rs` - CSV import support
- `vrp-cli/src/extensions/generate/fleet.rs` - Problem generation
- `vrp-cli/src/extensions/generate/plan.rs` - Plan generation

### Test Helpers
- `vrp-cli/tests/helpers/generate.rs`
- `vrp-pragmatic/tests/helpers/problem.rs`
- `vrp-pragmatic/tests/generator/jobs.rs`

### Examples
- `examples/data/pragmatic/basics/passenger-preferences.basic.problem.json` (new)

## Quality Assurance

✅ **All tests pass**: 1,155+ tests including 13 new preference tests
✅ **Clippy clean**: Passes with `-D warnings` (strict mode)
✅ **No build warnings**: Clean compilation
✅ **End-to-end verified**: JSON example works with vrp-cli
✅ **Documentation**: Inline code comments and examples

## Usage

### Programmatic API

```rust
use vrp_core::construction::features::{
    create_preferences_feature, PreferencePenalty,
    JobPreferences, JobPreferencesDimension,
    VehicleAttributesDimension
};

// Create feature with default penalties
let feature = create_preferences_feature(
    "preferences",
    PreferencePenalty::default()
)?;

// Add to job
job.dimens_mut().set_job_preferences(
    JobPreferences::new(
        Some(vec!["driver:alice".to_string()]),
        Some(vec!["driver:bob".to_string()]),
        Some(vec!["shift:night".to_string()])
    )
);

// Vehicle attributes set automatically from skills in pragmatic format
```

### JSON Format (vrp-cli)

```bash
vrp-cli solve pragmatic problem.json
```

Where `problem.json` includes:
```json
{
  "plan": {
    "jobs": [{
      "id": "passenger1",
      "preferences": {
        "preferred": ["driver:alice"],
        "avoid": ["shift:night"]
      }
    }]
  },
  "fleet": {
    "vehicles": [{
      "skills": ["driver:alice", "vehicle:suv", "shift:day"]
    }]
  }
}
```

## Future Enhancements

Potential improvements if needed:

1. **Route-level caching**: Add if profiling shows performance issues on large problems (1000+ jobs)
2. **Configurable penalties**: Allow penalty customization in JSON format
3. **Preference groups**: Support for preference groups (e.g., "any premium driver")
4. **Statistical reporting**: Track preference match rates in solution output

## References

- Core feature: `vrp-core/src/construction/features/preferences.rs`
- Example: `vrp-core/examples/passenger_preferences.rs`
- JSON example: `examples/data/pragmatic/basics/passenger-preferences.basic.problem.json`
- Tests: `vrp-core/tests/unit/construction/features/preferences_test.rs`
