use super::*;

impl Desktop {
    pub(super) fn session_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self
            .runtime
            .settings
            .mode
            .as_ref()
            .map(resolution_label)
            .unwrap_or_else(|| "1920 × 1080 · автоматически".into());
        let mut source = card(
            if self.runtime.active() {
                "Консоль подключена"
            } else {
                "Подключить консоль"
            },
            "Подключи консоль к HDMI-входу карты захвата, а карту к компьютеру.",
        )
        .child(self.device_picker(Selection::Video, "Карта захвата", cx))
        .child(self.device_picker(Selection::AudioInput, "Аудиовход карты", cx))
        .child(self.device_picker(Selection::AudioOutput, "Выход звука", cx))
        .child(hint(if self.runtime.active() {
            "Чтобы сменить устройства, заверши сеанс."
        } else {
            "Включи «Захват без звука»: аудиоустройства и доступ к микрофону не нужны."
        }))
        .child(
            column()
                .w_full()
                .gap_1()
                .py_2()
                .child(widgets::row("Разрешение", mode))
                .child(widgets::row(
                    "Целевая частота",
                    format!(
                        "{} FPS",
                        self.runtime.settings.capture_preferences.preferred_fps
                    ),
                ))
                .child(hint("Режим захвата можно изменить в настройках.")),
        );
        if self.runtime.opening {
            source = source
                .child(text("Подключаем консоль…"))
                .child(widgets::button(
                    "cancel-connect",
                    "Отменить",
                    false,
                    cx.listener(|this, _, window, cx| this.stop(window, cx)),
                ));
        } else if self.runtime.active() {
            source = source.child(widgets::button(
                "stop-session",
                "Завершить сеанс",
                false,
                cx.listener(|this, _, window, cx| this.stop(window, cx)),
            ));
        } else {
            if self.runtime.ready() {
                source = source.child(widgets::button(
                    "start-session",
                    "Подключить консоль",
                    true,
                    cx.listener(|this, _, window, cx| this.connect(window, cx)),
                ));
            } else {
                source = source.child(hint(if self.runtime.refreshing {
                    "Ищем карты захвата…"
                } else if self.runtime.settings.video_id.is_empty() {
                    "Выбери карту захвата. Если ее нет в списке, проверь USB-подключение и обнови устройства."
                } else {
                    "Проверь карту и её режим. Для звука выбери вход и выход или включи «Захват без звука»."
                }));
            }
            if !self.runtime.refreshing {
                source = source.child(action_button(
                    "refresh-video",
                    "Обновить устройства",
                    false,
                    cx.listener(|this, _, _, cx| {
                        this.runtime.refresh();
                        cx.notify();
                    }),
                ));
            }
        }
        let mut body = column().w_full().gap_6();
        if !self.runtime.active() && !self.runtime.opening {
            body = body.child(switch(
                cx,
                "video-only",
                "Захват без звука",
                !self.runtime.settings.audio_enabled,
                |s, value| s.runtime.settings.audio_enabled = !value,
            ));
        }
        let mut body = body.child(source);
        if !self.runtime.devices.warnings.is_empty() {
            body = body.child(
                card("Устройства требуют внимания", "")
                    .children(self.runtime.devices.warnings.iter().cloned().map(hint)),
            );
        }
        body.into_any_element()
    }
}
