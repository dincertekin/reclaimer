//! One view for the whole scan: files appear in the grid as they are found,
//! the header turns from progress into a summary when it's done, and an
//! inspector opens only when a file is selected.

use eframe::egui::{
    self, load::SizedTexture, pos2, vec2, Align, Align2, CornerRadius, CursorIcon, Frame, Layout, Margin,
    Rect, ScrollArea, Sense, Stroke, Ui,
};

use super::icons::{self, Icon};
use super::theme::*;
use super::types::*;
use super::ReclaimerApp;

const TILE_MIN: f32 = 150.0;
const TILE_GAP: f32 = 16.0;
const TILE_TEXT: f32 = 46.0; // name + detail under each thumbnail
const GRID_THUMB: u32 = 320;
const PREVIEW: u32 = 720;
const INSPECTOR_W: f32 = 320.0;

impl ReclaimerApp {
    pub(super) fn results_screen(&mut self, ui: &mut Ui) {
        egui::Panel::top("header")
            .frame(Frame::NONE.fill(WHITE).inner_margin(Margin { left: 32, right: 32, top: (TITLEBAR + 20.0) as i8, bottom: 16 }))
            .show_inside(ui, |ui| self.header(ui));

        if let Some(idx) = self.selected.filter(|&i| i < self.found.len()) {
            egui::Panel::right("inspector")
                .exact_size(INSPECTOR_W)
                .resizable(false)
                .frame(Frame::NONE.fill(WHITE).inner_margin(Margin::symmetric(24, 0)))
                .show_inside(ui, |ui| {
                    // Scrolls when the window is short, so the buttons stay reachable.
                    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        ui.add_space(20.0);
                        self.inspector(ui, idx);
                        ui.add_space(24.0);
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(WHITE).inner_margin(Margin { left: 0, right: 0, top: 20, bottom: 0 }))
            .show_inside(ui, |ui| self.grid(ui));
    }

    // ── Header ─────────────────────────────────────────────────────────────

    fn header(&mut self, ui: &mut Ui) {
        let scanning = self.phase == Phase::Scanning;
        let source = self.source.as_ref().map(Source::name).unwrap_or_default();
        let n = self.found.len();

        let title = match (scanning, n) {
            (true, _) => format!("Scanning {source}"),
            (false, 0) => "No files found".into(),
            (false, n) => plural(n, "file recovered", "files recovered"),
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).font(semibold(24.0)).color(BLACK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if scanning {
                    if secondary_button(ui, "Stop", vec2(88.0, 34.0), true).clicked() {
                        self.stop_scan();
                    }
                } else {
                    if primary_button(ui, "New Scan", vec2(116.0, 34.0), true).clicked() {
                        self.new_scan();
                    }
                    if let Some(folder) = self.scan_folder.clone().filter(|_| n > 0) {
                        let label = format!("Show in {FILE_MANAGER}");
                        if secondary_button(ui, &label, vec2(150.0, 34.0), true).clicked() {
                            let _ = open::that(folder);
                        }
                    }
                }
            });
        });

        let (done, total) = self.progress;
        let fraction = if total > 0 { done as f64 / total as f64 } else { 0.0 };
        let subtitle = if let Some(error) = &self.error {
            error.clone()
        } else if scanning && done == 0 && !self.stage.is_empty() {
            // Reading the filesystem: there is no percentage yet.
            format!("{}  \u{00B7}  {} found", self.stage, fmt_count(n))
        } else if scanning {
            let mut parts = vec![format!("{} found", fmt_count(n)), format!("{:.0}%", fraction * 100.0)];
            let elapsed = self.started.elapsed().as_secs_f64();
            if elapsed > 3.0 && fraction > 0.0 {
                parts.push(fmt_remaining(elapsed / fraction - elapsed));
            }
            parts.join("  \u{00B7}  ")
        } else {
            let mut parts = vec![format!("From {source}")];
            if let Some(folder) = &self.scan_folder {
                parts.push(format!("Saved to {}", pretty_path(folder)));
            }
            if self.stopped {
                parts.push("Stopped early".into());
            }
            parts.join("  \u{00B7}  ")
        };
        ui.label(egui::RichText::new(subtitle).font(regular(14.0)).color(GRAY_1));

        if scanning {
            ui.add_space(10.0);
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
            ui.painter().rect_filled(bar, CornerRadius::same(2), LINE);
            let filled = Rect::from_min_size(bar.min, vec2(bar.width() * fraction as f32, bar.height()));
            ui.painter().rect_filled(filled, CornerRadius::same(2), BLACK);
        }

