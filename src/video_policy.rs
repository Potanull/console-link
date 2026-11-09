use crate::types::VideoMode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CapturePreferences {
    pub automatic: bool,
    pub preferred_fps: u32,
}

impl Default for CapturePreferences {
    fn default() -> Self {
        Self {
            automatic: true,
            preferred_fps: 60,
        }
    }
}

pub fn choose_mode(
    modes: &[VideoMode],
    preferences: &CapturePreferences,
    manual: Option<&VideoMode>,
) -> Option<VideoMode> {
    if !preferences.automatic {
        return manual
            .filter(|mode| valid_mode(mode) && modes.contains(mode))
            .cloned();
    }
    if !(1..=60).contains(&preferences.preferred_fps) {
        return None;
    }
    let target = f64::from(preferences.preferred_fps);
    modes
        .iter()
        .filter(|mode| valid_mode(mode))
        .min_by(|a, b| {
            let a_fps = rate(a);
            let b_fps = rate(b);
            // Rates such as 60000/1001 and 60 belong to the same target family.
            // Their native fraction remains unchanged in the returned mode.
            let family_distance = |fps: f64| {
                let distance = (fps - target).abs();
                if distance <= 0.1 + 1e-9 {
                    0.0
                } else {
                    distance
                }
            };
            let full_hd = |mode: &VideoMode| mode.width == 1920 && mode.height == 1080;
            (a_fps > target)
                .cmp(&(b_fps > target))
                .then_with(|| full_hd(b).cmp(&full_hd(a)))
                .then_with(|| family_distance(a_fps).total_cmp(&family_distance(b_fps)))
                .then_with(|| area(b).cmp(&area(a)))
                .then_with(|| b.width.cmp(&a.width))
                .then_with(|| b.height.cmp(&a.height))
                .then_with(|| (a_fps - target).abs().total_cmp(&(b_fps - target).abs()))
                .then_with(|| a.fps_den.cmp(&b.fps_den))
                .then_with(|| a.fps_num.cmp(&b.fps_num))
        })
        .cloned()
}

/// Rounded display category only; capture always uses the original rational rate.
pub fn nominal_fps(mode: &VideoMode) -> u32 {
    if valid_mode(mode) {
        rate(mode).round() as u32
    } else {
        0
    }
}

fn valid_mode(mode: &VideoMode) -> bool {
    mode.width > 0
        && mode.width <= 1920
        && mode.height > 0
        && mode.height <= 1080
        && mode.fps_den > 0
        && mode.fps_num >= mode.fps_den
        && u64::from(mode.fps_num) <= 60 * u64::from(mode.fps_den)
}

