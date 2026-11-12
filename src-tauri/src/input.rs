const DEAD_ZONE: f32 = 0.12;

pub fn axis(value: f32) -> f32 {
    if !value.is_finite() || value.abs() <= DEAD_ZONE {
        0.
    } else {
        ((value.abs() - DEAD_ZONE) / (1. - DEAD_ZONE))
            .min(1.)
            .copysign(value)
    }
}
pub fn armed(previous: bool, x: f32, y: f32, clicked: bool) -> bool {
    !clicked && (previous || (axis(x) == 0. && axis(y) == 0.))
}
pub fn changed(previous: Option<f32>, current: f32) -> bool {
    previous.is_none_or(|p| (current - p).abs() >= 0.02 || (current == 0. && p != 0.))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn click_blocks_axes_until_neutral_after_release() {
        assert!(!armed(true, 0.2, 0., true));
        assert!(!armed(false, 0.2, 0., false));
        assert!(armed(false, 0.02, -0.02, false));
    }
    #[test]
    fn deadzone_is_continuous_and_slow_movements_accumulate() {
        assert_eq!(axis(0.1), 0.);
        assert!(axis(0.121) < 0.002);
        assert_eq!(axis(-1.1), -1.);
        assert_eq!(axis(f32::NAN), 0.);
        assert!(!changed(Some(0.1), 0.11));
        assert!(changed(Some(0.1), 0.125));
        assert!(changed(Some(0.01), 0.));
    }
}
