use console_link::settings::GameWindowMode;

#[derive(Default)]
pub struct PlaybackState {
    pub playing: bool,
    pub overlay_open: bool,
    pub mode: GameWindowMode,
    return_mode: GameWindowMode,
}

impl PlaybackState {
    pub fn begin(&mut self, mode: GameWindowMode) {
        self.playing = true;
        self.overlay_open = false;
        self.mode = mode;
        self.return_mode = if mode == GameWindowMode::Fullscreen {
            GameWindowMode::Windowed
        } else {
            mode
        };
    }
    pub fn finish(&mut self) {
        self.playing = false;
        self.overlay_open = false;
    }
    pub fn toggle_overlay(&mut self) {
        self.overlay_open = !self.overlay_open;
    }
    pub fn set_mode(&mut self, mode: GameWindowMode) {
        if mode == GameWindowMode::Fullscreen && self.mode != mode {
            self.return_mode = self.mode;
        }
        self.mode = mode;
    }
    pub fn toggle_fullscreen(&mut self) {
        if self.mode == GameWindowMode::Fullscreen {
            self.mode = self.return_mode;
        } else {
            self.return_mode = self.mode;
            self.mode = GameWindowMode::Fullscreen;
        }
    }
    pub fn cycle_mode(&mut self) {
        self.set_mode(match self.mode {
            GameWindowMode::Windowed => GameWindowMode::Borderless,
            GameWindowMode::Borderless => GameWindowMode::Fullscreen,
            GameWindowMode::Fullscreen => GameWindowMode::Windowed,
        });
    }
    /// Returns whether the native window mode changed.
    pub fn escape(&mut self) -> bool {
        if self.overlay_open {
            self.overlay_open = false;
            false
        } else if self.mode == GameWindowMode::Fullscreen {
            self.mode = self.return_mode;
            true
        } else {
            self.overlay_open = true;
            false
        }
    }
}

/// Transient UI feedback uses the UI clock, independent of capture FPS.
#[derive(Default)]
pub struct GameFeedback {
    cursor_active: bool,
    last_motion: f64,
    volume_changed: Option<f64>,
}

impl GameFeedback {
    pub fn hide_cursor(
        &mut self,
        now: f64,
        playing: bool,
        controls_open: bool,
        focused: bool,
        moved: bool,
    ) -> bool {
        let active = playing && !controls_open && focused;
        if !active || !self.cursor_active || moved {
            self.last_motion = now;
        }
        self.cursor_active = active;
        active && now - self.last_motion >= 4.0
    }

    pub fn volume_changed(&mut self, now: f64) {
        self.volume_changed = Some(now);
    }

    pub fn volume_visible(&self, now: f64) -> bool {
        self.volume_changed
            .is_some_and(|changed| now - changed < 2.0)
    }
}

