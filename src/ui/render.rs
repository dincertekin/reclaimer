use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};
use super::ReclaimerApp;
use super::types::*;

fn fmt_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn fmt_hex(n: u64) -> String {
    format!("0x{:08X}", n)
}

pub(super) fn fmt_num(n: u64) -> String {
    let digits = n.to_string();
    let mut result = String::new();
    for (i, digit) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(digit);
    }
    result.chars().rev().collect()
}

fn col_widths(total: f32) -> [f32; 5] {
    [total * 0.36, total * 0.11, total * 0.13, total * 0.15, total * 0.25]
}

// Each column gets an explicit absolute rect so headers and row cells share
// identical x positions. Using allocate_ui_with_layout in sequence would add
// item_spacing.x (6 px) between each call, shifting later columns right by a
// cumulative amount that differs between the header and the scroll-area rows.
fn col_rects(origin: egui::Pos2, content_w: f32, height: f32) -> [egui::Rect; 5] {
    let [c0, c1, c2, c3, c4] = col_widths(content_w);
    let x1 = origin.x + c0;
    let x2 = x1 + c1;
    let x3 = x2 + c2;
    let x4 = x3 + c3;
    [
        egui::Rect::from_min_size(origin,                    Vec2::new(c0, height)),
        egui::Rect::from_min_size(egui::pos2(x1, origin.y), Vec2::new(c1, height)),
        egui::Rect::from_min_size(egui::pos2(x2, origin.y), Vec2::new(c2, height)),
        egui::Rect::from_min_size(egui::pos2(x3, origin.y), Vec2::new(c3, height)),
        egui::Rect::from_min_size(egui::pos2(x4, origin.y), Vec2::new(c4, height)),
    ]
}

fn accent_tint(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(96, 165, 250, alpha)
}

enum SidebarIcon { All, Images, Documents, Media, Other, Chat, Database, Phone }

fn draw_icon(painter: &egui::Painter, bounds: egui::Rect, icon: SidebarIcon, color: Color32) {
    match icon {
        SidebarIcon::All       => icon_grid(painter, bounds, color),
        SidebarIcon::Images    => icon_photo(painter, bounds, color),
        SidebarIcon::Documents => icon_document(painter, bounds, color),
        SidebarIcon::Media     => icon_play(painter, bounds, color),
        SidebarIcon::Other     => icon_diamond(painter, bounds, color),
        SidebarIcon::Chat      => icon_chat(painter, bounds, color),
        SidebarIcon::Database  => icon_database(painter, bounds, color),
        SidebarIcon::Phone     => icon_phone(painter, bounds, color),
    }
}

fn icon_grid(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let sq_size = 4.5_f32;
    let gap     = 2.5_f32;
    let total   = sq_size * 2.0 + gap;
    let ox = bounds.center().x - total / 2.0;
    let oy = bounds.center().y - total / 2.0;
    for row in 0..2_i32 {
        for col in 0..2_i32 {
            let min = egui::pos2(ox + col as f32 * (sq_size + gap), oy + row as f32 * (sq_size + gap));
            painter.rect_filled(egui::Rect::from_min_size(min, Vec2::splat(sq_size)), 1.5, color);
        }
    }
}

fn icon_photo(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let inner  = bounds.shrink(1.5);
    let stroke = Stroke::new(1.0, color);
    painter.rect_stroke(inner, 1.5, stroke, egui::StrokeKind::Middle);
    let sun    = egui::pos2(inner.left() + inner.width() * 0.28, inner.top() + inner.height() * 0.30);
    painter.circle_filled(sun, 1.8, color);
    let base_l = egui::pos2(inner.left() + 1.0, inner.bottom() - 1.5);
    let base_r = egui::pos2(inner.right() - 1.0, inner.bottom() - 1.5);
    let peak   = egui::pos2(inner.center().x, inner.top() + inner.height() * 0.50);
    let mid    = egui::pos2(inner.right() - inner.width() * 0.25, inner.top() + inner.height() * 0.62);
    painter.line_segment([base_l, peak], stroke);
    painter.line_segment([peak, mid],    stroke);
    painter.line_segment([mid, base_r],  stroke);
}

