use eframe::egui::{self, Color32, FontId, RichText, Stroke, StrokeKind, Vec2};

pub const BACKGROUND: Color32 = Color32::from_rgb(23, 24, 27);
pub const SIDEBAR: Color32 = Color32::from_rgb(17, 18, 21);
pub const SURFACE: Color32 = Color32::from_rgb(30, 32, 36);
pub const BORDER: Color32 = Color32::from_rgb(49, 52, 58);
pub const TEXT: Color32 = Color32::from_rgb(239, 240, 242);
pub const MUTED: Color32 = Color32::from_rgb(145, 150, 161);
pub const SUBTLE: Color32 = Color32::from_rgb(100, 105, 115);

pub fn install(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/InterVariable.ttf"
        )))
        .into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .expect("proportional family")
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(25.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.5));
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(14.0, 9.0);
    style.spacing.interact_size = Vec2::new(40.0, 36.0);
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(23, 25, 29);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.selection.bg_fill = Color32::from_gray(74);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.hyperlink_color = TEXT;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.bg_stroke = Stroke::new(1.0_f32, BORDER);
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.fg_stroke = Stroke::new(1.4_f32, TEXT);
    }
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(37, 40, 45);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 51, 57);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(60, 63, 70);
    style.visuals.slider_trailing_fill = true;
    style.visuals.window_corner_radius = egui::CornerRadius::same(14);
    ctx.set_style(style);
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(14)
        .inner_margin(20)
}

pub fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED));
}
pub fn subtitle(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(13.0).color(MUTED));
}

pub fn section_heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(29.0).strong().color(TEXT));
    subtitle(ui, description);
    ui.add_space(16.0);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Session,
    Audio,
    Color,
    Discord,
    Settings,
    Refresh,
    Camera,
    Play,
    Stop,
    Plug,
}

pub fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.6_f32, color);
    let point = |x: f32, y: f32| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            stroke,
        ))
    };
    match icon {
        Icon::Session => {
            painter.rect_stroke(
                egui::Rect::from_min_max(point(0.08, 0.15), point(0.92, 0.73)),
                3,
                stroke,
                StrokeKind::Inside,
            );
            line(&[(0.5, 0.73), (0.5, 0.91)]);
            line(&[(0.3, 0.91), (0.7, 0.91)]);
        }
        Icon::Audio => {
            line(&[
                (0.09, 0.38),
                (0.29, 0.38),
                (0.52, 0.18),
                (0.52, 0.82),
                (0.29, 0.62),
                (0.09, 0.62),
                (0.09, 0.38),
            ]);
            line(&[(0.69, 0.32), (0.79, 0.43), (0.79, 0.57), (0.69, 0.68)]);
            line(&[(0.83, 0.19), (0.95, 0.36), (0.95, 0.64), (0.83, 0.81)]);
        }
        Icon::Color => {
            painter.rect_stroke(
                egui::Rect::from_min_max(point(0.08, 0.12), point(0.92, 0.88)),
                3,
                stroke,
                StrokeKind::Inside,
            );
            line(&[
                (0.10, 0.76),
                (0.37, 0.45),
                (0.58, 0.66),
                (0.70, 0.50),
                (0.91, 0.76),
            ]);
            painter.circle_stroke(point(0.69, 0.31), rect.width() * 0.08, stroke);
        }
        Icon::Discord => {
            line(&[
                (0.18, 0.27),
                (0.34, 0.21),
                (0.39, 0.3),
                (0.61, 0.3),
                (0.66, 0.21),
                (0.82, 0.27),
                (0.94, 0.73),
                (0.71, 0.82),
                (0.65, 0.72),
                (0.35, 0.72),
                (0.29, 0.82),
                (0.06, 0.73),
                (0.18, 0.27),
            ]);
            painter.circle_filled(point(0.36, 0.51), rect.width() * 0.055, color);
            painter.circle_filled(point(0.64, 0.51), rect.width() * 0.055, color);
        }
        Icon::Settings => {
            for (x, y) in [(0.23, 0.35), (0.5, 0.65), (0.77, 0.45)] {
                line(&[(x, 0.12), (x, 0.88)]);
                painter.circle_filled(point(x, y), rect.width() * 0.11, SIDEBAR);
                painter.circle_stroke(point(x, y), rect.width() * 0.11, stroke);
            }
        }
        Icon::Refresh => {
            line(&[
                (0.82, 0.42),
                (0.74, 0.24),
                (0.47, 0.15),
                (0.24, 0.27),
                (0.13, 0.49),
                (0.22, 0.73),
                (0.48, 0.85),
                (0.73, 0.75),
            ]);
            line(&[(0.64, 0.42), (0.84, 0.43), (0.86, 0.22)]);
        }
        Icon::Camera => {
            line(&[
                (0.1, 0.31),
                (0.29, 0.31),
                (0.37, 0.16),
                (0.64, 0.16),
                (0.72, 0.31),
                (0.9, 0.31),
                (0.9, 0.83),
                (0.1, 0.83),
                (0.1, 0.31),
            ]);
            painter.circle_stroke(point(0.5, 0.55), rect.width() * 0.19, stroke);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![point(0.27, 0.14), point(0.84, 0.5), point(0.27, 0.86)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                egui::Rect::from_min_max(point(0.22, 0.22), point(0.78, 0.78)),
                2,
                color,
            );
        }
        Icon::Plug => {
            line(&[(0.27, 0.12), (0.27, 0.35), (0.73, 0.35), (0.73, 0.12)]);
            line(&[
                (0.19, 0.35),
                (0.19, 0.64),
                (0.37, 0.8),
                (0.63, 0.8),
                (0.81, 0.64),
                (0.81, 0.35),
            ]);
            line(&[(0.5, 0.8), (0.5, 0.99)]);
        }
    }
}

