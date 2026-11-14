use super::*;

impl Desktop {
    pub(super) fn settings_page(&self, cx: &Context<Self>) -> AnyElement {
        let locked = self.runtime.active() || self.runtime.opening;
        let mut capture = card(
            "Изображение с карты",
            "По умолчанию Full HD и частота до 60 FPS.",
        );
        if locked {
            capture = capture.child(hint(
                "Изменить режим захвата можно после завершения сеанса.",
            ));
        } else {
            capture = capture.child(switch(
                cx,
                "automatic-capture",
                "Автоматическое разрешение",
                self.runtime.settings.capture_preferences.automatic,
                |s, v| s.runtime.settings.capture_preferences.automatic = v,
            ));
            let preferred = self.runtime.settings.capture_preferences.preferred_fps;
            let rates = self.select_box(
                Selection::FrameRate,
                "target-fps",
                format!("{preferred} FPS"),
                [120, 100, 90, 75, 60, 50, 30, 25, 24]
                    .into_iter()
                    .map(|fps| (fps.to_string(), format!("{fps} FPS"), fps == preferred))
                    .collect(),
                cx,
                |s, value, _, _| {
                    if let Ok(fps) = value.parse::<u32>()
                        && [120, 100, 90, 75, 60, 50, 30, 25, 24].contains(&fps)
                    {
                        s.runtime.settings.capture_preferences.preferred_fps = fps;
                        s.runtime.changed();
                    }
                },
            );
            capture = capture.child(setting_control("Целевая частота", rates));
            if !self.runtime.settings.capture_preferences.automatic {
                capture =
                    capture.child(self.device_picker(Selection::Resolution, "Разрешение", cx));
            }
        }
        capture=capture.child(hint("Если Full HD или нужный FPS недоступны, используем поддерживаемый режим карты. Частота карты не подменяется выбранным числом."));
        let mut modes = div().flex().flex_wrap().gap_2();
        for (i, (mode, label)) in [
            (GameWindowMode::Windowed, "Оконный"),
            (GameWindowMode::Borderless, "Без границ"),
            (GameWindowMode::Fullscreen, "Полный экран"),
        ]
        .into_iter()
        .enumerate()
        {
            modes = modes.child(widgets::button(
                ("window-mode", i),
                label,
                self.runtime.settings.game_window_mode == mode,
                cx.listener(move |s, _, window, cx| s.set_mode(mode, window, cx)),
            ));
        }
        let mut keys = column().gap_1();
        for (key, description) in [
            ("F1", "Боковое меню"),
            ("F11 / Alt+Enter / ⌘Enter", "Полный экран"),
            ("F10", "Сменить режим окна"),
            ("F8 / Ctrl+Shift+S / ⌘⇧S", "Скриншот"),
            ("M", "Включить или выключить звук"),
            ("+ / -", "Громкость на 5%"),
            ("Esc", "Закрыть меню / выйти из полного экрана"),
            ("Ctrl+Shift+Enter / ⌘⇧Enter", "Завершить сеанс"),
        ] {
            keys = keys.child(widgets::row(key, description));
        }
        let window=card("Окно игры","Без границ занимает текущий монитор и остается обычным окном рабочего стола.").child(modes).child(keys)
            .child(hint("Клавиши работают в активном окне. M и +/- не мешают вводу текста. Курсор скрывается через 4 секунды без движения."));
        let screenshot = card("Скриншоты", "PNG без рамки окна и элементов управления.")
            .child(caption("Папка для сохранения"))
            .child(self.path_input.clone())
            .child(action_button(
                "screenshot-now",
                "Сделать скриншот",
                false,
                cx.listener(|s, _, window, cx| s.screenshot(window, cx)),
            ));
        column()
            .w_full()
            .gap_6()
            .child(capture)
            .child(window)
            .child(screenshot)
            .child(self.monitor_card(cx))
            .into_any_element()
    }

    pub(super) fn monitor_card(&self, cx: &Context<Self>) -> AnyElement {
        let elapsed = self.runtime.session_elapsed().as_secs();
        let audio = self
            .runtime
            .audio_counters()
            .map(|(under, over)| format!("{under} / {over}"))
            .unwrap_or_else(|| "Не подключено".into());
        let mut about=card("Об приложении",&format!("Console Link {} · Rust · GPUI · macOS и Windows",env!("CARGO_PKG_VERSION")))
            .child(widgets::row("Сеанс",format!("{:02}:{:02}:{:02}",elapsed/3600,elapsed/60%60,elapsed%60)))
            .child(widgets::row("Кадры, принятые интерфейсом/с",format!("{:.1}",self.runtime.fps)))
            .child(widgets::row("Память истории видео",format!("{:.1} МБ",self.runtime.history_bytes() as f64/1048576.)))
            .child(widgets::row("Пропущенные кадры",self.runtime.dropped_frames().to_string()))
            .child(widgets::row("Аудиобуфер: паузы / переполнения",audio))
            .child(widgets::row("Синхронизация",format!("{:+} мс",self.runtime.settings.sync_ms)))
            .child(hint("История за 60 секунд, шаг 0,5 с. Это счетчики приложения, не полная задержка от HDMI до экрана."));
        for (audio, title) in [
            (false, "Кадры интерфейса · FPS"),
            (true, "Пик выходного звука · %"),
        ] {
            let maximum = if audio {
                100.
            } else {
                self.runtime
                    .monitor
                    .samples
                    .iter()
                    .map(|s| s.fps)
                    .fold(60., f32::max)
            };
            let mut bars = div()
                .flex()
                .items_end()
                .w_full()
                .h(px(64.))
                .flex_shrink_0()
                .gap(px(1.))
                .rounded_md()
                .bg(rgb(theme::BACKGROUND))
                .px_2()
                .py_2();
            for sample in &self.runtime.monitor.samples {
                let value = if audio {
                    sample.peak * 100.
                } else {
                    sample.fps
                };
                bars = bars.child(
                    div()
                        .flex_1()
                        .min_w(px(1.))
                        .h(relative((value / maximum).clamp(0., 1.)))
                        .rounded_sm()
                        .bg(rgb(theme::MUTED)),
                );
            }
            about = about.child(caption(title)).child(bars);
        }
        about
            .child(action_button(
                "repository",
                "Исходный код и загрузки",
                false,
                cx.listener(|_, _, _, _| {
                    let _ = webbrowser::open("https://github.com/Potanull/console-link");
                }),
            ))
            .into_any_element()
    }
}