fn icon_document(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let inner  = bounds.shrink(1.5);
    let fold   = inner.width() * 0.28;
    let stroke = Stroke::new(1.0, color);
    let tl     = inner.min;
    let bl     = egui::pos2(inner.min.x, inner.max.y);
    let br     = inner.max;
    let fold_x = egui::pos2(inner.max.x - fold, inner.min.y);
    let fold_y = egui::pos2(inner.max.x, inner.min.y + fold);
    painter.line_segment([tl, fold_x], stroke);
    painter.line_segment([fold_x, fold_y], stroke);
    painter.line_segment([fold_y, br], stroke);
    painter.line_segment([br, bl], stroke);
    painter.line_segment([bl, tl], stroke);
    let thin_stroke = Stroke::new(0.8, color);
    let x1 = inner.left() + 2.0;
    let x2 = inner.right() - 2.5;
    for i in 0..3 {
        let y = inner.top() + inner.height() * (0.50 + i as f32 * 0.18);
        painter.hline(x1..=x2, y, thin_stroke);
    }
}

fn icon_play(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let top_left    = egui::pos2(bounds.left() + bounds.width() * 0.22, bounds.top()    + bounds.height() * 0.14);
    let bottom_left = egui::pos2(bounds.left() + bounds.width() * 0.22, bounds.bottom() - bounds.height() * 0.14);
    let right_tip   = egui::pos2(bounds.right() - bounds.width() * 0.14, bounds.center().y);
    painter.add(egui::Shape::convex_polygon(vec![top_left, right_tip, bottom_left], color, Stroke::NONE));
}

fn icon_diamond(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let cx     = bounds.center().x;
    let cy     = bounds.center().y;
    let half_w = bounds.width()  * 0.44;
    let half_h = bounds.height() * 0.44;
    let stroke = Stroke::new(1.0, color);
    painter.line_segment([egui::pos2(cx, cy - half_h), egui::pos2(cx + half_w, cy)], stroke);
    painter.line_segment([egui::pos2(cx + half_w, cy), egui::pos2(cx, cy + half_h)], stroke);
    painter.line_segment([egui::pos2(cx, cy + half_h), egui::pos2(cx - half_w, cy)], stroke);
    painter.line_segment([egui::pos2(cx - half_w, cy), egui::pos2(cx, cy - half_h)], stroke);
}

fn icon_chat(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let inner    = bounds.shrink(1.5);
    let bubble_h = inner.height() * 0.75;
    let bubble   = egui::Rect::from_min_size(inner.min, Vec2::new(inner.width(), bubble_h));
    painter.rect_stroke(bubble, 2.5, Stroke::new(1.0, color), egui::StrokeKind::Middle);
    let tail_x = inner.left() + inner.width() * 0.25;
    painter.line_segment(
        [egui::pos2(tail_x, inner.top() + bubble_h), egui::pos2(tail_x, inner.bottom())],
        Stroke::new(1.0, color),
    );
}

fn icon_database(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let inner  = bounds.shrink(1.5);
    let oval_h = inner.height() * 0.22;
    let gap    = (inner.height() - oval_h * 3.0) / 2.0;
    let stroke = Stroke::new(1.0, color);
    for i in 0..3 {
        let y    = inner.top() + i as f32 * (oval_h + gap);
        let oval = egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), oval_h));
        painter.rect_stroke(oval, oval_h / 2.0, stroke, egui::StrokeKind::Middle);
    }
}

fn icon_phone(painter: &egui::Painter, bounds: egui::Rect, color: Color32) {
    let cx    = bounds.center().x;
    let cy    = bounds.center().y;
    let ear   = egui::Rect::from_center_size(
        egui::pos2(cx - bounds.width() * 0.18, cy - bounds.height() * 0.22),
        Vec2::new(bounds.width() * 0.28, bounds.height() * 0.22),
    );
    let mouth = egui::Rect::from_center_size(
        egui::pos2(cx + bounds.width() * 0.18, cy + bounds.height() * 0.22),
        Vec2::new(bounds.width() * 0.28, bounds.height() * 0.22),
    );
    painter.rect_filled(ear,   2.0, color);
    painter.rect_filled(mouth, 2.0, color);
    painter.line_segment([ear.center(), mouth.center()], Stroke::new(2.5, color));
}

