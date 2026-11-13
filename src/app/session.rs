use super::*;

fn spawn_device_discovery(
    video: impl FnOnce() -> Result<Vec<VideoDevice>, String> + Send + 'static,
    audio: Option<
        impl FnOnce() -> (
            Result<Vec<AudioDeviceInfo>, String>,
            Result<Vec<AudioDeviceInfo>, String>,
        ) + Send
        + 'static,
    >,
    send: impl Fn(Event) + Clone + Send + 'static,
) {
    let send_video = send.clone();
    std::thread::spawn(move || send_video(Event::VideoDevices(video())));
    if let Some(audio) = audio {
        std::thread::spawn(move || {
            let (inputs, outputs) = audio();
            send(Event::AudioDevices { inputs, outputs });
        });
    }
}

impl Devices {
    fn update_video(&mut self, result: Result<Vec<VideoDevice>, String>) {
        self.video = result.unwrap_or_else(|error| {
            self.warnings.push(error);
            Vec::new()
        });
    }

    fn update_audio(
        &mut self,
        inputs: Result<Vec<AudioDeviceInfo>, String>,
        outputs: Result<Vec<AudioDeviceInfo>, String>,
    ) {
        self.inputs = inputs.unwrap_or_else(|error| {
            self.warnings.push(error);
            Vec::new()
        });
        self.outputs = outputs.unwrap_or_else(|error| {
            self.warnings.push(error);
            Vec::new()
        });
    }

    fn video_ready(&self, settings: &Settings) -> bool {
        self.video.iter().any(|device| {
            device.id == settings.video_id
                && settings
                    .mode
                    .as_ref()
                    .is_some_and(|mode| device.modes.contains(mode))
        })
    }

    fn audio_ready(&self, settings: &Settings) -> bool {
        (settings.audio_input_id.is_empty() && settings.audio_output_id.is_empty())
            || (!settings.audio_input_id.is_empty()
                && !settings.audio_output_id.is_empty()
                && self
                    .inputs
                    .iter()
                    .any(|device| device.id == settings.audio_input_id)
                && self
                    .outputs
                    .iter()
                    .any(|device| device.id == settings.audio_output_id))
    }

    fn ready(&self, settings: &Settings, video_pending: bool, audio_pending: bool) -> bool {
        let without_audio =
            settings.audio_input_id.is_empty() && settings.audio_output_id.is_empty();
        !video_pending
            && (!audio_pending || without_audio)
            && self.video_ready(settings)
            && self.audio_ready(settings)
    }
}

impl ConsoleApp {
    pub(super) fn refresh(&mut self, ctx: egui::Context) {
        if self.refreshing {
            return;
        }
        let video_only_diagnostic = self.diagnostic_card.is_some()
            && argument_value("--diagnose-audio-input").is_none()
            && argument_value("--diagnose-audio-output").is_none();
        let discover_audio = !video_only_diagnostic && !self.refreshing_audio;
        self.refreshing = true;
        self.refreshing_audio |= discover_audio;
        self.devices.warnings.clear();
        let tx = self.tx.clone();
        let diagnostics = self.connection_diagnostics.clone();
        if let Some(trace) = &diagnostics {
            trace.record("enumeration_start", "independent device discovery");
        }
        spawn_device_discovery(
            capture::enumerate,
            discover_audio.then_some(|| (audio::input_devices(), audio::output_devices())),
            move |event| {
                if let Some(trace) = &diagnostics {
                    match &event {
                        Event::VideoDevices(result) => trace.record(
                            "enumeration_video_ready",
                            &format!("count={}", result.as_ref().map_or(0, Vec::len)),
                        ),
                        Event::AudioDevices { .. } => {
                            trace.record("enumeration_audio_ready", "audio discovery complete")
                        }
                        _ => unreachable!("discovery emits only device results"),
                    }
                }
                let _ = tx.send(event);
                ctx.request_repaint();
            },
        );
    }

