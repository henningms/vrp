# Flexible Routes Tests

This directory contains tests for the flexible routes feature, which enables deviated fixed routes and fixed-ish route scenarios.

## Feature Overview

The flexible routes feature introduces two types of special stops that can be defined in vehicle shifts:

### 1. Required Stops (Hard Constraint)
**Field:** `requiredStops`

Required stops are mandatory waypoints that **must** be visited in the exact order specified. They are enforced through sequence locks in the VRP solver.

**Use cases:**
- Fixed bus stops that must be visited on schedule
- Mandatory checkpoints for regulatory compliance
- Required inspection points
- Fixed pickup/delivery points in a milk run

**Characteristics:**
- Must be visited in sequential order
- Cannot be skipped
- Enforced via locks (hard constraint)
- Each vehicle instance gets its own set of required stops

### 2. Via Stops (Soft Constraint)
**Field:** `via`

Via stops are preferred waypoints that **should** be visited in order when possible, but can be skipped or reordered if it improves the overall solution.

**Use cases:**
- Preferred waypoints for scenic routes
- Suggested stops for customer service
- Optional checkpoints for quality checks
- Flexible waypoints in deviated fixed routes

**Characteristics:**
- Optional - can be skipped if not cost-effective
- Preferred order but not enforced
- Low penalty for not visiting (0.1 weight)
- Soft constraint via tour order objective

## Test Files

### Unit Tests

- **`required_stops_test.rs`**: Tests for required stops functionality
  - Order enforcement
  - Time window compliance
  - Multiple vehicle support

- **`via_stops_test.rs`**: Tests for via stops functionality
  - Optional visits
  - Preferred ordering
  - Skipping when not optimal

- **`combined_stops_test.rs`**: Tests combining both features
  - Interaction between required and via stops
  - Priority verification (required > via)
  - Complex realistic scenarios

### JSON Test Data

- **`test_data_required_stops.json`**: Example with mandatory checkpoints
  - 3 delivery jobs
  - 2 required checkpoints with time windows

- **`test_data_via_stops.json`**: Example with optional waypoints
  - 3 delivery jobs
  - 3 via points (optional)

- **`test_data_combined.json`**: Example with both types
  - 4 customer deliveries
  - 2 mandatory checkpoints
  - 3 optional waypoints

- **`test_data_deviated_fixed_route.json`**: Realistic bus route scenario
  - 3 on-demand requests
  - 5 fixed bus stops (required)
  - 3 preferred stops (via)

## JSON Schema

### Required Stops
```json
{
  "shifts": [{
    "requiredStops": [
      {
        "location": { "lat": 52.5, "lng": 13.4 },
        "duration": 600,
        "tag": "checkpoint_1",
        "times": [["2024-01-01T10:00:00Z", "2024-01-01T12:00:00Z"]],
        "requestedTime": "2024-01-01T10:30:00Z"
      }
    ]
  }]
}
```

### Via Stops
```json
{
  "shifts": [{
    "via": [
      {
        "location": { "lat": 52.5, "lng": 13.4 },
        "duration": 300,
        "tag": "via_point_1",
        "times": [["2024-01-01T09:00:00Z", "2024-01-01T18:00:00Z"]],
        "requestedTime": "2024-01-01T12:00:00Z"
      }
    ]
  }]
}
```

## Running Tests

```bash
# Run all flexible route tests
cargo test --test '*' flexible_routes

# Run specific test
cargo test --test '*' can_enforce_required_stops_order

# Run with output
cargo test --test '*' flexible_routes -- --nocapture
```

## Implementation Details

### Required Stops
- Created as conditional jobs with `job_type = "required"`
- Sequence locks enforce ordering: `Lock::new(condition, vec![LockDetail::Sequence], false)`
- Each vehicle ID gets its own lock with all required stops in order
- Generated job IDs: `{vehicle_id}_required_{shift_index}_{place_index}`

### Via Stops
- Created as conditional jobs with `job_type = "via"`
- Each via stop gets a `ViaOrder` dimension (1, 2, 3, ...)
- Unassigned weight: 0.1 (light penalty if not visited)
- Tour order soft feature encourages sequential visits
- Generated job IDs: `{vehicle_id}_via_{shift_index}_{place_index}`

### Objective Function
When `has_via` is true, an additional soft objective is added:
```rust
create_tour_order_soft_feature("via_order", get_via_order_fn())
```

This uses the `ViaOrder` dimension to prefer visiting via stops in sequence.

## Expected Behavior

### Required Stops
✅ Always present in solution
✅ Always in specified order
✅ Time windows must be respected
❌ Cannot be skipped
❌ Cannot be reordered

### Via Stops
✅ Can be skipped if not optimal
✅ Preferred to visit in order
✅ Lower cost when visited in sequence
❌ Not guaranteed to be visited
❌ Order not strictly enforced

### Combined
✅ Required stops take precedence
✅ Via stops can be inserted between required stops
✅ Via stops cannot break required stop sequence
✅ All regular jobs are still served

## Debugging Tips

1. **Check job creation**: Look for jobs with `job_type = "required"` or `"via"`
2. **Verify locks**: Required stops create sequence locks per vehicle
3. **Check via order**: Via stops should have `ViaOrder` dimension set
4. **Objective weights**: Via unassigned weight = 0.1
5. **Problem properties**: `has_via` flag should be set when via stops exist

## Future Enhancements

Potential improvements:
- [ ] Allow specifying penalty weights for via stops
- [ ] Support for time-dependent via preferences
- [ ] Soft time windows for via stops
- [ ] Clustered via stops (visit N out of M)
- [ ] Via stop priorities (high/medium/low preference)
