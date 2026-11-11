//! Clock-independent presentation progress for a short, bounded reveal.
use std::time::Duration;

/// Fraction of initial displacement remaining. A ready frame finishes after
/// `minimum`; absent readiness, the last portion completes by `deadline`.
pub fn reveal_remaining(
    elapsed: Duration,
    ready: bool,
    minimum: Duration,
    deadline: Duration,
) -> f32 {
    if minimum.is_zero()
        || deadline <= minimum
        || elapsed >= deadline
        || (ready && elapsed >= minimum)
    {
        return 0.0;
    }
    let t = (elapsed.as_secs_f32() / minimum.as_secs_f32()).min(1.0);
    let eased = 1.0 - (1.0 - t).powi(3);
    if elapsed < minimum {
        1.0 - 0.85 * eased
    } else {
        0.15 * (1.0 - (elapsed - minimum).as_secs_f32() / (deadline - minimum).as_secs_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_and_timeout_are_bounded_and_monotonic() {
        let min = Duration::from_millis(220);
        let cap = Duration::from_millis(650);
        assert_eq!(reveal_remaining(Duration::ZERO, false, min, cap), 1.0);
        assert!(reveal_remaining(min, false, min, cap) > 0.0);
        assert_eq!(reveal_remaining(min, true, min, cap), 0.0);
        assert_eq!(reveal_remaining(cap, false, min, cap), 0.0);
        let values: Vec<_> = (0..=650)
            .map(|ms| reveal_remaining(Duration::from_millis(ms), false, min, cap))
            .collect();
        assert!(values.windows(2).all(|w| w[0] >= w[1]));
    }
}

/// Time-reversed cubic reveal: displacement grows gently when dismissing.
pub fn dismiss_progress(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / duration.as_secs_f32())
        .clamp(0.0, 1.0)
        .powi(3)
}
