# Generic Preference System Design V2 (Soft Constraint Semantics)

## Problem with V1 (Skills-like semantics)

**The issue:** Using `allOf`, `oneOf`, `noneOf` for preferences is confusing because:
- These terms imply **guarantees** ("all of these MUST be present")
- But soft constraints only **nudge** the solution, they don't guarantee anything
- A passenger with `"oneOf": ["driver:alice"]` might still get driver Bob if it's more efficient

**Better approach:** Use scoring/weighting semantics that clearly indicate preference strength.

---

## Design Options

### Option 1: Simple Weighted Preferences

Most intuitive for users:

```json
{
  "id": "passenger1",
  "preferences": {
    "driver:alice": 100,      // Strong preference (high penalty if not matched)
    "driver:bob": 50,         // Moderate preference
    "vehicle:suv": 30,        // Weak preference
    "shift:morning": -20      // Negative = avoid this attribute
  }
}
```

**Semantics:**
- **Positive value:** Prefer this attribute (penalty if missing)
- **Negative value:** Avoid this attribute (penalty if present)
- **Magnitude:** How strongly the preference matters

**Cost calculation:**
```rust
// For each job preference:
//   If attribute is preferred (positive) and vehicle HAS it: +0 (good!)
//   If attribute is preferred (positive) and vehicle LACKS it: +penalty
//   If attribute is avoided (negative) and vehicle HAS it: +|penalty|
//   If attribute is avoided (negative) and vehicle LACKS it: +0 (good!)

penalty = preferences.iter()
    .map(|(attr, weight)| {
        let has_attr = vehicle_attrs.contains(attr);
        match (weight > 0, has_attr) {
            (true, true) => 0.0,          // Preferred and present = perfect
            (true, false) => weight,      // Preferred but missing = penalty
            (false, true) => weight.abs(),// Avoided but present = penalty
            (false, false) => 0.0,        // Avoided and absent = perfect
        }
    })
    .sum()
```

**Pros:**
- Very intuitive ("I prefer Alice with weight 100")
- Flexible (can have many preferences with different strengths)
- Clear semantics (number = how much it matters)

**Cons:**
- Doesn't express "I want Alice OR Bob" (list of acceptable options)

---

### Option 2: Tiered Preferences

Group preferences into tiers:

```json
{
  "id": "passenger1",
  "preferences": {
    "preferred": ["driver:alice", "driver:bob"],      // Penalty if NONE match
    "acceptable": ["driver:charlie"],                 // Small penalty if none match
    "avoid": ["vehicle:old_van", "shift:night"]       // Penalty if ANY match
  }
}
```

**Semantics:**
- **preferred:** Best options (large penalty if none present)
- **acceptable:** OK options (small penalty if none present)
- **avoid:** Don't want these (penalty for each present)

**Cost calculation:**
```rust
let mut penalty = 0.0;

// Check if any preferred attribute is present
if let Some(preferred) = &prefs.preferred {
    let has_preferred = preferred.iter().any(|attr| vehicle_attrs.contains(attr));
    if !has_preferred {
        penalty += 100.0;  // Fixed penalty for no preferred match
    }
}

// Check if any acceptable attribute is present
if let Some(acceptable) = &prefs.acceptable {
    let has_acceptable = acceptable.iter().any(|attr| vehicle_attrs.contains(attr));
    if !has_acceptable && !has_preferred {
        penalty += 50.0;  // Only penalize if no preferred AND no acceptable
    }
}

// Penalize each avoided attribute that's present
if let Some(avoid) = &prefs.avoid {
    let avoid_count = avoid.iter().filter(|attr| vehicle_attrs.contains(attr)).count();
    penalty += (avoid_count as f64) * 75.0;
}
```

**Pros:**
- Natural grouping (preferred vs avoid)
- Expresses "any of these is fine" (list of alternatives)
- Simpler for users than individual weights

