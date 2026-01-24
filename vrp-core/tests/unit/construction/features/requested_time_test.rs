use super::*;

#[test]
fn can_calculate_early_penalty() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    // Arriving 30 minutes (1800 seconds) early
    let arrival = 1000.0;
    let requested = 2800.0; // 1800 seconds later

    // Expected: 1800 seconds * (1.0 / 60) = 30 penalty
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 30.0).abs() < 0.001, "Expected 30.0, got {}", result);
}

#[test]
fn can_calculate_late_penalty() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    // Arriving 30 minutes (1800 seconds) late
    let arrival = 2800.0;
    let requested = 1000.0; // 1800 seconds earlier

    // Expected: 1800 seconds * (2.0 / 60) = 60 penalty
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 60.0).abs() < 0.001, "Expected 60.0, got {}", result);
}

#[test]
fn can_calculate_zero_penalty_for_on_time() {
    let penalty = RequestedTimePenalty::new(1.0, 2.0);

    let arrival = 1000.0;
    let requested = 1000.0;

    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 0.0).abs() < 0.001, "Expected 0.0, got {}", result);
}

#[test]
fn can_use_default_penalty() {
    let penalty = RequestedTimePenalty::default();

    // Arriving 60 minutes (3600 seconds) late
    let arrival = 4600.0;
    let requested = 1000.0;

    // Expected: 3600 seconds * (1.0 / 60) = 60 penalty (default 1.0 per minute)
    let result = penalty.calculate_penalty(arrival, requested);
    assert!((result - 60.0).abs() < 0.001, "Expected 60.0, got {}", result);
}
