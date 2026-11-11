use super::*;

impl ConsoleApp {
    pub(super) fn session_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::section_heading(ui, "Подключение", "Все готово для новой игровой сессии.");
        let running = self.active.is_some();
        if ui.ctx().content_rect().height() >= 680.0 {
            theme::card().inner_margin(22).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(95.0);
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.brand).fit_to_exact_size(egui::vec2(82.0, 82.0)));
                    ui.add_space(17.0);
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(if running {
                                "Хорошей игры."
                            } else if self.opening {
                                "Еще немного…"
                            } else {
                                "Твоя консоль. Твой экран."
                            })
                            .size(if ui.available_width() < 350.0 {
                                22.0
                            } else {
                                26.0
                            })
                            .strong(),
                        );
                        ui.add_space(5.0);
                        theme::subtitle(
                            ui,
                            if running {
                                "F1 открывает настройки поверх игры. Делись окном в Discord или OBS."
                            } else {
                                "Подключи карту захвата и играй на экране компьютера."
                            },
                        );
                        ui.add_space(13.0);
                        ui.horizontal(|ui| {
                            theme::icon(ui, Icon::Plug, 14.0, theme::MUTED);
                            theme::label(ui, "КОНСОЛЬ");
                            theme::label(ui, "-  HDMI  -");
                            theme::label(ui, "КАРТА");
                            theme::label(ui, "-  USB  -");
                            theme::label(ui, "КОМПЬЮТЕР");
                        });
                    });
                });
            });
            ui.add_space(17.0);
        }
        let locked = running || self.opening;
        ui.add_enabled_ui(!locked, |ui| {
            if ui.available_width() >= 650.0 {
                let width = (ui.available_width() - 16.0) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        self.video_card(ui, ctx);
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        self.audio_devices_card(ui);
                    });
                });
            } else {
                self.video_card(ui, ctx);
                ui.add_space(14.0);
                self.audio_devices_card(ui);
            }
        });
        ui.add_space(17.0);
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Session, 15.0, theme::SUBTLE);
            ui.label(
                RichText::new("F1  меню     F11  полный экран     F8  скриншот     M  звук")
                    .size(11.0)
                    .color(theme::SUBTLE),
            );
        });
    }

    pub(super) fn video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::card().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.set_min_height(230.0);
            card_title(
                ui,
                Icon::Session,
                "Изображение",
                "Источник и качество захвата",
            );
            theme::label(ui, "КАРТА ЗАХВАТА");
            let selected = self
                .devices
                .video
                .iter()
                .find(|d| d.id == self.settings.video_id)
                .map(|d| d.name.as_str())
                .unwrap_or("Выбери устройство");
            let previous = self.settings.video_id.clone();
            egui::ComboBox::from_id_salt("video")
                .truncate()
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for device in &self.devices.video {
                        ui.selectable_value(
                            &mut self.settings.video_id,
                            device.id.clone(),
                            &device.name,
                        );
                    }
                });
            if previous != self.settings.video_id {
                self.settings.mode = None;
            }
            ui.add_space(9.0);
            theme::label(ui, "РЕЖИМ ЗАХВАТА");
            self.refresh_capture_mode();
            if let Some(mode) = &self.settings.mode {
                ui.label(RichText::new(mode.to_string()).size(15.0));
                if self.settings.capture_preferences.automatic {
                    theme::subtitle(
                        ui,
                        &format!(
                            "Автоматически · цель {} FPS",
                            self.settings.capture_preferences.preferred_fps
                        ),
                    );
                }
                if nominal_fps(mode) != self.settings.capture_preferences.preferred_fps {
                    theme::subtitle(
                        ui,
                        &format!(
                            "Режим карты: {} FPS. Ограничение показа: {} FPS.",
                            nominal_fps(mode),
                            self.settings.capture_preferences.preferred_fps
                        ),
                    );
                }
            } else {
                theme::subtitle(ui, "Выбери карту. Качество настроится автоматически.");
            }
            if ui.small_button("Изменить в настройках").clicked() {
                self.page = Page::Settings;
            }
            ui.add_space(13.0);
            ui.horizontal(|ui| {
                theme::icon(ui, Icon::Refresh, 13.0, theme::MUTED);
                if self.refreshing {
                    ui.spinner();
                    theme::label(ui, "Ищем устройства…");
                } else if ui
                    .add(egui::Button::new("Обновить список").small().frame(false))
                    .clicked()
                {
                    self.refresh(ctx.clone());
                }
            });
            if self.devices.video.is_empty() {
                theme::subtitle(ui, "Пока не видим карту. Проверь USB-подключение.");
            }
        });
    }

    pub(super) fn audio_devices_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.set_min_height(230.0);
            card_title(ui, Icon::Audio, "Звук", "От консоли к твоим наушникам");
            audio_combo(
                ui,
                "audio-in",
                "АУДИОВХОД КАРТЫ",
                &mut self.settings.audio_input_id,
                &self.devices.inputs,
            );
            ui.add_space(9.0);
            audio_combo(
                ui,
                "audio-out",
                "КУДА ВЫВОДИТЬ ЗВУК",
                &mut self.settings.audio_output_id,
                &self.devices.outputs,
            );
            ui.add_space(13.0);
            theme::subtitle(ui, "Для игры без звука оставь оба поля пустыми.");
        });
    }

    pub(super) fn audio_page(&mut self, ui: &mut egui::Ui) {
        let compact = self.playback.playing;
        if !compact {
            theme::section_heading(ui, "Звук", "Комфортная громкость и точная синхронизация.");
        }
        let previous_gain = self.settings.gain;
        let previous_mute = self.settings.muted;
        let previous_offset = self.settings.sync_ms;
        theme::card()
            .inner_margin(if compact { 12 } else { 18 })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.spacing_mut().interact_size.y = 28.0;
                ui.visuals_mut().selection.bg_fill = theme::TEXT;
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Audio, 20.0, theme::TEXT);
                    ui.add_space(3.0);
                    ui.label(RichText::new("Громкость консоли").size(17.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{:.0}%", self.settings.gain * 100.0)).size(25.0),
                        );
                    });
                });
                ui.add_space(if compact { 5.0 } else { 12.0 });
                ui.style_mut().spacing.slider_width = ui.available_width();
                ui.add(egui::Slider::new(&mut self.settings.gain, 0.0..=1.0).show_value(false));
                ui.add_space(9.0);
                theme::toggle(ui, &mut self.settings.muted, "Выключить звук консоли");
                if !compact {
                    ui.add_space(12.0);
                    let peak = self
                        .active
                        .as_ref()
                        .and_then(|a| a.audio.as_ref())
                        .map(AudioEngine::peak)
                        .unwrap_or(0.0);
                    level_meter(ui, peak);
                    ui.add_space(9.0);
                    theme::subtitle(
                        ui,
                        "Громкость и выключение звука действуют и на демонстрацию в Discord.",
                    );
                }
            });
        ui.add_space(12.0);
        self.equalizer_card(ui);
        ui.add_space(12.0);
        theme::card()
            .inner_margin(if compact { 12 } else { 18 })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.spacing_mut().interact_size.y = 28.0;
                ui.visuals_mut().selection.bg_fill = theme::TEXT;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Синхронизация").size(17.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{:+} мс", self.settings.sync_ms)).size(20.0),
                        );
                    });
                });
                ui.add_space(8.0);
                if !compact {
                    theme::subtitle(
                        ui,
                        "Подстрой звук, если он опережает изображение или отстает.",
                    );
                }
                ui.add_space(12.0);
                ui.style_mut().spacing.slider_width = ui.available_width();
                ui.add(
                    egui::Slider::new(
                        &mut self.settings.sync_ms,
                        -capture_config::MAX_SYNC_MS..=capture_config::MAX_SYNC_MS,
                    )
                    .show_value(false),
                );
                ui.horizontal(|ui| {
                    theme::label(ui, "Задержать изображение");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::label(ui, "Задержать звук");
                    });
                });
                ui.add_space(8.0);
                if ui
                    .add(egui::Button::new("Сбросить коррекцию").small())
                    .clicked()
                {
                    self.settings.sync_ms = 0;
                }
                ui.add_space(0.0);
                if !compact {
                    theme::subtitle(ui, "Коррекция добавляет задержку выбранному потоку.");
                }
            });
        if let Some(audio) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
            if previous_gain != self.settings.gain {
                audio.set_gain(self.settings.gain);
            }
            if previous_mute != self.settings.muted {
                audio.set_muted(self.settings.muted);
            }
            if previous_offset != self.settings.sync_ms {
                audio.set_delay_ms(self.settings.sync_ms.max(0) as u32);
            }
        }
        if previous_offset != self.settings.sync_ms {
            self.history.clear();
            self.displayed = None;
            self.texture = None;
        }
    }

    pub(super) fn current_game(&mut self) -> Option<Game> {
        if self.game_metadata_name != self.game_name() {
            self.game_metadata_name = self.game_name().to_owned();
            self.game_metadata = catalog::find_game(self.game_name());
        }
        let mut game = self.game_metadata.clone();
        if self.settings.nintendo_enabled && !self.game_name().is_empty() {
            let entry = game.get_or_insert_with(|| Game {
                id: String::new(),
                name: self.game_name().to_owned(),
                aliases: Vec::new(),
                cover_url: None,
                game_url: None,
            });
            if let Some(cover) = self.nintendo_cover() {
                entry.cover_url = Some(cover.to_owned());
            }
        }
        game
    }

    pub(super) fn choose_game(&mut self, game: String, ctx: &egui::Context) {
        self.settings.nintendo_enabled = false;
        self.settings.game = game;
        self.started_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs() as i64)
            .unwrap_or(0);
        self.dirty = Some(Instant::now());
        if self.active.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{} - Console Link",
                self.settings.game
            )));
        }
    }

    pub(super) fn discord_page(&mut self, ui: &mut egui::Ui) {
        theme::section_heading(ui, "Discord", "Твоя игра. Твой профиль. Твои друзья.");
        let ctx = ui.ctx().clone();
        let auth_status = self
            .discord_auth
            .as_ref()
            .map(DiscordAuth::status)
            .unwrap_or(AuthStatus::Disconnected);
        let auth_status = if self.snapshot_path.is_some()
            && std::env::args().any(|arg| arg == "--ui-profile-preview")
        {
            AuthStatus::Connected(DiscordAccount {
                id: "1569416573180051537".into(),
                display_name: "Console Link Player".into(),
                avatar_url: Some("https://cdn.discordapp.com/embed/avatars/0.png".into()),
            })
        } else {
            auth_status
        };
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match &auth_status {
                AuthStatus::Connected(account) => {
                    ui.horizontal(|ui| {
                        if let Some(avatar) = account
                            .avatar_url
                            .as_deref()
                            .and_then(|url| self.artwork.image(&ctx, url))
                        {
                            ui.add(
                                egui::Image::new(&avatar)
                                    .fit_to_exact_size(egui::vec2(60.0, 60.0))
                                    .corner_radius(30),
                            );
                        } else {
                            theme::icon(ui, Icon::Discord, 48.0, theme::TEXT);
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&account.display_name).size(22.0).strong());
                            theme::subtitle(ui, "Аккаунт Discord подключен");
                        });
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Отключить").clicked() {
                            if let Some(auth) = &self.discord_auth {
                                auth.disconnect();
                            }
                            self.discord.require_account(None);
                            self.discord.set(None);
                        }
                    });
                }
                AuthStatus::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Подтверди подключение в Discord");
                    });
                    theme::subtitle(ui, "После входа приложение откроется автоматически.");
                    if ui.button("Отменить").clicked() {
                        if let Some(auth) = &self.discord_auth {
                            auth.disconnect();
                        }
                        self.discord.require_account(None);
                        self.discord.set(None);
                    }
                }
                AuthStatus::Disconnected | AuthStatus::Error(_) => {
                    card_title(
                        ui,
                        Icon::Discord,
                        "Подключи Discord",
                        "Название игры и обложка в твоем профиле",
                    );
                    if let AuthStatus::Error(message) = &auth_status {
                        ui.colored_label(Color32::from_rgb(232, 157, 149), message);
                    }
                    if ui
                        .add_enabled(
                            self.discord_auth.is_some(),
                            egui::Button::new("Подключить Discord")
                                .min_size(egui::vec2(190.0, 38.0)),
                        )
                        .clicked()
                        && let Some(auth) = &self.discord_auth
                    {
                        auth.connect();
                    }
                    if matches!(auth_status, AuthStatus::Error(_))
                        && ui.button("Забыть сохраненный вход").clicked()
                    {
                        if let Some(auth) = &self.discord_auth {
                            auth.disconnect();
                        }
                        self.discord.require_account(None);
                        self.discord.set(None);
                    }
                }
            }
        });
        ui.add_space(16.0);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Сейчас играешь").size(18.0).strong());
            ui.add_space(12.0);
            let game = self.current_game();
            ui.horizontal(|ui| {
                if let Some(cover) = game
                    .as_ref()
                    .and_then(|game| game.cover_url.as_deref())
                    .and_then(|url| self.artwork.image(&ctx, url))
                {
                    ui.add(
                        egui::Image::new(&cover)
                            .fit_to_exact_size(egui::vec2(88.0, 88.0))
                            .corner_radius(12),
                    );
                } else {
                    ui.add(egui::Image::new(&self.brand).fit_to_exact_size(egui::vec2(76.0, 76.0)));
                }
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if self.game_name().is_empty() {
                            "Игра не выбрана"
                        } else {
                            self.game_name()
                        })
                        .size(19.0)
                        .strong(),
                    );
                    theme::subtitle(ui, "Nintendo Switch");
                    if (self.active.is_some() || self.settings.nintendo_enabled)
                        && !self.game_name().is_empty()
                    {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|time| time.as_secs() as i64)
                            .unwrap_or(0);
                        let minutes = now.saturating_sub(self.game_started_at()).max(0) / 60;
                        theme::subtitle(ui, &format!("В игре {minutes} мин · Console Link"));
                    }
                });
            });
            ui.add_space(16.0);
            self.nintendo_controls(ui);
            ui.add_space(8.0);
            theme::toggle(
                ui,
                &mut self.settings.discord_enabled,
                "Показывать игру в Discord",
            );
            let raw = self.discord.status();
            let status = if raw.starts_with("Activity sent") {
                "Название игры и карточка подтверждены Discord."
            } else if raw.starts_with("Activity accepted but") {
                "Discord пока показывает имя приложения. Обнови настольный клиент Discord."
            } else if raw.starts_with("Activity accepted by") {
                "Активность принята. Отображаемое название игры еще не подтверждено."
            } else if !matches!(auth_status, AuthStatus::Connected(_)) {
                "Подключи аккаунт для игрового статуса."
            } else if raw.starts_with("Desktop Discord account differs") {
                "В настольном Discord другой аккаунт."
            } else if !self.settings.discord_enabled {
                "Показ активности выключен."
            } else if self.settings.nintendo_enabled && self.game_name().is_empty() {
                "Ожидаем текущую игру из Nintendo."
            } else if self.active.is_none() && !self.settings.nintendo_enabled {
                "Статус появится при подключении консоли."
            } else {
                "Ожидаем настольный Discord. Название игры можно выбрать ниже."
            };
            theme::subtitle(ui, status);
            ui.add_space(14.0);
            theme::subtitle(
                ui,
                "Выбери игру вручную или введи свое название. Ручной выбор отключает автоматический статус Nintendo.",
            );
            ui.collapsing(
                "Выбрать или исправить название",
                |ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.game_query)
                                .hint_text("Поиск по каталогу Switch")
                                .char_limit(128)
                                .desired_width(ui.available_width()),
                        )
                        .changed()
                    {
                        self.game_results = if self.game_query.chars().count() >= 2 {
                            catalog::search_games(&self.game_query, 6)
                        } else {
                            Vec::new()
                        };
                    }
                    for game in self.game_results.clone() {
                        if ui.button(&game.name).clicked() {
                            self.choose_game(game.name, &ctx);
                        }
                    }
                    if !self.game_query.trim().is_empty()
                        && ui.button("Использовать введенное название").clicked()
                    {
                        self.choose_game(self.game_query.trim().to_owned(), &ctx);
                    }
                },
            );
        });
    }

    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::section_heading(ui, "Настройки", "Все важное уже под рукой.");
        self.capture_settings(ui);
        ui.add_space(18.0);
        self.window_settings(ui, ctx);
        ui.add_space(18.0);
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Скриншоты").size(17.0).strong());
            ui.add_space(7.0);
            theme::subtitle(
                ui,
                "Снимки сохраняются в PNG, без рамки окна и элементов управления.",
            );
            ui.add_space(16.0);
            theme::label(ui, "ПАПКА ДЛЯ СОХРАНЕНИЯ");
            let mut directory = self.settings.screenshot_dir.to_string_lossy().into_owned();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut directory)
                        .desired_width(ui.available_width())
                        .margin(egui::vec2(12.0, 10.0)),
                )
                .changed()
            {
                self.settings.screenshot_dir = directory.into();
            }
        });
        ui.add_space(18.0);
        self.about_card(ui, ctx);
    }

    pub(super) fn capture_settings(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Качество изображения").size(17.0).strong());
            ui.add_space(10.0);
            let locked = self.active.is_some() || self.opening;
            ui.add_enabled_ui(!locked, |ui| {
                theme::toggle(ui, &mut self.settings.capture_preferences.automatic, "Автоматическое разрешение");
                ui.add_space(8.0); theme::label(ui, "ЦЕЛЕВОЙ FPS");
                egui::ComboBox::from_id_salt("preferred-fps").selected_text(format!("{} FPS", self.settings.capture_preferences.preferred_fps)).width(ui.available_width()).show_ui(ui, |ui| {
                    for fps in [60, 50, 30, 25, 24] { ui.selectable_value(&mut self.settings.capture_preferences.preferred_fps, fps, format!("{fps} FPS")); }
                });
                if !self.settings.capture_preferences.automatic {
                    if let Some(device) = self.devices.video.iter().find(|d| d.id == self.settings.video_id) {
                        egui::ComboBox::from_id_salt("manual-mode").selected_text(self.settings.mode.as_ref().map(ToString::to_string).unwrap_or_else(|| "Выбери ручной режим".into())).width(ui.available_width()).show_ui(ui, |ui| {
                            for mode in &device.modes { ui.selectable_value(&mut self.settings.mode, Some(mode.clone()), mode.to_string()); }
                        });
                    } else { theme::subtitle(ui, "Сначала выбери карту на странице подключения."); }
                }
            });
            ui.add_space(8.0);
            theme::subtitle(ui, if locked { "Изменить режим захвата можно после завершения сеанса." } else { "По умолчанию Full HD (1920 × 1080), затем ближайший доступный FPS. Если Full HD нет, выберем доступный режим карты." });
        });
        self.refresh_capture_mode();
    }

    pub(super) fn window_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Окно игры").size(17.0).strong());
            ui.add_space(10.0);
            let before = self.settings.game_window_mode;
            egui::ComboBox::from_id_salt("game-window-mode").selected_text(playback::mode_label(before)).width(ui.available_width()).show_ui(ui, |ui| {
                for mode in [GameWindowMode::Windowed, GameWindowMode::Borderless, GameWindowMode::Fullscreen] { ui.selectable_value(&mut self.settings.game_window_mode, mode, playback::mode_label(mode)); }
            });
            if before != self.settings.game_window_mode && self.playback.playing { self.playback.set_mode(self.settings.game_window_mode); self.apply_window_mode(ctx); }
            theme::subtitle(ui, "Безрамочный полный экран занимает текущий монитор и остается обычным окном рабочего стола.");
            ui.add_space(14.0); ui.strong("Горячие клавиши");
            for (key, action) in shortcuts::HELP { ui.horizontal_wrapped(|ui| { ui.label(RichText::new(*key).strong()); theme::label(ui, action); }); }
            theme::subtitle(ui, "Клавиши работают, когда окно приложения в фокусе. M и +/- - только при закрытом меню.");
        });
    }
}
