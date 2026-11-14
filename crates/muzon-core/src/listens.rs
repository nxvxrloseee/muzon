//! Same listen threshold as the original frontend, evaluated by the worker.
pub fn should_count(position: f64, duration: f64) -> bool {
    position.is_finite()
        && duration.is_finite()
        && duration > 30.0
        && position >= (duration / 2.0).min(240.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thresholds_and_invalid_values() {
        assert!(!should_count(29., 30.));
        assert!(!should_count(19., 40.));
        assert!(should_count(20., 40.));
        assert!(should_count(240., 3600.));
        assert!(!should_count(f64::NAN, 300.));
    }
}