pub fn icon(ui: &mut egui::Ui, kind: Icon, size: f32, color: Color32) {
    let (_, rect) = ui.allocate_space(egui::vec2(size, size));
    draw_icon(ui.painter(), rect, kind, color);
}

pub fn nav(
    ui: &mut egui::Ui,
    kind: Icon,
    text: &str,
    selected: bool,
    compact: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            9,
            if selected {
                Color32::from_rgb(39, 42, 48)
            } else {
                Color32::from_rgb(27, 29, 34)
            },
        );
    }
    let color = if selected { TEXT } else { MUTED };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 22.0, rect.center().y),
        egui::vec2(18.0, 18.0),
    );
    draw_icon(ui.painter(), icon_rect, kind, color);
    if !compact {
        ui.painter().text(
            egui::pos2(rect.left() + 46.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            FontId::proportional(13.0),
            color,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            text,
        )
    });
    response.on_hover_text(text)
}

pub fn action(
    ui: &mut egui::Ui,
    kind: Icon,
    text: &str,
    primary: bool,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 42.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    let fill = if primary {
        if enabled {
            if response.hovered() {
                Color32::WHITE
            } else {
                Color32::from_gray(229)
            }
        } else {
            Color32::from_gray(71)
        }
    } else if response.hovered() {
        Color32::from_rgb(48, 51, 57)
    } else {
        SURFACE
    };
    ui.painter().rect_filled(rect, 9, fill);
    if !primary {
        ui.painter()
            .rect_stroke(rect, 9, Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
    }
    let color = if primary {
        if enabled {
            Color32::from_gray(20)
        } else {
            Color32::from_gray(143)
        }
    } else if enabled {
        TEXT
    } else {
        SUBTLE
    };
    draw_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 23.0, rect.center().y),
            egui::vec2(15.0, 15.0),
        ),
        kind,
        color,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 42.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        color,
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), text));
    response
}

pub fn status(ui: &mut egui::Ui, label: &str, active: bool) {
    let color = if active { TEXT } else { MUTED };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(11.5), color);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 37.0, 30.0),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 16, Color32::from_rgb(35, 38, 43));
    ui.painter()
        .rect_stroke(rect, 16, Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
    ui.painter().circle_filled(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        2.5,
        if active { TEXT } else { SUBTLE },
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 23.0, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

pub fn toggle(ui: &mut egui::Ui, value: &mut bool, text: &str) -> egui::Response {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let track = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::vec2(36.0, 20.0),
    );
    ui.painter()
        .rect_filled(track, 10, if *value { TEXT } else { BORDER });
    let center = egui::pos2(
        if *value {
            track.right() - 10.0
        } else {
            track.left() + 10.0
        },
        track.center().y,
    );
    ui.painter()
        .circle_filled(center, 7.0, if *value { BACKGROUND } else { MUTED });
    ui.painter().text(
        egui::pos2(rect.left() + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        TEXT,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, text)
    });
    response
}
