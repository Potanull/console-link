use super::*;

impl ConsoleApp {
    pub(super) fn about_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Об приложении").size(17.0).strong());
            theme::subtitle(ui, &format!("Console Link {} · Rust · macOS и Windows", env!("CARGO_PKG_VERSION")));
            ui.hyperlink_to("Исходный код и загрузки ↗", config::REPOSITORY_URL);
            ui.add_space(14.0);
            ui.label(RichText::new("Мониторинг подключения").size(16.0).strong());
            let running = self.active.is_some();
            let audio = self.active.as_ref().and_then(|a| a.audio.as_ref());
            let seconds = self.session_started.map(|time| time.elapsed().as_secs()).unwrap_or(0);
            ui.horizontal_wrapped(|ui| {
                ui.label(if running { "● Консоль подключена" } else { "○ Нет активного сигнала" });
                if running { ui.label(format!("Сессия {:02}:{:02}:{:02}", seconds/3600, seconds/60%60, seconds%60)); }
            });
            let dropped = self.capture_dropped.load(Ordering::Relaxed) + self.history.dropped().saturating_sub(self.history_dropped_start);
            let underruns = audio.map(AudioEngine::underruns).unwrap_or(0);
            let overruns = audio.map(AudioEngine::overruns).unwrap_or(0);
            egui::Grid::new("connection-metrics").num_columns(2).spacing([24.0,8.0]).show(ui, |ui| {
                ui.label("Захват"); ui.label(if running { self.settings.mode.as_ref().map(ToString::to_string).unwrap_or_else(|| "—".into()) } else { "—".into() }); ui.end_row();
                ui.label("Кадры, принятые интерфейсом/с"); ui.label(if running {format!("{:.1}",self.fps)} else {"—".into()}); ui.end_row();
                ui.label("Память истории видео"); ui.label(format!("{:.1} МБ",self.history.bytes() as f64/1_048_576.0)); ui.end_row();
                ui.label("Пропущенные кадры"); ui.label(format!("{dropped}")); ui.end_row();
                ui.label("Аудиобуфер: паузы / переполнения"); ui.label(if audio.is_some() {format!("{underruns} / {overruns}")} else {"Аудио не подключено".into()}); ui.end_row();
                ui.label("Коррекция синхронизации"); ui.label(format!("{:+} мс",self.settings.sync_ms)); ui.end_row();
            });
            ui.add_space(12.0);
            plot(ui, "Кадры интерфейса · FPS", &self.monitor, false, self.settings.mode.as_ref().map(nominal_fps).unwrap_or(60) as f32, running);
            ui.add_space(10.0);
            plot(ui, "Уровень выходного звука · %", &self.monitor, true, 100.0, audio.is_some());
            ui.add_space(8.0);
            theme::subtitle(ui, "Последние 60 секунд · шаг 0,5 с. FPS обновляется раз в секунду. Уровень звука — пик последнего аудиобуфера.");
            theme::subtitle(ui, "FPS показывает кадры, принятые интерфейсом, а не частоту сигнала HDMI. Полная задержка карты здесь не измеряется.");
            if ui.add_enabled(!running && !self.opening && !self.refreshing, egui::Button::new("Обновить устройства")).clicked() { self.refresh(ctx.clone()); }
        });
    }
}

fn plot(
    ui: &mut egui::Ui,
    title: &str,
    monitor: &console_link::monitoring::Monitor,
    audio: bool,
    max: f32,
    available: bool,
) {
    ui.label(title);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 105.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 6, theme::BACKGROUND);
    let chart = rect.shrink2(egui::vec2(32.0, 12.0));
    let max = max.max(1.0).max(if audio {
        100.0
    } else {
        monitor.samples.iter().map(|s| s.fps).fold(0.0, f32::max)
    });
    for (fraction, label) in [(0.0, max), (0.5, max / 2.0), (1.0, 0.0)] {
        let y = chart.top() + chart.height() * fraction;
        ui.painter().line_segment(
            [egui::pos2(chart.left(), y), egui::pos2(chart.right(), y)],
            egui::Stroke::new(1.0_f32, theme::BORDER),
        );
        ui.painter().text(
            egui::pos2(rect.left() + 4.0, y),
            egui::Align2::LEFT_CENTER,
            format!("{label:.0}"),
            egui::FontId::proportional(10.0),
            theme::MUTED,
        );
    }
    if !available || monitor.samples.is_empty() {
        ui.painter().text(
            chart.center(),
            egui::Align2::CENTER_CENTER,
            if audio {
                "Подключи аудио для графика"
            } else {
                "Подключи консоль для графика"
            },
            egui::FontId::proportional(12.0),
            theme::MUTED,
        );
        return;
    }
    let end = monitor.samples.back().unwrap().elapsed.max(60.0);
    let points = monitor
        .samples
        .iter()
        .map(|s| {
            let value = if audio { s.peak * 100.0 } else { s.fps };
            egui::pos2(
                chart.right() - ((end - s.elapsed) / 60.0) as f32 * chart.width(),
                chart.bottom() - (value / max).clamp(0.0, 1.0) * chart.height(),
            )
        })
        .collect::<Vec<_>>();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(130, 190, 230)),
    ));
    if let Some(pointer) = response.hover_pos() {
        let at = end
            - f64::from((chart.right() - pointer.x).clamp(0.0, chart.width()) / chart.width())
                * 60.0;
        if let Some(sample) = monitor
            .samples
            .iter()
            .min_by(|a, b| (a.elapsed - at).abs().total_cmp(&(b.elapsed - at).abs()))
        {
            response.on_hover_text(format!(
                "{:.1} с с начала сессии · {:.1} {}",
                sample.elapsed,
                if audio {
                    sample.peak * 100.0
                } else {
                    sample.fps
                },
                if audio { "%" } else { "FPS" }
            ));
        }
    }
}