    pub(super) fn refresh_capture_mode(&mut self) {
        if self.active.is_some() || self.opening {
            return;
        }
        if let Some(device) = self
            .devices
            .video
            .iter()
            .find(|device| device.id == self.settings.video_id)
        {
            let chosen = choose_mode(
                &device.modes,
                &self.settings.capture_preferences,
                self.settings.mode.as_ref(),
            );
            if chosen != self.settings.mode {
                self.settings.mode = chosen;
                self.dirty = Some(Instant::now());
            }
        }
    }

    pub(super) fn connect(&mut self, ctx: egui::Context) {
        if let Some(trace) = &self.connection_diagnostics {
            trace.record("connect_requested", "production connection handler entered");
        }
        if self.refreshing
            || (self.refreshing_audio
                && (!self.settings.audio_input_id.is_empty()
                    || !self.settings.audio_output_id.is_empty()))
        {
            self.error = Some("Дождись обнаружения выбранных устройств.".into());
            return;
        }
        self.refresh_capture_mode();
        let Some(device) = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)
        else {
            if let Some(trace) = &self.connection_diagnostics {
                trace.record("validation_failed", "selected capture card was not found");
            }
            self.error = Some("Выбери карту захвата.".into());
            return;
        };
        let Some(mode) = self
            .settings
            .mode
            .clone()
            .filter(|m| device.modes.contains(m))
        else {
            if let Some(trace) = &self.connection_diagnostics {
                trace.record(
                    "validation_failed",
                    "selected capture mode is not advertised",
                );
            }
            self.error = Some("Выбери доступный режим карты.".into());
            return;
        };
        if self.settings.audio_input_id.is_empty() != self.settings.audio_output_id.is_empty() {
            if let Some(trace) = &self.connection_diagnostics {
                trace.record(
                    "validation_failed",
                    "audio requires both selected input and output",
                );
            }
            self.error = Some("Для звука выбери и вход карты, и аудиовыход. Для видео без звука убери оба выбора.".into());
            return;
        }
        let recovering = self.reconnect.active();
        self.stop();
        self.recovery_attempt = recovering;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&self.cancelled);
        self.opening = true;
        self.error = None;
        self.notice = None;
        self.capture_dropped.store(0, Ordering::Relaxed);
        self.history_dropped_start = self.history.dropped();
        let frame_tx = self.frames_tx.clone();
        let discard_frames = self.frames_rx.clone();
        let tx = self.tx.clone();
        let settings = self.settings.clone();
        let dropped = Arc::clone(&self.capture_dropped);
        let diagnostics = self.connection_diagnostics.clone();
        std::thread::spawn(move || {
            if let Some(trace) = &diagnostics {
                trace.record("worker_started", &format!("generation={generation}"));
            }
            let frame_cancelled = Arc::clone(&cancelled);
            let paint = ctx.clone();
            let frame_trace = diagnostics.clone();
            let first_frame_recorded = AtomicBool::new(false);
            let sink: FrameSink = Arc::new(move |frame| {
                if frame_cancelled.load(Ordering::Acquire) {
                    return;
                }
                if let Some(trace) = &frame_trace
                    && !first_frame_recorded.swap(true, Ordering::Relaxed)
                {
                    trace.record(
                        "first_frame",
                        &format!(
                            "generation={generation} width={} height={}",
                            frame.width, frame.height
                        ),
                    );
                }
                dropped.fetch_add(
                    send_latest(&frame_tx, &discard_frames, (generation, frame)),
                    Ordering::Relaxed,
                );
                paint.request_repaint();
            });
            let result = (|| {
                if let Some(trace) = &diagnostics {
                    trace.record(
                        "video_start",
                        &format!(
                            "generation={generation} width={} height={} fps={}/{}",
                            mode.width, mode.height, mode.fps_num, mode.fps_den
                        ),
                    );
                }
                let video =
                    capture::start(&settings.video_id, &mode, sink).inspect_err(|error| {
                        if let Some(trace) = &diagnostics {
                            trace.record("video_error", error);
                        }
                    })?;
                if let Some(trace) = &diagnostics {
                    trace.record("video_ready", &format!("generation={generation}"));
                }
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                let audio = if settings.audio_input_id.is_empty() {
                    if let Some(trace) = &diagnostics {
                        trace.record("audio_skipped", "video-only connection");
                    }
                    None
                } else {
                    if let Some(trace) = &diagnostics {
                        trace.record("audio_start", &format!("generation={generation}"));
                    }
                    let audio = AudioEngine::start(&AudioConfig {
                        input_id: settings.audio_input_id,
                        output_id: settings.audio_output_id,
                        gain: settings.gain,
                        muted: settings.muted,
                        delay_ms: settings.sync_ms.max(0) as u32,
                        eq_enabled: settings.equalizer.enabled,
                        eq_gains: settings.equalizer.gains,
                    })
                    .inspect_err(|error| {
                        if let Some(trace) = &diagnostics {
                            trace.record("audio_error", error);
                        }
                    })?;
                    if let Some(trace) = &diagnostics {
                        trace.record("audio_ready", &format!("generation={generation}"));
                    }
                    Some(audio)
                };
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                Ok(Active {
                    video,
                    audio,
                    cancelled: Arc::clone(&cancelled),
                })
            })();
            let _ = tx.send(Event::Started(generation, result));
            ctx.request_repaint();
        });
    }

    pub(super) fn stop(&mut self) {
        self.cursor_visibility.set_hidden(false);
        self.session_started = None;
        self.monitor.reset();
        self.frame_pacer.reset();
        self.reconnect.cancel();
        self.retry_refresh = false;
        self.recovery_attempt = false;
        self.cancelled.store(true, Ordering::Release);
        if let Some(active) = self.active.take() {
            std::thread::spawn(move || drop(active));
        }
        self.opening = false;
        self.history.clear();
        self.displayed = None;
        self.texture = None;
        self.presented_image = None;
        self.color_original = false;
        while self.frames_rx.try_recv().is_ok() {}
        self.discord.set(None);
    }

    pub(super) fn disconnected(&mut self, error: String) {
        self.stop();
        self.error = Some(error);
        self.notice = Some(
            "Ожидаю выбранную карту и устройства звука. Отключить отменяет повторное подключение."
                .into(),
        );
        self.reconnect.arm(Instant::now());
    }

    pub(super) fn events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            let discovering = self.refreshing || self.refreshing_audio;
            match event {
                Event::VideoDevices(result) => {
                    self.refreshing = false;
                    self.devices.update_video(result);
                    self.refresh_capture_mode();
                    if !self.settings.video_id.is_empty()
                        && !self
                            .devices
                            .video
                            .iter()
                            .any(|d| d.id == self.settings.video_id)
                    {
                        self.notice = Some(
                            "Сохраненная карта не найдена. Подключи ее и обнови список.".into(),
                        );
                    }
                }
                Event::AudioDevices { inputs, outputs } => {
                    self.refreshing_audio = false;
                    self.devices.update_audio(inputs, outputs);
                }
                Event::Started(generation, result) => {
                    if generation != self.generation || self.cancelled.load(Ordering::Acquire) {
                        if let Some(trace) = &self.connection_diagnostics {
                            trace
                                .record("started_event_stale", &format!("generation={generation}"));
                        }
                        if let Ok(active) = result {
                            std::thread::spawn(move || drop(active));
                        }
                        continue;
                    }
                    self.opening = false;
                    match result {
                        Ok(active) => {
                            if let Some(trace) = &self.connection_diagnostics {
                                trace.record(
                                    "started_event_ok",
                                    &format!("generation={generation}"),
                                );
                            }
                            if let Some(audio) = &active.audio {
                                audio.set_gain(self.settings.gain);
                                audio.set_equalizer(
                                    self.settings.equalizer.enabled,
                                    self.settings.equalizer.gains,
                                );
                                audio.set_muted(self.settings.muted);
                                audio.set_delay_ms(self.settings.sync_ms.max(0) as u32);
                            }
                            self.reconnect.cancel();
                            self.recovery_attempt = false;
                            self.active = Some(active);
                            self.started = Instant::now();
                            self.last_frame = Instant::now();
                            self.started_unix = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;
                            self.session_started = Some(Instant::now());
                            self.monitor.reset();
                            self.rate_since = Instant::now();
                            self.received = 0;
                            self.fps = 0.0;
                        }
                        Err(e) => {
                            if let Some(trace) = &self.connection_diagnostics {
                                trace.record(
                                    "started_event_error",
                                    &format!("generation={generation} error={e}"),
                                );
                            }
                            self.cancelled.store(true, Ordering::Release);
                            self.error = Some(e);
                            self.history.clear();
                            if self.recovery_attempt {
                                self.reconnect.arm(Instant::now());
                                self.reconnect.failed(Instant::now());
                            }
                        }
                    }
                }
                Event::Screenshot(result) => {
                    self.screenshot_pending = false;
                    match result {
                        Ok(path) => self.notice = Some(format!("Скриншот сохранен: {path}")),
                        Err(e) => self.error = Some(format!("Не удалось сохранить скриншот: {e}")),
                    }
                }
            }
            if discovering
                && !self.refreshing
                && !self.refreshing_audio
                && let Some(trace) = &self.connection_diagnostics
            {
                trace.record("enumeration_complete", "device discovery complete");
            }
        }
        if self.retry_refresh && self.reconnect.active() {
            if self
                .devices
                .ready(&self.settings, self.refreshing, self.refreshing_audio)
            {
                self.retry_refresh = false;
                self.connect(ctx.clone());
            } else if !self.refreshing && !self.refreshing_audio {
                self.retry_refresh = false;
            }
        }
        let delay_ns = (-self.settings.sync_ms.min(0)) as u64 * 1_000_000;
        self.history.set_retention_ns(delay_ns + 60_000_000);
        while let Ok((generation, frame)) = self.frames_rx.try_recv() {
            if generation == self.generation && !self.cancelled.load(Ordering::Acquire) {
                let fps = self
                    .settings
                    .mode
                    .as_ref()
                    .map(|m| m.fps_num as f64 / m.fps_den.max(1) as f64)
                    .unwrap_or(60.0);
                let max_delay = self.history.max_delay_ms(frame.rgba.len(), fps) as i32;
                if self.settings.sync_ms < -max_delay {
                    self.settings.sync_ms = -max_delay;
                    self.notice = Some(format!(
                        "Задержка видео ограничена {max_delay} мс по доступному бюджету памяти."
                    ));
                    self.dirty = Some(Instant::now());
                }
                self.history.push(Arc::new(frame));
                self.last_frame = Instant::now();
                self.received += 1;
            }
        }
        if self.rate_since.elapsed() >= Duration::from_secs(1) {
            self.fps = self.received as f64 / self.rate_since.elapsed().as_secs_f64();
            self.received = 0;
            self.rate_since = Instant::now();
            if let Some(trace) = &self.connection_diagnostics
                && let Some(stats) = self
                    .active
                    .as_ref()
                    .and_then(|active| active.video.statistics())
            {
                trace.record(
                    "frame_rate",
                    &format!(
                        "ui_fps={:.2} callbacks={} delivered={} native_dropped={} queue_dropped={} conversion_ms_total={:.2} active_min_fps={:?} active_max_fps={:?} size={}x{}",
                        self.fps, stats.callback_frames, stats.delivered_frames,
                        stats.dropped_frames, self.capture_dropped.load(Ordering::Relaxed),
                        stats.conversion_ns as f64 / 1_000_000.0,
                        stats.active_min_fps, stats.active_max_fps, stats.width, stats.height
                    ),
                );
            }
        }
        let now_ns = monotonic_ns();
        if self.active.is_some()
            && self
                .frame_pacer
                .ready(now_ns, self.settings.capture_preferences.preferred_fps)
            && let Some(frame) = self.history.select(now_ns, delay_ns)
        {
            if !self.color_gpu_available {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.width as usize, frame.height as usize],
                    &frame.rgba,
                );
                if let Some(texture) = &mut self.texture {
                    texture.set(image, egui::TextureOptions::LINEAR);
                } else {
                    self.texture = Some(ctx.load_texture(
                        "console-frame",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            self.frame_pacer.presented(now_ns);
            self.displayed = Some(frame);
        }
        let stream_error = self.active.as_ref().and_then(|active| {
            active
                .video
                .error()
                .or_else(|| active.audio.as_ref().and_then(AudioEngine::error))
        });
        if let Some(error) = stream_error {
            self.disconnected(error);
        } else if self.active.is_some() && self.last_frame.elapsed() > config::SIGNAL_TIMEOUT {
            self.disconnected(
                "Кадры не поступают. Проверь USB, HDMI и выбранную карту. Подключение остановлено."
                    .into(),
            );
        }
        if self.reconnect.due(Instant::now())
            && !self.opening
            && !self.refreshing
            && (!self.refreshing_audio
                || (self.settings.audio_input_id.is_empty()
                    && self.settings.audio_output_id.is_empty()))
        {
            self.reconnect.failed(Instant::now());
            self.retry_refresh = true;
            self.refresh(ctx.clone());
        }
        if self
            .dirty
            .is_some_and(|since| since.elapsed() > config::SETTINGS_SAVE_DELAY)
        {
            self.dirty = None;
            if let Some(store) = &self.store
                && let Err(error) = store.save(&self.settings)
            {
                self.notice = Some(error);
            }
        }
        if let Some(auth) = &self.discord_auth {
            let completion = auth.completion_generation();
            if completion != self.auth_completion {
                self.auth_completion = completion;
                self.settings.discord_enabled = true;
                self.dirty = Some(Instant::now());
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.page = Page::Discord;
            }
        }
        let current_game = self.current_game();
        let linked_account = self.discord_auth.as_ref().and_then(DiscordAuth::account);
        self.discord
            .require_account(linked_account.as_ref().map(|account| account.id.clone()));
        let presence = if (self.active.is_some() || self.settings.nintendo_enabled)
            && !self.game_name().is_empty()
            && self.settings.discord_enabled
            && linked_account.is_some()
        {
            Some(Presence {
                application_id: DEFAULT_APPLICATION_ID.to_owned(),
                game: self
                    .game_name()
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(128)
                    .collect(),
                started_at: self.game_started_at(),
                cover_url: current_game
                    .as_ref()
                    .and_then(|game| game.cover_url.clone()),
                game_url: current_game.as_ref().and_then(|game| game.game_url.clone()),
            })
        } else {
            None
        };
        self.discord.set(presence);
    }

    pub(super) fn screenshot(&mut self, ctx: egui::Context) {
        self.screenshot_with_color(ctx, false);
    }
    pub(super) fn screenshot_with_color(&mut self, ctx: egui::Context, original: bool) {
        if self.screenshot_pending {
            self.notice = Some("Скриншот уже сохраняется…".into());
            return;
        }
        let Some(image) = self.presented_image.clone() else {
            return;
        };
        self.screenshot_pending = true;
        let directory = self.settings.screenshot_dir.clone();
        let tx = self.tx.clone();
        let gpu = self.export_gpu.clone();
        std::thread::spawn(move || {
            let result = if original {
                color_integration::save_screenshot(
                    &image.frame,
                    &Default::default(),
                    true,
                    &directory,
                )
            } else if let Some(state) = gpu {
                upscale_gpu::render_export(
                    &state.device,
                    &state.queue,
                    image.frame.clone(),
                    image.color,
                    image.upscale,
                    image.target,
                )
                .and_then(|rgba| {
                    let output = VideoFrame {
                        width: image.target[0],
                        height: image.target[1],
                        rgba,
                        timestamp_ns: image.frame.timestamp_ns,
                    };
                    color_integration::save_screenshot(
                        &output,
                        &Default::default(),
                        false,
                        &directory,
                    )
                })
            } else {
                Err("GPU-экспорт недоступен. Можно сохранить исходный кадр.".into())
            };
            let _ = tx.send(Event::Screenshot(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn controls(&mut self, ctx: &egui::Context) {
        let before = self.settings.clone();
        let compact = ctx.content_rect().width() < 900.0;
        let running = self.active.is_some();
        let status = if running {
            "Подключено"
        } else if self.opening {
            "Подключение"
        } else if self.reconnect.active() {
            "Ожидание карты"
        } else {
            "Не подключено"
        };

        egui::SidePanel::left("navigation")
            .exact_width(if compact { 74.0 } else { 204.0 })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(14, 24)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.brand).fit_to_exact_size(egui::vec2(42.0, 42.0)));
                    if !compact {
                        ui.vertical(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Console Link").size(15.0).strong());
                            theme::label(ui, "ИГРА БЛИЖЕ");
                        });
                    }
                });
                ui.add_space(35.0);
                for (page, icon, label) in [
                    (Page::Session, Icon::Session, "Подключение"),
                    (Page::Audio, Icon::Audio, "Звук"),
                    (Page::Color, Icon::Color, "Изображение"),
                    (Page::Discord, Icon::Discord, "Discord"),
                    (Page::Settings, Icon::Settings, "Настройки"),
                ] {
                    if theme::nav(ui, icon, label, self.page == page, compact).clicked() {
                        self.page = page;
                    }
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(if compact {
                            format!("v{}", env!("CARGO_PKG_VERSION"))
                        } else {
                            format!("Console Link  {}", env!("CARGO_PKG_VERSION"))
                        })
                        .size(10.5)
                        .color(theme::SUBTLE),
                    );
                    if !compact {
                        theme::label(ui, "Только ты и твоя игра.");
                        ui.add_space(10.0);
                    }
                });
            });

        egui::TopBottomPanel::bottom("session-actions")
            .exact_height(83.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(28, 18))
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let stop = running || self.opening || self.reconnect.active();
                    let can_connect =
                        self.devices
                            .ready(&self.settings, self.refreshing, self.refreshing_audio);
                    ui.add_enabled_ui(stop || can_connect, |ui| {
                        if theme::action(
                            ui,
                            if stop { Icon::Stop } else { Icon::Play },
                            if stop {
                                "Отключить"
                            } else {
                                "Подключить консоль"
                            },
                            true,
                            210.0,
                        )
                        .clicked()
                        {
                            if let Some(trace) = &self.connection_diagnostics {
                                trace.record(
                                    "button_clicked",
                                    if stop { "stop" } else { "connect" },
                                );
                            }
                            if stop {
                                self.stop();
                            } else {
                                self.connect(ctx.clone());
                            }
                        }
                    });
                    ui.add_space(5.0);
                    ui.add_enabled_ui(self.displayed.is_some() && !self.screenshot_pending, |ui| {
                        if theme::action(ui, Icon::Camera, "Скриншот", false, 132.0).clicked()
                        {
                            self.screenshot(ctx.clone());
                        }
                    });
                    ui.add_enabled_ui(
                        self.presented_image.is_some() && !self.screenshot_pending,
                        |ui| {
                            ui.menu_button("Ещё", |ui| {
                                if ui.button("Сохранить исходный кадр").clicked()
                                {
                                    self.screenshot_with_color(ctx.clone(), true);
                                    ui.close();
                                }
                            });
                        },
                    );
                    if !compact {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(if running {
                                    "F1 открывает боковое меню"
                                } else if self.opening {
                                    "Готовим изображение и звук…"
                                } else {
                                    if self.error.is_some() {
                                        "Подключение не удалось"
                                    } else if self.refreshing {
                                        "Ищем видеоустройства…"
                                    } else if !self.devices.video_ready(&self.settings) {
                                        "Выбери карту, чтобы начать"
                                    } else if self.refreshing_audio
                                        && (!self.settings.audio_input_id.is_empty()
                                            || !self.settings.audio_output_id.is_empty())
                                    {
                                        "Ищем аудиоустройства…"
                                    } else if !self.devices.audio_ready(&self.settings) {
                                        "Выбери аудиовход и выход"
                                    } else {
                                        "Все готово к подключению"
                                    }
                                })
                                .size(11.0)
                                .color(theme::MUTED),
                            );
                        });
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(28, 25)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::label(ui, "ТВОЕ ИГРОВОЕ ПРОСТРАНСТВО");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::status(ui, status, running);
                    });
                });
                ui.add_space(15.0);
                if compact {
                    ui.spacing_mut().scroll.floating = false;
                    ui.spacing_mut().scroll.bar_width = 4.0;
                    ui.spacing_mut().scroll.bar_inner_margin = 8.0;
                }
                // Connection failures must remain visible even when device cards overflow.
                if let Some(error) = &self.error {
                    message(ui, "Не удалось подключиться", error, true);
                    ui.add_space(12.0);
                }
                if !running && !self.opening && self.refreshing_audio
                    && (!self.settings.audio_input_id.is_empty() || !self.settings.audio_output_id.is_empty())
                {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Ищем выбранные аудиоустройства. Подключение со звуком станет доступно после проверки.");
                    });
                    ui.add_space(12.0);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("menu-page", self.page as u8))
                    .scroll_bar_visibility(if compact {
                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible
                    } else {
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                    })
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.page {
                            Page::Session => self.session_page(ui, ctx),
                            Page::Audio => self.audio_page(ui),
                            Page::Color => self.color_page(ui),
                            Page::Discord => self.discord_page(ui),
                            Page::Settings => self.settings_page(ui, ctx),
                        }
                        if let Some(notice) = &self.notice {
                            ui.add_space(10.0);
                            message(ui, "Обрати внимание", notice, false);
                        }
                        for warning in &self.devices.warnings {
                            ui.add_space(10.0);
                            message(ui, "Устройство недоступно", warning, false);
                        }
                    });
            });
        if before != self.settings {
            self.dirty = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    fn video() -> VideoDevice {
        VideoDevice {
            id: "synthetic-video".into(),
            name: "Synthetic card".into(),
            modes: vec![capture::VideoMode {
                width: 1920,
                height: 1080,
                fps_num: 50,
                fps_den: 1,
            }],
        }
    }

    fn devices() -> Devices {
        Devices {
            video: vec![video()],
            inputs: vec![],
            outputs: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn blocked_audio_discovery_does_not_delay_video() {
        let (audio_entered_tx, audio_entered_rx) = std::sync::mpsc::channel();
        let (release_audio_tx, release_audio_rx) = std::sync::mpsc::channel();
        let (tx, rx) = crossbeam_channel::unbounded();
        spawn_device_discovery(
            || Ok(vec![video()]),
            Some(move || {
                audio_entered_tx.send(()).unwrap();
                release_audio_rx.recv().unwrap();
                (Err("Synthetic input failure".into()), Ok(Vec::new()))
            }),
            move |event| tx.send(event).unwrap(),
        );
        audio_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(event, Event::VideoDevices(Ok(video)) if video.len() == 1));
        release_audio_tx.send(()).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Event::AudioDevices {
                inputs: Err(_),
                outputs: Ok(_)
            }
        ));
    }

    #[test]
    fn failed_discovery_preserves_other_results_and_warnings() {
        let mut devices = devices();
        devices.update_audio(
            Err("Synthetic input failure".into()),
            Ok(vec![AudioDeviceInfo {
                id: "synthetic-output".into(),
                name: "Synthetic output".into(),
            }]),
        );
        assert_eq!(devices.video.len(), 1);
        assert_eq!(devices.outputs.len(), 1);
        devices.update_video(Err("Synthetic video failure".into()));
        assert_eq!(devices.outputs.len(), 1);
        assert!(devices.video.is_empty());
        assert_eq!(
            devices.warnings,
            ["Synthetic input failure", "Synthetic video failure"]
        );
    }

    #[test]
    fn connection_waits_only_for_its_selected_devices() {
        let mut devices = devices();
        let mut settings = Settings {
            video_id: "synthetic-video".into(),
            mode: Some(video().modes.remove(0)),
            ..Settings::default()
        };
        assert!(!devices.ready(&settings, true, false));
        assert!(devices.ready(&settings, false, true));
        settings.audio_input_id = "synthetic-input".into();
        settings.audio_output_id = "synthetic-output".into();
        devices.update_audio(
            Ok(vec![AudioDeviceInfo {
                id: settings.audio_input_id.clone(),
                name: "Synthetic input".into(),
            }]),
            Ok(vec![AudioDeviceInfo {
                id: settings.audio_output_id.clone(),
                name: "Synthetic output".into(),
            }]),
        );
        assert!(!devices.ready(&settings, false, true));
        assert!(devices.ready(&settings, false, false));
        settings.audio_output_id.clear();
        assert!(!devices.ready(&settings, false, false));
        assert_eq!(settings.video_id, "synthetic-video");
    }
}
