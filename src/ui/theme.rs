//! The whole visual language in one place: black on white, a few grays,
//! one typeface, generous space. No accent colour — emphasis comes from
//! weight, size and black.

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Pos2,
    Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use std::sync::Arc;

// ── Colour ─────────────────────────────────────────────────────────────────
pub const WHITE:   Color32 = Color32::WHITE;
pub const BLACK:   Color32 = Color32::from_rgb(0x1D, 0x1D, 0x1F); // primary text and actions
pub const GRAY_1:  Color32 = Color32::from_rgb(0x6E, 0x6E, 0x73); // secondary text
pub const GRAY_2:  Color32 = Color32::from_rgb(0xAE, 0xAE, 0xB2); // tertiary text, icons
pub const LINE:    Color32 = Color32::from_rgb(0xE5, 0xE5, 0xEA); // hairlines
pub const SURFACE: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7); // fills: tiles, fields
pub const HOVER:   Color32 = Color32::from_rgb(0xEB, 0xEB, 0xF0); // fills under the pointer
const BLACK_HOVER: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3C);

// ── Space and shape ────────────────────────────────────────────────────────
pub const MARGIN: f32 = 32.0;
pub const RADIUS: u8 = 10;

/// macOS draws its window buttons over our content (seamless title bar).
pub const TITLEBAR: f32 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

// ── Type ───────────────────────────────────────────────────────────────────
const SEMIBOLD: &str = "semibold";

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf"))));
    fonts.font_data.insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"))));
    // Inter first; egui's bundled fonts stay behind it for symbols Inter lacks.
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "Inter".into());
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), [vec!["Inter-SemiBold".into()], fallbacks].concat());
    ctx.set_fonts(fonts);

    // One light style for both themes, so the system appearance never
    // switches us into egui's dark defaults.
    ctx.set_theme(egui::ThemePreference::Light);
    let mut style = egui::Style { visuals: egui::Visuals::light(), ..Default::default() };
    let v = &mut style.visuals;
    v.panel_fill = WHITE;
    v.window_fill = WHITE;
    v.extreme_bg_color = SURFACE; // text field background
    v.override_text_color = Some(BLACK);
    v.selection.bg_fill = HOVER;
    v.selection.stroke = Stroke::new(1.0, BLACK);
    v.text_cursor.stroke = Stroke::new(1.5, BLACK);
    v.text_edit_bg_color = Some(SURFACE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE); // panel separators
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = CornerRadius::same(8);
        w.expansion = 0.0;
    }
    v.widgets.inactive.weak_bg_fill = SURFACE; // menu buttons match the search field
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(24) };
    v.window_stroke = Stroke::new(1.0, LINE);
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.visuals.menu_corner_radius = CornerRadius::same(RADIUS);
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

// ── Controls ───────────────────────────────────────────────────────────────

/// The one black button per screen.
pub fn primary_button(ui: &mut Ui, label: &str, size: Vec2, enabled: bool) -> Response {
    let (fill, hover_fill, text) = if enabled { (BLACK, BLACK_HOVER, WHITE) } else { (LINE, LINE, GRAY_2) };
    button(ui, label, size, enabled, fill, hover_fill, text)
}

/// Everything else.
pub fn secondary_button(ui: &mut Ui, label: &str, size: Vec2, enabled: bool) -> Response {
    button(ui, label, size, enabled, SURFACE, HOVER, if enabled { BLACK } else { GRAY_2 })
}

fn button(ui: &mut Ui, label: &str, size: Vec2, enabled: bool, fill: Color32, hover_fill: Color32, text: Color32) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let fill = if response.hovered() { hover_fill } else { fill };
    ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, semibold(14.0), text);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

const MENU_WIDTH: f32 = 196.0;

/// A row in a dropdown menu, macOS-style: a checkmark beside the current
/// choice, a soft highlight only under the pointer, room around the text.
pub fn menu_item(ui: &mut Ui, label: &str, checked: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(MENU_WIDTH, 30.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), HOVER);
    }
    if checked {
        let c = egui::pos2(rect.left() + 16.0, rect.center().y);
        let stroke = Stroke::new(1.75, BLACK);
        ui.painter().line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.0, 3.5)], stroke);
        ui.painter().line_segment([c + Vec2::new(-1.0, 3.5), c + Vec2::new(5.0, -4.0)], stroke);
    }
    ui.painter().text(egui::pos2(rect.left() + 32.0, rect.center().y), egui::Align2::LEFT_CENTER, label, regular(14.0), BLACK);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, checked, label));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A hairline between groups in a dropdown menu.
pub fn menu_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(MENU_WIDTH, 11.0), Sense::hover());
    ui.painter().hline(rect.shrink2(Vec2::new(8.0, 0.0)).x_range(), rect.center().y, Stroke::new(1.0, LINE));
}

/// Inline text that acts like a link.
pub fn link(ui: &mut Ui, label: &str, size: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(label.into(), semibold(size), BLACK);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    ui.painter().galley(rect.min, galley, BLACK);
    if response.hovered() {
        ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, BLACK));
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, label));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Single-line text cut off with "…" at `max_width`.
pub fn text_truncated(ui: &Ui, pos: Pos2, align: egui::Align2, text: &str, font: FontId, color: Color32, max_width: f32) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.into(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    let galley = ui.painter().layout_job(job);
    let rect = align.anchor_size(pos, galley.size());
    ui.painter().galley(rect.min, galley, color);
    rect
}

/// A hairline outline, used for selection (black) and hover (gray).
pub fn ring(ui: &Ui, rect: Rect, color: Color32, width: f32) {
    ui.painter().rect_stroke(rect, CornerRadius::same(RADIUS), Stroke::new(width, color), StrokeKind::Outside);
}
