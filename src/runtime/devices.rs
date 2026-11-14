use super::*;

impl Runtime {
    pub fn ready(&self) -> bool {
        let s = &self.settings;
        !self.opening
            && !self.refreshing
            && self
                .devices
                .video
                .iter()
                .any(|d| d.id == s.video_id && s.mode.as_ref().is_some_and(|m| d.modes.contains(m)))
            && ((!s.audio_enabled || (s.audio_input_id.is_empty() && s.audio_output_id.is_empty()))
                || (!self.refreshing_audio
                    && self.devices.inputs.iter().any(|d| d.id == s.audio_input_id)
                    && self
                        .devices
                        .outputs
                        .iter()
                        .any(|d| d.id == s.audio_output_id)))
    }
    pub fn refresh(&mut self) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.devices.warnings.clear();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Video(capture::enumerate()));
        });
        if !self.refreshing_audio {
            self.refreshing_audio = true;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Audio(
                    audio::input_devices(),
                    audio::output_devices(),
                ));
            });
        }
    }
    pub fn refresh_capture_mode(&mut self) {
        if self.active() || self.opening {
            return;
        }
        if let Some(d) = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)
        {
            let chosen = choose_mode(
                &d.modes,
                &self.settings.capture_preferences,
                self.settings.mode.as_ref(),
            );
            if chosen != self.settings.mode {
                self.settings.mode = chosen;
                self.dirty = Some(Instant::now());
            }
        }
    }
    /// Resolution and target FPS are independent UI choices; capture keeps an advertised native rate.
    pub fn select_resolution(&mut self, width: u32, height: u32) {
        self.settings.capture_preferences.automatic = false;
        self.settings.mode = self.mode_for_resolution(width, height);
        self.changed();
    }
    pub(super) fn mode_for_resolution(
        &self,
        width: u32,
        height: u32,
    ) -> Option<capture::VideoMode> {
        let device = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)?;
        let modes: Vec<_> = device
            .modes
            .iter()
            .filter(|m| m.width == width && m.height == height)
            .cloned()
            .collect();
        choose_mode(
            &modes,
            &capture::CapturePreferences {
                automatic: true,
                preferred_fps: self
                    .settings
                    .capture_preferences
                    .preferred_fps
                    .clamp(1, capture_config::MAX_FPS),
            },
            None,
        )
    }
}
