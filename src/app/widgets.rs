use super::*;

pub(super) fn audio_combo(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    selected: &mut String,
    devices: &[AudioDeviceInfo],
) {
    theme::label(ui, label);
    let name = devices
        .iter()
        .find(|d| d.id == *selected)
        .map(|d| d.name.as_str())
        .unwrap_or(if selected.is_empty() {
            "Не выбран"
        } else {
            "Устройство недоступно"
        });
    egui::ComboBox::from_id_salt(id)
        .truncate()
        .selected_text(name)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, String::new(), "Без звука / не выбран");
            for device in devices {
                ui.selectable_value(selected, device.id.clone(), &device.name);
            }
        });
}

pub(super) fn card_title(ui: &mut egui::Ui, icon: Icon, title: &str, description: &str) {
    ui.horizontal(|ui| {
        theme::icon(ui, icon, 19.0, theme::TEXT);
        ui.add_space(3.0);
        ui.label(RichText::new(title).size(16.0).strong());
    });
    theme::subtitle(ui, description);
    ui.add_space(8.0);
}

pub(super) fn message(ui: &mut egui::Ui, title: &str, text: &str, error: bool) {
    egui::Frame::new()
        .fill(if error {
            Color32::from_rgb(47, 32, 34)
        } else {
            theme::SURFACE
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if error {
                Color32::from_rgb(94, 57, 61)
            } else {
                theme::BORDER
            },
        ))
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(13.0).strong());
            ui.add_space(4.0);
            theme::subtitle(ui, text);
        });
}

pub(super) fn level_meter(ui: &mut egui::Ui, peak: f32) {
    theme::label(ui, "УРОВЕНЬ СИГНАЛА");
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::hover());
    let cells = 40;
    let gap = 3.0;
    let width = (rect.width() - gap * (cells - 1) as f32) / cells as f32;
    for n in 0..cells {
        let active = (n as f32) / (cells as f32) < peak.clamp(0.0, 1.0);
        let r = egui::Rect::from_min_size(
            rect.min + egui::vec2(n as f32 * (width + gap), 0.0),
            egui::vec2(width, rect.height()),
        );
        ui.painter().rect_filled(
            r,
            2,
            if active {
                theme::TEXT
            } else {
                Color32::from_rgb(49, 52, 58)
            },
        );
    }
}