**Cons:**
- Less granular (can't say "Alice is better than Bob")
- Fixed penalty structure

---

### Option 3: Hybrid (Weighted Tiers)

Combine both approaches:

```json
{
  "id": "passenger1",
  "preferences": [
    {
      "attributes": ["driver:alice", "driver:bob"],
      "weight": 100,
      "type": "preferred"   // Penalty if NONE match
    },
    {
      "attributes": ["vehicle:old_van"],
      "weight": 50,
      "type": "avoid"       // Penalty if ANY match
    }
  ]
}
```

**Semantics:**
- Each preference group has a weight (how much it matters)
- **type: "preferred"** = penalty if NONE of the attributes match
- **type: "avoid"** = penalty if ANY of the attributes match
- **type: "required"** = penalty per missing attribute (like allOf but soft)

**Pros:**
- Maximum flexibility
- Can express complex preferences
- Clear semantics

**Cons:**
- More complex JSON structure
- Might be overkill for simple use cases

---

## Recommended: Option 2 (Tiered Preferences)

**Rationale:**
- Matches how people naturally think about preferences
- "I prefer Alice or Bob, but definitely not the night shift"
- Simple enough for most use cases
- Can always add weights later if needed

---

## Recommended Design

### Data Structure (Core)

```rust
// vrp-core/src/construction/features/preferences.rs

/// Job preferences for vehicle attributes (soft constraint).
pub struct JobPreferences {
    /// List of preferred attributes. Penalty applied if NONE are present.
    /// Example: ["driver:alice", "driver:bob"] means "prefer Alice or Bob"
    pub preferred: Option<HashSet<String>>,

    /// List of acceptable attributes. Smaller penalty if none present and no preferred match.
    /// Used as fallback: "these are OK if preferred isn't available"
    pub acceptable: Option<HashSet<String>>,

    /// List of attributes to avoid. Penalty applied for EACH attribute present.
    /// Example: ["shift:night", "vehicle:old_van"] means "don't want night shift or old van"
    pub avoid: Option<HashSet<String>>,
}

impl JobPreferences {
    pub fn new(
        preferred: Option<Vec<String>>,
        acceptable: Option<Vec<String>>,
        avoid: Option<Vec<String>>
    ) -> Self {
        let map: fn(Option<Vec<_>>) -> Option<HashSet<_>> = |attrs| {
            attrs.and_then(|v| if v.is_empty() { None } else { Some(v.into_iter().collect()) })
        };

        Self {
            preferred: map(preferred),
            acceptable: map(acceptable),
            avoid: map(avoid),
        }
    }

    /// Check if any preferred attribute matches
    pub fn has_preferred_match(&self, vehicle_attrs: Option<&HashSet<String>>) -> bool {
        match (&self.preferred, vehicle_attrs) {
            (Some(preferred), Some(attrs)) => preferred.iter().any(|attr| attrs.contains(attr)),
            _ => false,
        }
    }

    /// Check if any acceptable attribute matches
    pub fn has_acceptable_match(&self, vehicle_attrs: Option<&HashSet<String>>) -> bool {
        match (&self.acceptable, vehicle_attrs) {
            (Some(acceptable), Some(attrs)) => acceptable.iter().any(|attr| attrs.contains(attr)),
            _ => false,
        }
    }

    /// Count how many avoided attributes are present
    pub fn count_avoided(&self, vehicle_attrs: Option<&HashSet<String>>) -> usize {
        match (&self.avoid, vehicle_attrs) {
            (Some(avoid), Some(attrs)) => avoid.iter().filter(|attr| attrs.contains(attr)).count(),
            _ => 0,
        }
    }
}

custom_dimension!(pub JobPreferences typeof JobPreferences);
custom_dimension!(pub VehicleAttributes typeof HashSet<String>);
```

### Penalty Configuration

```rust
/// Configurable penalty structure for preferences
pub struct PreferencePenalty {
    /// Penalty if none of the preferred attributes match
    pub no_preferred_match: Cost,

    /// Penalty if no preferred AND no acceptable attributes match
    pub no_acceptable_match: Cost,

    /// Penalty per avoided attribute that is present
    pub per_avoided_present: Cost,
}

impl Default for PreferencePenalty {
    fn default() -> Self {
        Self {
            no_preferred_match: 100.0,    // High penalty
            no_acceptable_match: 30.0,    // Lower penalty (fallback)
            per_avoided_present: 75.0,    // High penalty per unwanted attribute
        }
    }
}
```

### Cost Calculation

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

    // Check preferred attributes
    let has_preferred = preferences.has_preferred_match(vehicle_attrs);
    let has_acceptable = preferences.has_acceptable_match(vehicle_attrs);

    if preferences.preferred.is_some() && !has_preferred {
        // None of the preferred attributes match
        total_penalty += penalty_config.no_preferred_match;

        // If also no acceptable match, add additional penalty
        if preferences.acceptable.is_some() && !has_acceptable {
            total_penalty += penalty_config.no_acceptable_match;
        }
    }

    // Check avoided attributes (penalize each one present)
    let avoided_count = preferences.count_avoided(vehicle_attrs);
    total_penalty += (avoided_count as Cost) * penalty_config.per_avoided_present;

    total_penalty
}
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
          "preferred": ["driver:alice", "driver:bob"],
          "avoid": ["shift:night"]
        }
      },
      {
        "id": "passenger2",
        "deliveries": [...],
        "preferences": {
          "preferred": ["driver:alice"],
          "acceptable": ["driver:charlie"],
          "avoid": ["vehicle:old_van", "vehicle:motorcycle"]
        }
      }
    ]
  },
  "fleet": {
    "vehicles": [
      {
        "typeId": "alice_morning",
        "vehicleIds": ["alice_vehicle_1"],
        "attributes": ["driver:alice", "shift:morning", "vehicle:suv"]
      },
      {
        "typeId": "bob_night",
        "vehicleIds": ["bob_vehicle_1"],
        "attributes": ["driver:bob", "shift:night", "vehicle:sedan"]
      }
    ]
  }
}
```

### Example Scenarios

#### Scenario 1: Clear Preferred Match
```
Job preferences: {"preferred": ["driver:alice"]}
Vehicle attributes: ["driver:alice", "vehicle:suv"]
Penalty: 0.0 ✓ (Alice matches!)
```

#### Scenario 2: No Preferred, Has Acceptable
```
Job preferences: {
  "preferred": ["driver:alice"],
  "acceptable": ["driver:bob"]
}
Vehicle attributes: ["driver:bob"]
Penalty: 100.0 (no preferred match)
Note: No additional penalty because Bob is acceptable
```

#### Scenario 3: Neither Preferred Nor Acceptable
```
Job preferences: {
  "preferred": ["driver:alice"],
  "acceptable": ["driver:bob"]
}
Vehicle attributes: ["driver:charlie"]
Penalty: 100.0 (no preferred) + 30.0 (no acceptable) = 130.0
```

#### Scenario 4: Avoided Attribute Present
```
Job preferences: {
  "preferred": ["driver:alice"],
  "avoid": ["shift:night", "vehicle:old_van"]
}
Vehicle attributes: ["driver:alice", "shift:night"]
Penalty: 0.0 (alice matches) + 75.0 (night shift present) = 75.0
```

#### Scenario 5: Multiple Violations
```
Job preferences: {
  "preferred": ["driver:alice"],
  "avoid": ["shift:night"]
}
Vehicle attributes: ["driver:charlie", "shift:night"]
Penalty: 100.0 (no preferred) + 75.0 (night present) = 175.0
```

---

## Alternative: Simple List-Based (Option 1 Simplified)

If the tiered approach seems too complex, here's a simpler alternative:

```json
{
  "id": "passenger1",
  "preferences": {
    "match": ["driver:alice", "driver:bob"],     // Prefer if ANY match
    "avoid": ["shift:night"]                      // Avoid if ANY match
  }
}
```

**Cost:**
- If none of "match" attributes present: +100.0
- For each "avoid" attribute present: +50.0

Even simpler, but less expressive (can't distinguish between "preferred" and "acceptable").

---

## Comparison Table

| Approach | JSON Complexity | Expressiveness | User Intuition | Recommended |
|----------|----------------|----------------|----------------|-------------|
| Weighted (Option 1) | Medium | High | Medium | For power users |
| Tiered (Option 2) | Low | Medium-High | **High** | **YES** ✓ |
| Hybrid (Option 3) | High | Very High | Low | For complex cases |
| Simple match/avoid | Very Low | Low | High | For simple cases |

---

## Final Recommendation: Tiered with Optional Weights

Start with tiered (preferred/acceptable/avoid) but allow optional weights:

```json
{
  "preferences": {
    "preferred": ["driver:alice", "driver:bob"],
    "avoid": ["shift:night"],
    "weights": {
      "preferredPenalty": 100.0,    // Optional: override default
      "acceptablePenalty": 30.0,
      "avoidPenalty": 75.0
    }
  }
}
```

**Benefits:**
- Simple for basic use (just list preferred/avoid)
- Flexible for advanced use (customize penalties)
- Clear semantics ("preferred" vs "required")
- Natural to explain to users

---

## Implementation Notes

### State Management

```rust
custom_solution_state!(PreferencesFitness typeof Cost);
custom_tour_state!(RoutePreferencesPenalty typeof Cost);

struct PreferencesState {
    penalty: PreferencePenalty,
}

impl FeatureState for PreferencesState {
    fn accept_route_state(&self, route_ctx: &mut RouteContext) {
        let penalty = calculate_route_penalty(&self.penalty, route_ctx);
        route_ctx.state_mut().set_route_preferences_penalty(penalty);
    }

    fn accept_solution_state(&self, solution_ctx: &mut SolutionContext) {
        let total: Cost = solution_ctx.routes.iter()
            .filter_map(|r| r.state().get_route_preferences_penalty())
            .sum();
        solution_ctx.state.set_preferences_fitness(total);
    }
}
```

---

## Next Steps

Would you like me to:
1. **Implement the tiered preference system** (preferred/acceptable/avoid)?
2. **Implement the simple weighted system** (simpler but less expressive)?
3. **Create a comparison example** showing both approaches side-by-side?
4. **Discuss the semantics further** to nail down the exact behavior?

The key insight is: **Soft constraints should use preference/scoring language, not requirement language.**