impl ReclaimerApp {
    pub(super) fn draw_toolbar(&self, ui: &mut egui::Ui) -> bool {
        let toolbar_bounds = ui.max_rect();
        ui.painter().hline(toolbar_bounds.x_range(), toolbar_bounds.bottom() - 1.0, Stroke::new(1.0, BORDER));

        let mut open_clicked = false;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(16.0);
            ui.label(RichText::new("Reclaimer").color(TEXT).font(FontId::monospace(14.0)).strong());
            ui.add_space(14.0);
            ui.add(egui::Separator::default().vertical());
            ui.add_space(10.0);

            let load_btn_width  = 140.0;
            let breadcrumb_width = (ui.available_width() - load_btn_width).max(0.0);

            ui.allocate_ui_with_layout(
                Vec2::new(breadcrumb_width, 48.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(RichText::new("EVIDENCE").color(TEXT_DIM).font(FontId::monospace(11.0)));
                    ui.add_space(5.0);
                    ui.label(RichText::new("/").color(TEXT_DIM).font(FontId::monospace(11.0)));
                    ui.add_space(5.0);
                    let (name, color) = match &self.image_path {
                        Some(path) => (
                            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            TEXT,
                        ),
                        None => ("no image loaded".into(), TEXT_DIM),
                    };
                    ui.label(RichText::new(name).color(color).font(FontId::monospace(13.0)));
                },
            );

            let load_btn_result = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                ui.add_enabled(
                    !matches!(self.phase, Phase::Scanning),
                    egui::Button::new(RichText::new("Load Image").color(TEXT_SEC).size(15.0))
                        .fill(PANEL_RAISED)
                        .stroke(Stroke::new(1.0, BORDER_MID)),
                ).clicked()
            });

            open_clicked = load_btn_result.inner;
        });

        open_clicked
    }
}

impl ReclaimerApp {
    pub(super) fn draw_statusbar(&self, ui: &mut egui::Ui) {
        let statusbar_bounds = ui.max_rect();
        ui.painter().hline(statusbar_bounds.x_range(), statusbar_bounds.top(), Stroke::new(1.0, BORDER));

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(14.0);

            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
            ui.painter().circle_filled(dot_rect.center(), 4.0, self.dot_state.color());
            if self.dot_state == DotState::Scanning {
                ui.painter().circle_stroke(dot_rect.center(), 6.5, Stroke::new(1.0, accent_tint(80)));
            }

            ui.add_space(8.0);
            ui.label(RichText::new(&self.status_text).color(TEXT_SEC).size(14.0));

            if self.phase == Phase::Scanning && self.total_sectors > 0 {
                let progress       = self.scanned_sectors as f32 / self.total_sectors as f32;
                let avail_width    = ui.available_width();
                let scan_bar_width = 180.0;
                let right_label_w  = 120.0;
                let center_offset  = (avail_width - right_label_w - scan_bar_width) / 2.0;

                ui.add_space(center_offset.max(16.0));
                let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(scan_bar_width, 3.0), egui::Sense::hover());
                ui.painter().rect_filled(bar_rect, 2.0, BORDER_MID);
                let filled_rect = egui::Rect::from_min_size(bar_rect.min, Vec2::new(bar_rect.width() * progress, bar_rect.height()));
                ui.painter().rect_filled(filled_rect, 2.0, ACCENT);
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{:.0}%", progress * 100.0)).color(ACCENT).font(FontId::monospace(13.0)).strong());
            }

            if matches!(self.phase, Phase::Scanning | Phase::Done) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.0);
                    let right_label = if self.phase == Phase::Done {
                        "recovered/".to_string()
                    } else {
                        format!("{} found", self.found_count)
                    };
                    ui.label(RichText::new(right_label).color(TEXT_DIM).font(FontId::monospace(13.0)));
                });
            }
        });
    }
}

impl ReclaimerApp {
    pub(super) fn draw_workspace(&self, ui: &mut egui::Ui) -> (Option<Filter>, Option<ResultAction>) {
        let mut filter_change = None;
        let mut result_action = None;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let sidebar_w = 210.0;
            let sidebar_h = ui.available_height();

            let sidebar_resp = ui.allocate_ui_with_layout(
                Vec2::new(sidebar_w, sidebar_h),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.draw_sidebar(ui),
            );
            filter_change = sidebar_resp.inner;

            ui.painter().vline(sidebar_resp.response.rect.right(), sidebar_resp.response.rect.y_range(), Stroke::new(1.0, BORDER));

            let main_resp = ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.draw_main(ui),
            );
            result_action = main_resp.inner;
        });

        (filter_change, result_action)
    }
}

