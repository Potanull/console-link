use super::*;

impl Runtime {
    pub fn apply_audio(&self) {
        if let Some(a) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
            a.set_gain(self.settings.gain);
            a.set_muted(self.settings.muted);
            a.set_delay_ms(self.settings.sync_ms.max(0) as u32);
            a.set_equalizer(
                self.settings.equalizer.enabled,
                self.settings.equalizer.gains,
            );
        }
    }
    pub fn connect(&mut self) {
        self.refresh_capture_mode();
        if !self.ready() {
            self.error = Some("Выбери доступную карту и режим. Для аудио выбери вход и выход или включи «Захват без звука».".into());
            return;
        }
        let mode = self.settings.mode.clone().expect("ready validates mode");
        let recovering = self.reconnect.active();
        self.stop();
        self.recovery_attempt = recovering;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        self.opening = true;
        self.error = None;
        self.notice = None;
        self.capture_dropped.store(0, Ordering::Relaxed);
        self.history_dropped_start = self.history.dropped();
        let settings = self.settings.clone();
        let tx = self.tx.clone();
        let frames = self.frames_tx.clone();
        let discard = self.frames_rx.clone();
        let dropped = self.capture_dropped.clone();
        std::thread::spawn(move || {
            let frame_cancelled = cancelled.clone();
            let sink: FrameSink = Arc::new(move |frame| {
                if !frame_cancelled.load(Ordering::Acquire) {
                    dropped.fetch_add(
                        send_latest(&frames, &discard, (generation, frame)),
                        Ordering::Relaxed,
                    );
                }
            });
            let result = (|| {
                let video = capture::start(&settings.video_id, &mode, sink)?;
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                let audio = if !settings.audio_enabled || settings.audio_input_id.is_empty() {
                    None
                } else {
                    Some(AudioEngine::start(&AudioConfig {
                        input_id: settings.audio_input_id,
                        output_id: settings.audio_output_id,
                        gain: settings.gain,
                        muted: settings.muted,
                        delay_ms: settings.sync_ms.max(0) as u32,
                        eq_enabled: settings.equalizer.enabled,
                        eq_gains: settings.equalizer.gains,
                    })?)
                };
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                Ok(Active {
                    video,
                    audio,
                    cancelled: cancelled.clone(),
                })
            })();
            let _ = tx.send(Event::Started(generation, result));
        });
    }
    pub fn stop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.generation = self.generation.wrapping_add(1);
        if let Some(active) = self.active.take() {
            std::thread::spawn(move || drop(active));
        }
        self.opening = false;
        self.started = None;
        self.monitor.reset();
        self.history.clear();
        self.displayed = None;
        self.next_frame_deadline = None;
        self.reconnect.cancel();
        self.retry_refresh = false;
        self.recovery_attempt = false;
        while self.frames_rx.try_recv().is_ok() {}
        if let Some(d) = &self.discord {
            d.set(None);
        }
    }
    pub(super) fn disconnected(&mut self, error: String) {
        self.stop();
        self.error = Some(error);
        self.notice =
            Some("Ожидаю выбранную карту. Отключить отменяет повторное подключение.".into());
        self.reconnect.arm(Instant::now());
    }
    pub(super) fn current_generation(&self, generation: u64) -> bool {
        generation == self.generation && !self.cancelled.load(Ordering::Acquire)
    }
}
