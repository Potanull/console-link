use super::*;

impl ConsoleApp {
    pub(super) fn synchronize_presentation(&mut self, ctx: &egui::Context) {
        let wants_game = self.active.is_some() || self.snapshot_game;
        if wants_game && !self.playback.playing {
            self.menu_geometry = WindowGeometry::current(ctx);
            self.game_geometry.get_or_insert(WindowGeometry {
                position: self
                    .menu_geometry
                    .map(|g| g.position)
                    .unwrap_or(egui::pos2(80.0, 80.0)),
                size: if self.snapshot_game {
                    self.menu_geometry
                        .map(|geometry| geometry.size)
                        .unwrap_or(egui::vec2(config::WINDOW_SIZE[0], config::WINDOW_SIZE[1]))
                } else {
                    egui::vec2(960.0, 540.0)
                },
            });
            self.playback.begin(self.settings.game_window_mode);
            if self.snapshot_game
                && matches!(
                    argument_value("--ui-page").as_deref(),
                    Some("overlay" | "overlay-color")
                )
            {
                self.playback.overlay_open = true;
            }
            self.native_mode = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                if self.settings.game.is_empty() {
                    "Console Link - Game".into()
                } else {
                    format!("{} - Console Link", self.settings.game)
                },
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                480.0, 270.0,
            )));
            self.apply_window_mode(ctx);
        } else if !wants_game && self.playback.playing {
            self.playback.finish();
            self.pending_borderless = None;
            self.page = Page::Session;
            self.native_mode = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("Console Link".into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                720.0, 600.0,
            )));
            self.pending_geometry = self.menu_geometry;
        }
        if self.pending_geometry.is_some()
            && !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
        {
            let geometry = self
                .pending_geometry
                .take()
                .expect("checked pending geometry");
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(geometry.position));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(geometry.size));
        }
    }

    pub(super) fn apply_window_mode(&mut self, ctx: &egui::Context) {
        let mode = self.playback.mode;
        if self.native_mode == Some(mode) {
            return;
        }
        if self.native_mode == Some(GameWindowMode::Windowed) {
            self.game_geometry = WindowGeometry::current(ctx).or(self.game_geometry);
        }
        self.pending_borderless = (mode == GameWindowMode::Borderless).then(Instant::now);
        self.pending_geometry = None;
        if mode == GameWindowMode::Fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                mode == GameWindowMode::Windowed,
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            if mode == GameWindowMode::Windowed {
                self.pending_geometry = self.game_geometry;
            }
        }
        self.native_mode = Some(mode);
        self.settings.game_window_mode = mode;
        self.dirty = Some(Instant::now());
    }

    pub(super) fn game_shortcuts(&mut self, ctx: &egui::Context) {
        for action in shortcuts::pressed(ctx) {
            if self.playback.overlay_open
                && matches!(action, Action::Mute | Action::Louder | Action::Quieter)
            {
                continue;
            }
            match action {
                Action::Overlay => self.playback.toggle_overlay(),
                Action::Escape => {
                    if self.playback.escape() {
                        self.apply_window_mode(ctx);
                    }
                }
                Action::Fullscreen => {
                    self.playback.toggle_fullscreen();
                    self.apply_window_mode(ctx);
                }
                Action::CycleMode => {
                    self.playback.cycle_mode();
                    self.apply_window_mode(ctx);
                }
                Action::Screenshot => self.screenshot(ctx.clone()),
                Action::Stop => {
                    self.stop();
                    break;
                }
                Action::Mute => {
                    self.settings.muted = !self.settings.muted;
                    self.sync_audio_controls();
                }
                Action::Louder => {
                    self.settings.gain = (self.settings.gain + 0.05).min(1.0);
                    self.sync_audio_controls();
                }
                Action::Quieter => {
                    self.settings.gain = (self.settings.gain - 0.05).max(0.0);
                    self.sync_audio_controls();
                }
            }
        }
    }

    pub(super) fn sync_audio_controls(&mut self) {
        if let Some(audio) = self
            .active
            .as_ref()
            .and_then(|active| active.audio.as_ref())
        {
            audio.set_gain(self.settings.gain);
            audio.set_muted(self.settings.muted);
        }
        self.dirty = Some(Instant::now());
    }

    pub(super) fn volume_feedback(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("volume-feedback"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -60.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(16)
                    .inner_margin(18)
                    .show(ui, |ui| {
                        ui.set_width(230.0);
                        ui.horizontal(|ui| {
                            theme::icon(ui, Icon::Audio, 20.0, theme::TEXT);
                            ui.label(
                                RichText::new(if self.settings.muted {
                                    "Звук выключен".into()
                                } else {
                                    format!("Громкость  {:.0}%", self.settings.gain * 100.0)
                                })
                                .size(17.0)
                                .strong(),
                            );
                        });
                        ui.add_space(10.0);
                        let (track, _) =
                            ui.allocate_exact_size(egui::vec2(230.0, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(track, 3, theme::BORDER);
                        let gain = if self.settings.muted {
                            0.0
                        } else {
                            self.settings.gain
                        };
                        let end = track.left() + track.width() * gain;
                        if gain > 0.0 {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(
                                    track.min,
                                    egui::pos2(end, track.bottom()),
                                ),
                                3,
                                theme::TEXT,
                            );
                        }
                        ui.painter().circle_filled(
                            egui::pos2(end, track.center().y),
                            5.0,
                            theme::TEXT,
                        );
                    });
            });
    }

    pub(super) fn game_scene(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                if self.color_gpu_available {
                    if let Some(frame) = &self.displayed {
                        let available = ui.available_size();
                        let original = egui::vec2(frame.width as f32, frame.height as f32);
                        let size =
                            original * (available.x / original.x).min(available.y / original.y);
                        let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
                        let effective = color_integration::effective_color(
                            self.settings.color,
                            self.color_original,
                            true,
                        );
                        self.presented_frame = Some(frame.clone());
                        self.presented_color = effective;
                        color_gpu::paint(ui, rect, frame.clone(), effective, false);
                    }
                } else if let Some(texture) = &self.texture {
                    let available = ui.available_size();
                    let original = texture.size_vec2();
                    let size = original * (available.x / original.x).min(available.y / original.y);
                    let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
                    ui.put(rect, egui::Image::new(texture).fit_to_exact_size(size));
                    self.presented_frame = self.displayed.clone();
                    self.presented_color = console_link::color::ColorSettings::default();
                }
            });
        let progress = ctx.animate_bool_with_time(
            egui::Id::new("game-drawer-animation"),
            self.playback.overlay_open,
            0.18,
        );
        if progress > 0.0 {
            let before = self.settings.clone();
            let bounds = ctx.content_rect();
            let drawer = drawer_rect(bounds);
            let width = drawer.width();
            let position = drawer.min + egui::vec2(width * (1.0 - progress), 0.0);
            egui::Area::new(egui::Id::new("game-settings-drawer"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .movable(false)
                .interactable(self.playback.overlay_open)
                .constrain(false)
                .show(ctx, |ui| {
                    drawer_frame().show(ui, |ui| {
                        let content_size = drawer_content_size(drawer);
                        ui.set_min_size(content_size);
                        ui.set_max_size(content_size);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Console Link").size(18.0).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Закрыть · F1").clicked() {
                                        self.playback.overlay_open = false;
                                    }
                                },
                            );
                        });
                        ui.add_space(10.0);
                        drawer_tabs(ui, &mut self.page);
                        ui.add_space(10.0);
                        ui.spacing_mut().scroll.floating = false;
                        ui.spacing_mut().scroll.bar_width = 3.0;
                        let scroll_height = drawer_scroll_height(ui);
                        egui::ScrollArea::vertical()
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                            )
                            .id_salt(("drawer-scroll", self.page as u8))
                            .max_width(content_size.x)
                            .min_scrolled_height(scroll_height)
                            .max_height(scroll_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                match self.page {
                                    Page::Session => {
                                        ui.label(RichText::new("Сеанс игры").size(21.0).strong());
                                        if let Some(mode) = &self.settings.mode {
                                            theme::subtitle(ui, &mode.to_string());
                                        }
                                        self.window_settings(ui, ctx);
                                    }
                                    Page::Audio => self.audio_page(ui),
                                    Page::Color => self.color_page(ui),
                                    Page::Discord => self.discord_page(ui),
                                    Page::Settings => self.settings_page(ui, ctx),
                                }
                                if let Some(error) = &self.error {
                                    theme::subtitle(ui, error);
                                }
                            });
                        ui.add_space(10.0);
                        if theme::action(
                            ui,
                            Icon::Stop,
                            "Завершить сеанс",
                            false,
                            ui.available_width(),
                        )
                        .clicked()
                        {
                            self.stop();
                        }
                    });
                });
            if before != self.settings {
                self.dirty = Some(Instant::now());
            }
        } else if self.started.elapsed() < Duration::from_secs(5) || self.snapshot_game {
            egui::Area::new(egui::Id::new("game-hotkey-hint"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(190))
                        .corner_radius(9)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("F1  меню     F11  полный экран     F8  скриншот")
                                    .size(12.0)
                                    .color(theme::TEXT),
                            );
                        });
                });
        }
    }
}