impl ReclaimerApp {
    fn draw_sidebar(&self, ui: &mut egui::Ui) -> Option<Filter> {
        ui.painter().rect_filled(ui.max_rect(), 0.0, SIDEBAR_BG);
        ui.style_mut().spacing.item_spacing = Vec2::new(0.0, 0.0);
        let mut filter_change = None;

        sidebar_section(ui, "Evidence Tree");
        if sidebar_item(ui, SidebarIcon::All, "All Artifacts", self.count(&Filter::All), self.active_filter == Filter::All).is_some() {
            filter_change = Some(Filter::All);
        }

        sidebar_divider(ui);
        sidebar_section(ui, "File Types");

        for (icon, label, filter) in [
            (SidebarIcon::Images,    "Images",    Filter::Images),
            (SidebarIcon::Documents, "Documents", Filter::Documents),
            (SidebarIcon::Media,     "Media",     Filter::Media),
            (SidebarIcon::Other,     "Other",     Filter::Other),
        ] {
            let count  = self.count(&filter);
            let active = self.active_filter == filter;
            if sidebar_item(ui, icon, label, count, active).is_some() {
                filter_change = Some(filter);
            }
        }

        sidebar_divider(ui);
        sidebar_section(ui, "Artifacts");
        sidebar_item_soon(ui, SidebarIcon::Chat,     "WhatsApp");
        sidebar_item_soon(ui, SidebarIcon::Database, "SQLite DBs");
        sidebar_item_soon(ui, SidebarIcon::Phone,    "Call Logs");

        filter_change
    }
}

fn sidebar_section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(RichText::new(label.to_uppercase()).color(TEXT_DIM).font(FontId::monospace(11.0)).strong());
    });
    ui.add_space(4.0);
}

fn sidebar_divider(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let divider_rect = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 1.0));
    ui.painter().rect_filled(divider_rect, 0.0, BORDER);
    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.add_space(4.0);
}

fn sidebar_item(ui: &mut egui::Ui, icon: SidebarIcon, label: &str, count: usize, active: bool) -> Option<()> {
    let height = 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg = if active {
            accent_tint(22)
        } else if resp.hovered() {
            accent_tint(10)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 0.0, bg);

        if active {
            let indicator = egui::Rect::from_min_size(rect.min, Vec2::new(3.0, height - 8.0)).translate(Vec2::new(0.0, 4.0));
            ui.painter().rect_filled(indicator, egui::CornerRadius::same(2), ACCENT);
        }

        let text_color = if active { TEXT } else { TEXT_SEC };
        let icon_color = if active { ACCENT } else { TEXT_DIM };

        let mut row = ui.new_child(
            egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.add_space(16.0);

        let (icon_rect, _) = row.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        if row.is_rect_visible(icon_rect) {
            draw_icon(row.painter(), icon_rect, icon, icon_color);
        }
        row.add_space(9.0);
        row.label(RichText::new(label).color(text_color).size(15.0));

        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            if count > 0 || active {
                let (badge_bg, badge_text_color, badge_border) = if active {
                    (accent_tint(25), ACCENT, accent_tint(120))
                } else {
                    (Color32::TRANSPARENT, TEXT_DIM, BORDER_MID)
                };
                egui::Frame::NONE
                    .fill(badge_bg)
                    .stroke(Stroke::new(1.0, badge_border))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(RichText::new(count.to_string()).color(badge_text_color).font(FontId::monospace(12.0)));
                    });
            }
        });
    }

    if resp.clicked() { Some(()) } else { None }
}

fn sidebar_item_soon(ui: &mut egui::Ui, icon: SidebarIcon, label: &str) {
    let height = 40.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let mut row = ui.new_child(
            egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.add_space(16.0);
        let (icon_rect, _) = row.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        if row.is_rect_visible(icon_rect) {
            draw_icon(row.painter(), icon_rect, icon, TEXT_DIM);
        }
        row.add_space(9.0);
        row.label(RichText::new(label).color(TEXT_DIM).size(15.0));

        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, BORDER_MID))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new("SOON").color(TEXT_DIM).font(FontId::monospace(10.0)));
                });
        });
    }
}