        if n > 0 {
            ui.add_space(18.0);
            self.filter_bar(ui);
        }
    }

    /// All · Photos · Videos & Audio · ...   and a search field on the right.
    fn filter_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mut choice = None;
            if chip(ui, "All", self.found.len(), self.filter.is_none()).clicked() {
                choice = Some(None);
            }
            for category in Category::ALL {
                let count = self.count(category);
                let active = self.filter == Some(category);
                // Hide empty categories — fewer, more meaningful choices.
                if (count > 0 || active) && chip(ui, category.label(), count, active).clicked() {
                    choice = Some(Some(category));
                }
            }
            if let Some(filter) = choice {
                self.filter = filter;
                self.selected = None;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search")
                        .font(regular(14.0))
                        .desired_width(200.0)
                        .margin(Margin::symmetric(10, 6)),
                );
            });
        });
    }

    // ── Grid ───────────────────────────────────────────────────────────────

    fn grid(&mut self, ui: &mut Ui) {
        let visible = self.visible_files();
        if visible.is_empty() {
            self.empty_state(ui);
            return;
        }

        let width = ui.available_width() - 2.0 * MARGIN;
        let per_row = (((width + TILE_GAP) / (TILE_MIN + TILE_GAP)).floor() as usize).max(1);
        let tile_w = (width - TILE_GAP * (per_row - 1) as f32) / per_row as f32;
        let rows = visible.len().div_ceil(per_row);

        let mut clicked = None;
        let mut opened = None;
        ui.spacing_mut().item_spacing.y = TILE_GAP;
        ScrollArea::vertical().auto_shrink(false).show_rows(ui, tile_w + TILE_TEXT, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = TILE_GAP;
                    ui.add_space(MARGIN);
                    for &idx in visible.iter().skip(row * per_row).take(per_row) {
                        let response = self.tile(ui, idx, tile_w);
                        if response.double_clicked() {
                            opened = Some(idx);
                        } else if response.clicked() {
                            clicked = Some(idx);
                        }
                    }
                });
            }
        });

        if let Some(idx) = clicked {
            self.selected = if self.selected == Some(idx) { None } else { Some(idx) };
        }
        if let Some(idx) = opened {
            self.open_file(idx);
        }
    }

    fn tile(&mut self, ui: &mut Ui, idx: usize, w: f32) -> egui::Response {
        let file = self.found[idx].clone();
        let (rect, response) = ui.allocate_exact_size(vec2(w, w + TILE_TEXT), Sense::click());
        let selected = self.selected == Some(idx);
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &file.filename));
        let thumb_rect = Rect::from_min_size(rect.min, vec2(w, w));

        ui.painter().rect_filled(thumb_rect, CornerRadius::same(RADIUS), SURFACE);
        let thumb = (file.category == Category::Photos).then(|| self.thumbs.get(&file.path, GRID_THUMB)).flatten();
        match thumb {
            Some(texture) => {
                // Fill the square, cropping the longer side — like Photos.
                let [tw, th] = texture.size().map(|v| v as f32);
                let uv = if tw > th {
                    let m = (1.0 - th / tw) / 2.0;
                    Rect::from_min_max(pos2(m, 0.0), pos2(1.0 - m, 1.0))
                } else {
                    let m = (1.0 - tw / th) / 2.0;
                    Rect::from_min_max(pos2(0.0, m), pos2(1.0, 1.0 - m))
                };
                egui::Image::from_texture(SizedTexture::from_handle(texture))
                    .uv(uv)
                    .corner_radius(CornerRadius::same(RADIUS))
                    .paint_at(ui, thumb_rect);
            }
            None => file_glyph(ui, thumb_rect, &file, 0.28),
        }

        if self.selected == Some(idx) {
            ring(ui, thumb_rect, BLACK, 2.0);
        } else if response.hovered() {
            ring(ui, thumb_rect, GRAY_2, 1.0);
        }

        let text_top = thumb_rect.left_bottom() + vec2(2.0, 8.0);
        text_truncated(ui, text_top, Align2::LEFT_TOP, &file.filename, regular(13.0), BLACK, w - 4.0);
        let detail = if file.complete { fmt_size(file.size) } else { format!("{} \u{00B7} Partial", fmt_size(file.size)) };
        text_truncated(ui, text_top + vec2(0.0, 19.0), Align2::LEFT_TOP, &detail, regular(12.0), GRAY_1, w - 4.0);

        response.on_hover_cursor(CursorIcon::PointingHand)
    }

    fn empty_state(&self, ui: &mut Ui) {
        let (title, hint) = match self.phase {
            Phase::Scanning => ("Files will appear here as they\u{2019}re found.", ""),
            _ if !self.found.is_empty() => ("No matches", "Try a different search or filter."),
            _ => (
                "Nothing to recover here.",
                "Deleted files may already have been overwritten, or the drive may be encrypted.",
            ),
        };
        ui.add_space((ui.available_height() / 2.0 - 60.0).max(40.0));
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(title).font(semibold(17.0)).color(BLACK));
            if !hint.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(hint).font(regular(14.0)).color(GRAY_1));
            }
        });
    }

    // ── Inspector ──────────────────────────────────────────────────────────

    fn inspector(&mut self, ui: &mut Ui, idx: usize) {
        let file = self.found[idx].clone();
        let width = ui.available_width();

        let (preview, _) = ui.allocate_exact_size(vec2(width, width * 0.62), Sense::hover());
        ui.painter().rect_filled(preview, CornerRadius::same(RADIUS), SURFACE);
        let texture = (file.category == Category::Photos).then(|| self.thumbs.get(&file.path, PREVIEW)).flatten();
        match texture {
            Some(texture) => {
                // Show the whole picture, letterboxed.
                let [tw, th] = texture.size().map(|v| v as f32);
                let scale = (preview.width() / tw).min(preview.height() / th);
                let fit = Rect::from_center_size(preview.center(), vec2(tw, th) * scale);
                egui::Image::from_texture(SizedTexture::from_handle(texture))
                    .corner_radius(CornerRadius::same(RADIUS))
                    .paint_at(ui, fit);
            }
            None => file_glyph(ui, preview, &file, 0.3),
        }

        ui.add_space(18.0);
        ui.add(egui::Label::new(egui::RichText::new(&file.filename).font(semibold(17.0)).color(BLACK)).wrap());
        ui.label(egui::RichText::new(format!("{} \u{00B7} {}", file.kind, fmt_size(file.size))).font(regular(13.0)).color(GRAY_1));
        ui.add_space(16.0);

        let mut rows = vec![("Kind", file.kind.clone())];
        if let Some(folder) = &file.folder {
            rows.push(("Was in", folder.clone()));
        }
        if let Some(modified) = file.modified {
            rows.push(("Modified", fmt_date(modified)));
        }
        rows.push(("Size", format!("{} bytes", fmt_count(file.size as usize))));
        rows.push(("Status", if file.complete { "Intact".into() } else { "Partial".into() }));
        rows.push(("Found at", format!("0x{:08X}", file.offset)));
        for (label, value) in rows {
            let (row, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
            ui.painter().hline(row.x_range(), row.top(), Stroke::new(1.0, LINE));
            let label_rect = ui.painter().text(row.left_center(), Align2::LEFT_CENTER, label, regular(13.0), GRAY_1);
            let room = width - label_rect.width() - 16.0;
            text_truncated(ui, row.right_center(), Align2::RIGHT_CENTER, &value, regular(13.0), BLACK, room);
        }
        if !file.complete {
            ui.add_space(8.0);
            let why = if file.folder.is_some() {
                "Some of this file\u{2019}s space has been reused since it was deleted. It may open with pieces missing."
            } else {
                "Only part of this file was found. It may open with pieces missing."
            };
            ui.label(egui::RichText::new(why).font(regular(12.0)).color(GRAY_1));
        }

        ui.add_space(20.0);
        if primary_button(ui, "Open", vec2(width, 38.0), true).clicked() {
            self.open_file(idx);
        }
        ui.add_space(4.0);
        if secondary_button(ui, &format!("Show in {FILE_MANAGER}"), vec2(width, 38.0), true).clicked() {
            reveal(&file.path);
        }
    }
}

