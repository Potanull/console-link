use super::*;

impl ConsoleApp {
    pub(super) fn refresh(&mut self, ctx: egui::Context) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut devices = Devices {
                video: vec![],
                inputs: vec![],
                outputs: vec![],
                warnings: vec![],
            };
            match capture::enumerate() {
                Ok(video) => devices.video = video,
                Err(e) => devices.warnings.push(e),
            }
            match audio::input_devices() {
                Ok(inputs) => devices.inputs = inputs,
                Err(e) => devices.warnings.push(e),
            }
            match audio::output_devices() {
                Ok(outputs) => devices.outputs = outputs,
                Err(e) => devices.warnings.push(e),
            }
            let _ = tx.send(Event::Devices(Ok(devices)));
            ctx.request_repaint();
        });
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
        self.refresh_capture_mode();
        let Some(device) = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)
        else {
            self.error = Some("Выбери карту захвата.".into());
            return;
        };
        let Some(mode) = self
            .settings
            .mode
            .clone()
            .filter(|m| device.modes.contains(m))
        else {
            self.error = Some("Выбери доступный режим карты.".into());
            return;
        };
        if self.settings.audio_input_id.is_empty() != self.settings.audio_output_id.is_empty() {
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
        std::thread::spawn(move || {
            let frame_cancelled = Arc::clone(&cancelled);
            let paint = ctx.clone();
            let sink: FrameSink = Arc::new(move |frame| {
                if frame_cancelled.load(Ordering::Acquire) {
                    return;
                }
                dropped.fetch_add(
                    send_latest(&frame_tx, &discard_frames, (generation, frame)),
                    Ordering::Relaxed,
                );
                paint.request_repaint();
            });
            let result = (|| {
                let video = capture::start(&settings.video_id, &mode, sink)?;
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                let audio = if settings.audio_input_id.is_empty() {
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
                    cancelled: Arc::clone(&cancelled),
                })
            })();
            let _ = tx.send(Event::Started(generation, result));
            ctx.request_repaint();
        });
    }

    pub(super) fn stop(&mut self) {
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
        self.presented_frame = None;
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
            match event {
                Event::Devices(result) => {
                    self.refreshing = false;
                    match result {
                        Ok(devices) => {
                            self.devices = devices;
                            self.refresh_capture_mode();
                            if !self.settings.video_id.is_empty()
                                && !self
                                    .devices
                                    .video
                                    .iter()
                                    .any(|d| d.id == self.settings.video_id)
                            {
                                self.notice = Some(
                                    "Сохраненная карта не найдена. Подключи ее и обнови список."
                                        .into(),
                                );
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                    if self.retry_refresh && self.reconnect.active() {
                        self.retry_refresh = false;
                        let video_ready = self.devices.video.iter().any(|d| {
                            d.id == self.settings.video_id
                                && self
                                    .settings
                                    .mode
                                    .as_ref()
                                    .is_some_and(|m| d.modes.contains(m))
                        });
                        let audio_ready = self.settings.audio_input_id.is_empty()
                            || (self
                                .devices
                                .inputs
                                .iter()
                                .any(|d| d.id == self.settings.audio_input_id)
                                && self
                                    .devices
                                    .outputs
                                    .iter()
                                    .any(|d| d.id == self.settings.audio_output_id));
                        if video_ready && audio_ready {
                            self.connect(ctx.clone());
                        }
                    }
                }
                Event::Started(generation, result) => {
                    if generation != self.generation || self.cancelled.load(Ordering::Acquire) {
                        if let Ok(active) = result {
                            std::thread::spawn(move || drop(active));
                        }
                        continue;
                    }
                    self.opening = false;
                    match result {
                        Ok(active) => {
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
        if self.reconnect.due(Instant::now()) && !self.opening && !self.refreshing {
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
        let presence =
            if self.active.is_some() && self.settings.discord_enabled && linked_account.is_some() {
                Some(Presence {
                    application_id: DEFAULT_APPLICATION_ID.to_owned(),
                    game: self
                        .settings
                        .game
                        .chars()
                        .filter(|character| !character.is_control())
                        .take(128)
                        .collect(),
                    started_at: self.started_unix,
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
            return;
        }
        let Some(frame) = self.presented_frame.clone() else {
            return;
        };
        let color = if original {
            console_link::color::ColorSettings::default()
        } else {
            self.presented_color
        };
        self.screenshot_pending = true;
        let directory = self.settings.screenshot_dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = color_integration::save_screenshot(&frame, &color, original, &directory);
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
                    let valid_video = self.devices.video.iter().any(|d| {
                        d.id == self.settings.video_id
                            && self
                                .settings
                                .mode
                                .as_ref()
                                .is_some_and(|m| d.modes.contains(m))
                    });
                    let valid_audio = (self.settings.audio_input_id.is_empty()
                        && self.settings.audio_output_id.is_empty())
                        || (self
                            .devices
                            .inputs
                            .iter()
                            .any(|d| d.id == self.settings.audio_input_id)
                            && self
                                .devices
                                .outputs
                                .iter()
                                .any(|d| d.id == self.settings.audio_output_id));
                    let can_connect = valid_video && valid_audio && !self.refreshing;
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
                        self.presented_frame.is_some() && !self.screenshot_pending,
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
                                    if !valid_video {
                                        "Выбери карту, чтобы начать"
                                    } else if !valid_audio {
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
                        if let Some(error) = &self.error {
                            ui.add_space(14.0);
                            message(ui, "Не удалось подключиться", error, true);
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