impl ReclaimerApp {
    fn draw_main(&self, ui: &mut egui::Ui) -> Option<ResultAction> {
        let mut action = None;
        let full_rect  = ui.max_rect();
        let detail_w   = 320.0_f32;

        // The table always occupies the full width, even when the detail panel is
        // open. This keeps column widths stable so they don't jump when selecting
        // a row. The detail panel draws as an overlay on top of the right side.
        let mut table_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(full_rect).layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        if let Some(a) = self.draw_table(&mut table_ui) {
            action = Some(a);
        }

        if let Some(idx) = self.selected_idx
            && let Some(file) = self.found_files.get(idx)
        {
            let panel_rect = egui::Rect::from_min_max(
                egui::pos2(full_rect.right() - detail_w, full_rect.top()),
                full_rect.max,
            );
            ui.painter().vline(panel_rect.left(), full_rect.y_range(), Stroke::new(1.0, BORDER));

            let mut detail_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(panel_rect).layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            if let Some(a) = self.draw_detail(&mut detail_ui, idx, file) {
                action = Some(a);
            }
        }

        action
    }

    fn draw_table(&self, ui: &mut egui::Ui) -> Option<ResultAction> {
        let mut action     = None;
        let mut start_scan = false;

        // Computed once here so header and scroll-area rows always share the same
        // column widths. available_width() can differ slightly between the two
        // draw sites depending on scroll-area internals.
        let content_w = ui.max_rect().width();
        let total_h   = ui.max_rect().height();
        let toolbar_h = 46.0_f32;
        let header_h  = 36.0_f32;
        let scroll_h  = (total_h - toolbar_h - header_h).max(0.0);

        let w = ui.available_width();
        let (toolbar_rect, _) = ui.allocate_exact_size(Vec2::new(w, toolbar_h), egui::Sense::hover());
        ui.painter().rect_filled(toolbar_rect, 0.0, PANEL);
        ui.painter().hline(toolbar_rect.x_range(), toolbar_rect.bottom(), Stroke::new(1.0, BORDER));

        let toolbar_inner = egui::Rect::from_min_size(
            toolbar_rect.min + Vec2::new(18.0, 0.0),
            Vec2::new(toolbar_rect.width() - 36.0, toolbar_h),
        );
        let mut toolbar_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(toolbar_inner).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        let artifact_count = self.visible_files().len();
        toolbar_ui.label(
            RichText::new(format!("{} artifact{}", artifact_count, if artifact_count == 1 { "" } else { "s" }))
                .color(TEXT_SEC)
                .size(15.0),
        );

        if self.phase == Phase::Done {
            toolbar_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(
                    egui::Button::new(RichText::new("Re-scan").color(ACCENT).size(14.0))
                        .fill(accent_tint(15))
                        .stroke(Stroke::new(1.0, accent_tint(100))),
                ).clicked() {
                    action = Some(ResultAction::StartScan);
                }
            });
        }

        let w = ui.available_width();
        let (header_rect, _) = ui.allocate_exact_size(Vec2::new(w, header_h), egui::Sense::hover());
        ui.painter().rect_filled(header_rect, 0.0, BG);
        ui.painter().hline(header_rect.x_range(), header_rect.bottom(), Stroke::new(1.0, BORDER));

        let cell_rects = col_rects(header_rect.min, content_w, header_h);
        for (col, label, left_aligned) in [
            (0, "FILENAME", true),
            (1, "TYPE",     true),
            (2, "SIZE",     false),
            (3, "SECTOR",   false),
            (4, "OFFSET",   false),
        ] {
            let layout = if left_aligned {
                egui::Layout::left_to_right(egui::Align::Center)
            } else {
                egui::Layout::right_to_left(egui::Align::Center)
            };
            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[col]).layout(layout));
            cell.add_space(18.0);
            cell.label(RichText::new(label).color(TEXT_DIM).font(FontId::monospace(12.0)).strong());
        }

        // AlwaysHidden keeps the scroll area at full content_w so column positions
        // inside the scroll area match the headers above. Scrolling still works via
        // mouse wheel and trackpad — only the visible scrollbar widget is hidden.
        egui::ScrollArea::vertical()
            .max_height(scroll_h)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .id_salt("results_scroll")
            .show(ui, |ui| {
                let files = self.visible_files();

                if files.is_empty() {
                    ui.add_space(90.0);
                    ui.vertical_centered(|ui| {
                        match self.phase {
                            Phase::Idle => {
                                ui.label(RichText::new("Load a disk image to begin").color(TEXT_SEC).size(20.0));
                                ui.add_space(10.0);
                                ui.label(RichText::new("or drop a .img file anywhere in this window").color(TEXT_DIM).size(14.0));
                            }
                            Phase::Ready => {
                                let image_name = self.image_path.as_ref()
                                    .and_then(|p| p.file_name())
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                ui.label(RichText::new(&image_name).color(TEXT).font(FontId::monospace(16.0)).strong());
                                ui.add_space(6.0);
                                ui.label(RichText::new("Ready to scan").color(TEXT_DIM).size(15.0));
                                ui.add_space(28.0);
                                if ui.add(
                                    egui::Button::new(
                                        RichText::new("  Run Scan  ").color(Color32::WHITE).size(16.0).strong(),
                                    )
                                    .fill(ACCENT)
                                    .stroke(Stroke::new(0.0, Color32::TRANSPARENT)),
                                ).clicked() {
                                    start_scan = true;
                                }
                            }
                            Phase::Scanning => {
                                ui.label(RichText::new("Scanning\u{2026}").color(TEXT_SEC).size(18.0));
                                ui.add_space(28.0);

                                if self.total_sectors > 0 {
                                    let progress = self.scanned_sectors as f32 / self.total_sectors as f32;

                                    let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(340.0, 4.0), egui::Sense::hover());
                                    ui.painter().rect_filled(bar_rect, 2.0, BORDER_MID);
                                    let filled_rect = egui::Rect::from_min_size(bar_rect.min, Vec2::new(bar_rect.width() * progress, bar_rect.height()));
                                    ui.painter().rect_filled(filled_rect, 2.0, ACCENT);

                                    ui.add_space(20.0);
                                    ui.label(RichText::new(format!("{:.0}%", progress * 100.0)).color(ACCENT).size(40.0).strong());
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!("sector {} of {}", fmt_num(self.scanned_sectors), fmt_num(self.total_sectors)))
                                            .color(TEXT_DIM)
                                            .font(FontId::monospace(14.0)),
                                    );
                                } else {
                                    ui.label(RichText::new("Initialising\u{2026}").color(TEXT_DIM).size(15.0));
                                }

                                if self.found_count > 0 {
                                    ui.add_space(18.0);
                                    ui.label(
                                        RichText::new(format!("{} artifact{} recovered so far", self.found_count, if self.found_count == 1 { "" } else { "s" }))
                                            .color(GREEN)
                                            .size(15.0),
                                    );
                                }
                            }
                            Phase::Done => {
                                ui.label(RichText::new("No artifacts match the current filter.").color(TEXT_SEC).size(16.0));
                                ui.add_space(8.0);
                                ui.label(RichText::new("Try selecting All Artifacts in the sidebar.").color(TEXT_DIM).size(14.0));
                            }
                        }
                    });
                    return;
                }

                let row_h = 44.0;
                for (orig_idx, file) in &files {
                    let orig_idx  = *orig_idx;
                    let is_selected = self.selected_idx == Some(orig_idx);
                    let (row_rect, row_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), egui::Sense::click());

                    if ui.is_rect_visible(row_rect) {
                        let alternating_tint = if orig_idx.is_multiple_of(2) {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 4)
                        } else {
                            Color32::TRANSPARENT
                        };

                        let row_bg = if is_selected {
                            accent_tint(25)
                        } else if row_resp.hovered() {
                            accent_tint(12)
                        } else {
                            alternating_tint
                        };

                        ui.painter().rect_filled(row_rect, 0.0, row_bg);

                        if is_selected {
                            let selection_bar = egui::Rect::from_min_size(row_rect.min, Vec2::new(3.0, row_rect.height()));
                            ui.painter().rect_filled(selection_bar, 0.0, ACCENT);
                        }

                        ui.painter().hline(row_rect.x_range(), row_rect.bottom(), Stroke::new(1.0, BORDER));

                        let cell_rects = col_rects(row_rect.min, content_w, row_h);

                        {
                            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[0]).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            cell.add_space(18.0);
                            cell.add(egui::Label::new(
                                RichText::new(&file.filename)
                                    .color(if is_selected { TEXT } else { TEXT_SEC })
                                    .font(FontId::monospace(14.0))
                            ).truncate());
                        }
                        {
                            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[1]).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            cell.add_space(18.0);
                            let badge_color = file.category.color();
                            egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(badge_color.r(), badge_color.g(), badge_color.b(), 18))
                                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(badge_color.r(), badge_color.g(), badge_color.b(), 60)))
                                .corner_radius(egui::CornerRadius::same(5))
                                .inner_margin(egui::Margin::symmetric(7, 3))
                                .show(&mut cell, |ui| {
                                    ui.label(RichText::new(file.extension.to_uppercase()).color(badge_color).font(FontId::monospace(11.0)).strong());
                                });
                        }
                        {
                            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[2]).layout(egui::Layout::right_to_left(egui::Align::Center)));
                            cell.add_space(18.0);
                            cell.label(RichText::new(fmt_size(file.size_bytes)).color(TEXT_DIM).font(FontId::monospace(13.0)));
                        }
                        {
                            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[3]).layout(egui::Layout::right_to_left(egui::Align::Center)));
                            cell.add_space(18.0);
                            cell.label(RichText::new(fmt_num(file.sector)).color(TEXT_DIM).font(FontId::monospace(13.0)));
                        }
                        {
                            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(cell_rects[4]).layout(egui::Layout::right_to_left(egui::Align::Center)));
                            cell.add_space(18.0);
                            cell.label(RichText::new(fmt_hex(file.offset)).color(TEXT_DIM).font(FontId::monospace(13.0)));
                        }
                    }

                    if row_resp.clicked() {
                        action = Some(ResultAction::Select(orig_idx));
                    }
                }
            });

        if start_scan {
            action = Some(ResultAction::StartScan);
        }
        action
    }

    fn draw_detail(&self, ui: &mut egui::Ui, idx: usize, file: &FoundFile) -> Option<ResultAction> {
        let mut action = None;

        ui.painter().rect_filled(ui.max_rect(), 0.0, PANEL);
        ui.add_space(28.0);

        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let badge_color = file.category.color();
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter().circle_filled(dot_rect.center() + Vec2::new(0.0, 1.0), 5.0, badge_color);
            ui.add_space(7.0);
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(badge_color.r(), badge_color.g(), badge_color.b(), 18))
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(badge_color.r(), badge_color.g(), badge_color.b(), 60)))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(9, 4))
                .show(ui, |ui| {
                    ui.label(RichText::new(file.extension.to_uppercase()).color(badge_color).font(FontId::monospace(12.0)).strong());
                });
        });

        ui.add_space(14.0);

        ui.horizontal(|ui| {
            ui.add_space(22.0);
            ui.add(egui::Label::new(
                RichText::new(&file.filename).color(TEXT).font(FontId::monospace(15.0)).strong()
            ).truncate());
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(22.0);
            ui.label(RichText::new(&file.file_type).color(TEXT_DIM).size(14.0));
        });

        ui.add_space(22.0);

        let separator_width = ui.available_width() - 44.0;
        let draw_separator  = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                let (sep_rect, _) = ui.allocate_exact_size(Vec2::new(separator_width, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(sep_rect, 0.0, BORDER);
            });
        };

        draw_separator(ui);
        ui.add_space(16.0);

        for (label, value) in [
            ("Size",   fmt_size(file.size_bytes)),
            ("Bytes",  fmt_num(file.size_bytes as u64)),
            ("Sector", fmt_num(file.sector)),
            ("Offset", fmt_hex(file.offset)),
        ] {
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                let row_w = ui.available_width() - 22.0;
                ui.allocate_ui_with_layout(Vec2::new(row_w, 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(RichText::new(label).color(TEXT_SEC).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(value).color(TEXT).font(FontId::monospace(14.0)).strong());
                    });
                });
            });
            ui.add_space(2.0);
        }

        ui.add_space(22.0);
        draw_separator(ui);
        ui.add_space(18.0);

        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let btn_w = ui.available_width() - 22.0;
            if ui.add_sized(
                Vec2::new(btn_w, 48.0),
                egui::Button::new(RichText::new("Open File").color(Color32::WHITE).size(16.0).strong())
                    .fill(ACCENT)
                    .stroke(Stroke::new(0.0, Color32::TRANSPARENT)),
            ).clicked() {
                action = Some(ResultAction::Open(idx));
            }
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let btn_w = ui.available_width() - 22.0;
            if ui.add_sized(
                Vec2::new(btn_w, 42.0),
                egui::Button::new(RichText::new("Copy Path").color(TEXT_SEC).size(15.0))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, BORDER_MID)),
            ).clicked() {
                action = Some(ResultAction::CopyPath(idx));
            }
        });

        action
    }
}