fn drawer_tabs(ui: &mut egui::Ui, page: &mut Page) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.horizontal_wrapped(|ui| {
            for (tab, label) in [
                (Page::Session, "Сеанс"),
                (Page::Audio, "Звук"),
                (Page::Color, "Изображение"),
                (Page::Discord, "Discord"),
                (Page::Settings, "Настройки"),
            ] {
                if ui
                    .selectable_label(*page == tab, RichText::new(label).size(12.0))
                    .clicked()
                {
                    *page = tab;
                }
            }
        });
    });
}

fn drawer_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::BACKGROUND)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(16)
        .inner_margin(18)
}
fn drawer_content_size(drawer: egui::Rect) -> egui::Vec2 {
    (drawer.size() - drawer_frame().total_margin().sum()).max(egui::vec2(1.0, 1.0))
}
fn drawer_scroll_height(ui: &egui::Ui) -> f32 {
    (ui.available_height() - ui.spacing().item_spacing.y - 10.0 - 42.0).max(1.0)
}

fn drawer_rect(bounds: egui::Rect) -> egui::Rect {
    let margin = 12.0;
    let available = bounds.shrink(margin);
    let width = (bounds.width() * 0.5)
        .clamp(420.0, 640.0)
        .min(available.width());
    egui::Rect::from_min_max(
        egui::pos2(available.right() - width, available.top()),
        available.max,
    )
}