/// A filter chip: "Photos 12". The active one sits on a gray pill.
fn chip(ui: &mut Ui, label: &str, count: usize, active: bool) -> egui::Response {
    let label_g = ui.painter().layout_no_wrap(label.into(), semibold(13.0), if active { BLACK } else { GRAY_1 });
    let count_g = ui.painter().layout_no_wrap(fmt_count(count), regular(13.0), GRAY_2);
    let size = vec2(label_g.size().x + 6.0 + count_g.size().x + 24.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, label));
    if active || response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), if active { SURFACE } else { WHITE.lerp_to_gamma(SURFACE, 0.6) });
    }
    let y = rect.center().y - label_g.size().y / 2.0;
    let label_w = label_g.size().x;
    ui.painter().galley(pos2(rect.left() + 12.0, y), label_g, BLACK);
    ui.painter().galley(pos2(rect.left() + 12.0 + label_w + 6.0, y), count_g, GRAY_2);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Icon + extension label, for files without a thumbnail.
fn file_glyph(ui: &Ui, rect: Rect, file: &FoundFile, scale: f32) {
    let side = rect.width() * scale;
    let icon_rect = Rect::from_center_size(rect.center() - vec2(0.0, side * 0.2), vec2(side, side));
    icons::paint(ui.painter(), icon_rect, Icon::for_extension(&file.extension), GRAY_1);
    ui.painter().text(
        icon_rect.center_bottom() + vec2(0.0, side * 0.25),
        Align2::CENTER_TOP,
        file.extension.to_uppercase(),
        semibold((side * 0.22).clamp(10.0, 14.0)),
        GRAY_1,
    );
}