fn rate(mode: &VideoMode) -> f64 {
    f64::from(mode.fps_num) / f64::from(mode.fps_den)
}
fn area(mode: &VideoMode) -> u64 {
    u64::from(mode.width) * u64::from(mode.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(width: u32, height: u32, fps_num: u32, fps_den: u32) -> VideoMode {
        VideoMode {
            width,
            height,
            fps_num,
            fps_den,
        }
    }

    #[test]
    fn defaults_enable_automatic_sixty_fps_for_existing_settings() {
        let preferences: CapturePreferences = serde_json::from_str("{}").unwrap();
        assert!(preferences.automatic);
        assert_eq!(preferences.preferred_fps, 60);
    }

    #[test]
    fn automatic_ignores_previously_cached_thirty_fps_mode() {
        let old = mode(1920, 1080, 30, 1);
        let best = mode(1920, 1080, 60, 1);
        assert_eq!(
            choose_mode(
                &[old.clone(), best.clone()],
                &CapturePreferences::default(),
                Some(&old)
            ),
            Some(best)
        );
    }

    #[test]
    fn defaults_to_full_hd_even_when_lower_resolution_offers_sixty() {
        let full_hd = mode(1920, 1080, 30, 1);
        assert_eq!(
            choose_mode(
                &[full_hd.clone(), mode(1280, 720, 60, 1)],
                &CapturePreferences::default(),
                None
            ),
            Some(full_hd)
        );
    }

    #[test]
    fn ntsc_sixty_keeps_rational_rate_and_prefers_larger_resolution() {
        let ntsc = mode(1920, 1080, 60000, 1001);
        assert_eq!(nominal_fps(&ntsc), 60);
        assert_eq!(
            choose_mode(
                &[mode(1280, 720, 60, 1), ntsc.clone()],
                &CapturePreferences::default(),
                None
            ),
            Some(ntsc)
        );
    }

    #[test]
    fn exact_sixty_wins_only_after_resolution_matches() {
        let exact = mode(1920, 1080, 60, 1);
        assert_eq!(
            choose_mode(
                &[mode(1920, 1080, 60000, 1001), exact.clone()],
                &CapturePreferences::default(),
                None
            ),
            Some(exact)
        );
    }

    #[test]
    fn unsupported_target_falls_back_to_fastest_below_without_inventing_sixty() {
        let available = mode(1920, 1080, 50, 1);
        let chosen = choose_mode(
            &[mode(1920, 1080, 30, 1), available.clone()],
            &CapturePreferences::default(),
            None,
        );
        assert_eq!(chosen, Some(available));
        assert_eq!(nominal_fps(&chosen.unwrap()), 50);
    }

    #[test]
    fn configured_thirty_prefers_actual_thirty_and_never_exceeds_target_if_lower_exists() {
        let preferences = CapturePreferences {
            automatic: true,
            preferred_fps: 30,
        };
        let thirty = mode(1920, 1080, 30, 1);
        assert_eq!(
            choose_mode(
                &[mode(1920, 1080, 60, 1), thirty.clone()],
                &preferences,
                None
            ),
            Some(thirty)
        );
        let lower_resolution = mode(1280, 720, 30, 1);
        assert_eq!(
            choose_mode(
                &[mode(1920, 1080, 60, 1), lower_resolution.clone()],
                &preferences,
                None
            ),
            Some(lower_resolution)
        );
        let twenty_five = mode(1920, 1080, 25, 1);
        assert_eq!(
            choose_mode(
                &[mode(1920, 1080, 50, 1), twenty_five.clone()],
                &preferences,
                None
            ),
            Some(twenty_five)
        );
    }

    #[test]
    fn manual_mode_must_be_explicit_valid_and_advertised() {
        let preferences = CapturePreferences {
            automatic: false,
            preferred_fps: 60,
        };
        let selected = mode(1280, 720, 30, 1);
        let modes = [mode(1920, 1080, 60, 1), selected.clone()];
        assert_eq!(
            choose_mode(&modes, &preferences, Some(&selected)),
            Some(selected)
        );
        assert_eq!(choose_mode(&modes, &preferences, None), None);
        assert_eq!(
            choose_mode(&modes, &preferences, Some(&mode(640, 480, 30, 1))),
            None
        );
    }

    #[test]
    fn invalid_or_unsupported_modes_never_win() {
        let valid = mode(640, 480, 30, 1);
        let invalid = [
            mode(0, 1080, 60, 1),
            mode(1920, 1080, 60, 0),
            mode(1920, 1080, 0, 1),
            mode(3840, 2160, 60, 1),
            mode(1920, 1080, 120, 1),
        ];
        let mut modes = invalid.to_vec();
        modes.push(valid.clone());
        assert_eq!(
            choose_mode(&modes, &CapturePreferences::default(), None),
            Some(valid)
        );
        assert_eq!(
            choose_mode(&invalid, &CapturePreferences::default(), None),
            None
        );
        assert_eq!(choose_mode(&[], &CapturePreferences::default(), None), None);
    }

    #[test]
    fn selection_is_independent_of_driver_enumeration_order() {
        let mut modes = vec![
            mode(1920, 1080, 60000, 1001),
            mode(1920, 1080, 60000, 1000),
            mode(1280, 720, 60, 1),
            mode(1920, 1080, 60, 1),
        ];
        let expected = Some(mode(1920, 1080, 60, 1));
        for _ in 0..modes.len() {
            assert_eq!(
                choose_mode(&modes, &CapturePreferences::default(), None),
                expected
            );
            modes.rotate_left(1);
            modes.reverse();
        }
    }
}