#[derive(Default)]
pub struct FramePacer {
    last_presented_ns: Option<u64>,
    // Deadlines are nanoseconds multiplied by FPS, preserving fractional periods.
    next_deadline: Option<u128>,
    fps: u32,
}
impl FramePacer {
    fn deadline_for(&self, fps: u32) -> Option<u128> {
        if self.fps == fps {
            self.next_deadline
        } else {
            self.last_presented_ns
                .map(|last| u128::from(last) * u128::from(fps) + 1_000_000_000)
        }
    }
    pub fn ready(&mut self, now_ns: u64, fps: u32) -> bool {
        let fps = fps.clamp(1, 60);
        self.next_deadline = self.deadline_for(fps);
        self.fps = fps;
        // Allow ordinary sub-millisecond vsync jitter without moving the phase.
        self.next_deadline
            .is_none_or(|deadline| (u128::from(now_ns) + 500_000) * u128::from(fps) >= deadline)
    }
    pub fn presented(&mut self, now_ns: u64) {
        if self.fps == 0 {
            self.fps = 60;
        }
        let now = u128::from(now_ns) * u128::from(self.fps);
        self.next_deadline = Some(match self.next_deadline {
            // Skip missed slots after a stall; never accumulate catch-up frames.
            Some(deadline) => {
                deadline + (now.saturating_sub(deadline) / 1_000_000_000 + 1) * 1_000_000_000
            }
            None => now + 1_000_000_000,
        });
        self.last_presented_ns = Some(now_ns);
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn repaint_after(&self, now_ns: u64, fps: u32) -> std::time::Duration {
        let fps = fps.clamp(1, 60);
        let remaining = self.deadline_for(fps).map_or(1_000_000_000, |deadline| {
            deadline.saturating_sub(u128::from(now_ns) * u128::from(fps))
        });
        // An overdue deadline with no new source frame must not busy-poll.
        // Capture callbacks can still wake the UI before this fallback interval.
        let remaining = if remaining == 0 {
            1_000_000_000
        } else {
            remaining
        };
        std::time::Duration::from_nanos(remaining.div_ceil(u128::from(fps)) as u64)
    }
}

pub fn game_window_title(game: &str) -> String {
    if game.is_empty() {
        "Console Link - Game".into()
    } else {
        format!("{game} - Console Link")
    }
}

pub fn mode_label(mode: GameWindowMode) -> &'static str {
    match mode {
        GameWindowMode::Windowed => "Оконный",
        GameWindowMode::Borderless => "Безрамочный полный экран",
        GameWindowMode::Fullscreen => "Полный экран",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_title_has_one_fallback_for_manual_and_nintendo_sources() {
        assert_eq!(game_window_title(""), "Console Link - Game");
        assert_eq!(game_window_title("Test Game"), "Test Game - Console Link");
    }

    #[test]
    fn cursor_hides_after_four_seconds_and_returns_for_motion_controls_or_focus() {
        let mut feedback = GameFeedback::default();
        assert!(!feedback.hide_cursor(10.0, true, false, true, false));
        assert!(!feedback.hide_cursor(13.99, true, false, true, false));
        assert!(feedback.hide_cursor(14.0, true, false, true, false));
        assert!(!feedback.hide_cursor(14.1, true, false, true, true));
        assert!(feedback.hide_cursor(18.1, true, false, true, false));
        assert!(!feedback.hide_cursor(19.0, true, true, true, false));
        assert!(!feedback.hide_cursor(20.0, true, false, true, false));
        assert!(!feedback.hide_cursor(25.0, true, false, false, false));
        assert!(!feedback.hide_cursor(26.0, true, false, true, false));
        assert!(!feedback.hide_cursor(40.0, false, false, true, false));
        assert!(!feedback.hide_cursor(50.0, true, false, true, false));
    }

    #[test]
    fn volume_feedback_expires_and_each_change_extends_it() {
        let mut feedback = GameFeedback::default();
        assert!(!feedback.volume_visible(0.0));
        feedback.volume_changed(10.0);
        assert!(feedback.volume_visible(11.99));
        feedback.volume_changed(11.9);
        assert!(feedback.volume_visible(13.8));
        assert!(!feedback.volume_visible(13.91));
    }

    fn count_presentations(source_hz: u64, target_fps: u32, seconds: u64, jitter: bool) -> u64 {
        let mut pacer = FramePacer::default();
        let mut shown = 0;
        for n in 0..source_hz * seconds {
            let now =
                n * 1_000_000_000 / source_hz + if jitter && n % 2 == 1 { 200_000 } else { 0 };
            if pacer.ready(now, target_fps) {
                pacer.presented(now);
                shown += 1;
            }
        }
        shown
    }

    #[test]
    fn steady_and_jittered_sixty_hz_preserve_every_frame() {
        assert_eq!(count_presentations(60, 60, 2, false), 120);
        assert_eq!(count_presentations(60, 60, 2, true), 120);
    }

    #[test]
    fn fast_sources_remain_capped_without_losing_target_rate() {
        for source in [60, 120, 144] {
            for target in [30, 60] {
                let shown = count_presentations(source, target, 10, true);
                let expected = u64::from(target) * 10;
                assert!(
                    shown.abs_diff(expected) <= 1,
                    "{source} Hz -> {target} FPS: {shown}"
                );
            }
        }
    }
    #[test]
    fn menu_is_only_hidden_during_a_session_and_overlay_does_not_end_it() {
        let mut state = PlaybackState::default();
        state.begin(GameWindowMode::Windowed);
        assert!(state.playing);
        state.toggle_overlay();
        assert!(state.overlay_open && state.playing);
        state.finish();
        assert!(!state.playing && !state.overlay_open);
    }
    #[test]
    fn escape_closes_overlay_before_leaving_fullscreen() {
        let mut state = PlaybackState::default();
        state.begin(GameWindowMode::Borderless);
        state.toggle_fullscreen();
        state.toggle_overlay();
        assert!(!state.escape());
        assert_eq!(state.mode, GameWindowMode::Fullscreen);
        assert!(state.escape());
        assert_eq!(state.mode, GameWindowMode::Borderless);
    }
    #[test]
    fn pacer_caps_presentations_without_fabricating_frames() {
        let mut pacer = FramePacer::default();
        assert!(pacer.ready(0, 60));
        pacer.presented(0);
        assert!(!pacer.ready(8_000_000, 60));
        assert!(pacer.ready(16_666_667, 60));
        assert!(!pacer.ready(20_000_000, 30));
        assert!(pacer.ready(33_333_334, 30));
        pacer.reset();
        assert!(pacer.ready(0, 60));
    }
    #[test]
    fn capture_wakeups_do_not_reset_the_presentation_deadline() {
        let mut pacer = FramePacer::default();
        assert!(pacer.ready(0, 30));
        pacer.presented(0);
        for now in (1_000_000..33_000_000).step_by(1_000_000) {
            assert!(!pacer.ready(now, 30));
            assert_eq!(
                pacer.repaint_after(now, 30).as_nanos(),
                u128::from(33_333_334_u64 - now)
            );
        }
        assert_eq!(pacer.repaint_after(32_000_000, 30).as_nanos(), 1_333_334);
        assert!(!pacer.ready(32_000_000, 30));
        assert!(pacer.ready(33_333_334, 30));
    }
    #[test]
    fn late_frame_skips_missed_slots_without_a_catch_up_burst() {
        let mut pacer = FramePacer::default();
        assert!(pacer.ready(0, 60));
        pacer.presented(0);
        assert!(pacer.ready(1_000_000_000, 60));
        pacer.presented(1_000_000_000);
        assert!(!pacer.ready(1_001_000_000, 60));
        assert!(pacer.ready(1_016_666_666, 60));
    }
    #[test]
    fn missing_source_frames_do_not_create_an_overdue_repaint_loop() {
        for fps in [30, 60] {
            let mut pacer = FramePacer::default();
            assert!(pacer.ready(0, fps));
            pacer.presented(0);
            for now in [50_000_000, 100_000_000, 1_000_000_000] {
                assert!(pacer.ready(now, fps));
                let wait = pacer.repaint_after(now, fps);
                assert!(wait >= std::time::Duration::from_millis(16));
                assert!(wait <= std::time::Duration::from_millis(34));
            }
        }
    }
    #[test]
    fn every_display_mode_can_be_reached_and_stop_resets_overlay() {
        let mut state = PlaybackState::default();
        state.begin(GameWindowMode::Windowed);
        state.cycle_mode();
        assert_eq!(state.mode, GameWindowMode::Borderless);
        state.cycle_mode();
        assert_eq!(state.mode, GameWindowMode::Fullscreen);
        state.cycle_mode();
        assert_eq!(state.mode, GameWindowMode::Windowed);
        assert!(!state.escape());
        assert!(state.overlay_open);
        state.finish();
        assert!(!state.overlay_open);
    }
}