#[cfg(test)]
mod drawer_regressions {
    use super::*;
    #[test]
    fn drawer_keeps_header_body_and_footer_inside_every_window() {
        for size in [
            [480.0, 270.0],
            [720.0, 600.0],
            [960.0, 540.0],
            [1100.0, 1380.0],
            [1920.0, 1080.0],
        ] {
            let bounds =
                egui::Rect::from_min_size(egui::pos2(-200.0, 35.0), egui::vec2(size[0], size[1]));
            let drawer = drawer_rect(bounds);
            assert!(bounds.contains_rect(drawer), "{size:?}: {drawer:?}");
            assert!(drawer.height() <= bounds.height() - 16.0);
            assert!(drawer.width() <= bounds.width() - 16.0);
        }
    }
    #[test]
    fn rendered_drawer_keeps_the_stop_button_and_frame_inside_viewport() {
        for size in [[480.0, 270.0], [960.0, 540.0], [1100.0, 1380.0]] {
            let ctx = egui::Context::default();
            theme::install(&ctx);
            let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]));
            let drawer = drawer_rect(bounds);
            let mut outer = egui::Rect::NOTHING;
            let mut footer = egui::Rect::NOTHING;
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(bounds),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::Area::new(egui::Id::new("drawer-test"))
                            .fixed_pos(drawer.min)
                            .movable(false)
                            .constrain(false)
                            .show(ctx, |ui| {
                                let response = drawer_frame().show(ui, |ui| {
                                    let content = drawer_content_size(drawer);
                                    ui.set_min_size(content);
                                    ui.set_max_size(content);
                                    ui.horizontal(|ui| {
                                        ui.label("Console Link");
                                        let _ = ui.button("Закрыть · F1");
                                    });
                                    ui.add_space(10.0);
                                    let mut page = Page::Audio;
                                    drawer_tabs(ui, &mut page);
                                    ui.add_space(10.0);
                                    let height = drawer_scroll_height(ui);
                                    egui::ScrollArea::vertical()
                                        .max_width(content.x)
                                        .min_scrolled_height(height)
                                        .max_height(height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_min_height(1500.0);
                                            ui.label("Tall page");
                                        });
                                    ui.add_space(10.0);
                                    footer = theme::action(
                                        ui,
                                        Icon::Stop,
                                        "Завершить сеанс",
                                        false,
                                        ui.available_width(),
                                    )
                                    .rect;
                                });
                                outer = response.response.rect;
                            });
                    },
                );
            }
            assert!(
                bounds.shrink(8.0).contains_rect(outer),
                "{size:?}: outer {outer:?}"
            );
            assert!(
                bounds.shrink(8.0).contains_rect(footer),
                "{size:?}: footer {footer:?}"
            );
        }
    }
}
