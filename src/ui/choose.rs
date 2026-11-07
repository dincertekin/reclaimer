//! "Recover your files. Choose where they were lost."
//! A card per drive, one for a disk image, and one black Scan button.

use eframe::egui::{self, vec2, Align2, CornerRadius, CursorIcon, Frame, Rect, Sense, Ui};

use super::icons::{self, Icon};
use super::theme::*;
use super::types::*;
use super::ReclaimerApp;

const CARD: egui::Vec2 = vec2(176.0, 156.0);
const GAP: f32 = 16.0;

struct Card {
    icon:     Icon,
    title:    String,
    detail:   String,
    selected: bool,
    pick:     Pick,
}

#[derive(Clone, Copy)]
enum Pick {
    Drive(usize),
    Image,
}

impl ReclaimerApp {
    pub(super) fn choose_screen(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("save-to")
            .frame(Frame::NONE.fill(WHITE).inner_margin(egui::Margin::symmetric(32, 24)))
            .show_separator_line(false)
            .show_inside(ui, |ui| self.save_to_line(ui));

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(WHITE))
            .show_inside(ui, |ui| self.choose_body(ui));
    }

    fn cards(&self) -> Vec<Card> {
        let mut cards: Vec<Card> = self.drives.iter().enumerate().map(|(i, drive)| Card {
            icon:     if drive.removable { Icon::Card } else { Icon::Drive },
            title:    drive.name.clone(),
            detail:   format!("{} \u{00B7} {}", fmt_size(drive.size), if drive.removable { "External" } else { "Internal" }),
            selected: matches!(&self.source, Some(Source::Drive(d)) if d.path == drive.path),
            pick:     Pick::Drive(i),
        }).collect();

        let image = match &self.source {
            Some(source @ Source::Image(_)) => Card {
                icon: Icon::DiskImage, title: source.name(), detail: "Disk image".into(), selected: true, pick: Pick::Image,
            },
            _ => Card {
                icon: Icon::DiskImage, title: "Disk Image\u{2026}".into(), detail: "Choose a file".into(), selected: false, pick: Pick::Image,
            },
        };
        cards.push(image);
        cards
    }

    fn choose_body(&mut self, ui: &mut Ui) {
        let cards = self.cards();
        let width = ui.available_width() - 2.0 * MARGIN;
        let per_row = (((width + GAP) / (CARD.x + GAP)).floor() as usize).clamp(1, cards.len());
        let rows = cards.len().div_ceil(per_row);

        // Centre everything vertically.
        let error_h = if self.error.is_some() { 90.0 } else { 0.0 };
        let content_h = 36.0 + 10.0 + 22.0 + 44.0 + rows as f32 * (CARD.y + GAP) - GAP + 44.0 + 44.0 + error_h;
        let top = ((ui.available_height() - content_h) / 2.0).max(TITLEBAR + 24.0);
        ui.add_space(top);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Recover your files.").font(semibold(32.0)).color(BLACK));
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Choose where they were lost.").font(regular(17.0)).color(GRAY_1));
        });
        ui.add_space(44.0);

        let mut picked = None;
        for row in cards.chunks(per_row) {
            let row_w = row.len() as f32 * (CARD.x + GAP) - GAP;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));
                for card in row {
                    if draw_card(ui, card).clicked() {
                        picked = Some(card.pick);
                    }
                }
            });
            ui.add_space(GAP);
        }
        ui.add_space(44.0 - GAP);

        ui.vertical_centered(|ui| {
            if primary_button(ui, "Scan", vec2(200.0, 44.0), self.source.is_some()).clicked() {
                self.start_scan(ui.ctx().clone());
            }
            if let Some(error) = &self.error {
                ui.add_space(20.0);
                Frame::NONE
                    .fill(SURFACE)
                    .corner_radius(CornerRadius::same(RADIUS))
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_max_width(440.0);
                        ui.label(egui::RichText::new(error).font(regular(14.0)).color(BLACK));
                    });
            }
        });

        match picked {
            Some(Pick::Drive(i)) => {
                if let Some(drive) = self.drives.get(i).cloned() {
                    self.set_source(Source::Drive(drive));
                }
            }
            Some(Pick::Image) => self.choose_image(),
            None => {}
        }
    }

    /// "Recovered files are saved to ~/Documents/Reclaimer.  Change…"
    fn save_to_line(&mut self, ui: &mut Ui) {
        let text = format!("Recovered files are saved to {}.", pretty_path(&self.output_dir));
        let text_w = ui.painter().layout_no_wrap(text.clone(), regular(13.0), GRAY_1).size().x;
        let link_w = ui.painter().layout_no_wrap("Change\u{2026}".into(), semibold(13.0), BLACK).size().x;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(((ui.available_width() - text_w - link_w - 8.0) / 2.0).max(0.0));
            ui.label(egui::RichText::new(text).font(regular(13.0)).color(GRAY_1));
            if link(ui, "Change\u{2026}", 13.0).clicked() {
                self.choose_output_folder();
            }
        });
    }
}

fn draw_card(ui: &mut Ui, card: &Card) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(CARD, Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, card.selected, &card.title));

    let fill = if response.hovered() && !card.selected { SURFACE } else { WHITE };
    ui.painter().rect_filled(rect, CornerRadius::same(RADIUS + 4), fill);
    let (stroke_color, stroke_w) = if card.selected { (BLACK, 2.0) } else { (LINE, 1.0) };
    ui.painter().rect_stroke(rect, CornerRadius::same(RADIUS + 4), egui::Stroke::new(stroke_w, stroke_color), egui::StrokeKind::Inside);

    let icon_rect = Rect::from_center_size(rect.center_top() + vec2(0.0, 52.0), vec2(44.0, 44.0));
    icons::paint(ui.painter(), icon_rect, card.icon, BLACK);

    let max_w = rect.width() - 24.0;
    text_truncated(ui, rect.center_top() + vec2(0.0, 100.0), Align2::CENTER_TOP, &card.title, semibold(14.0), BLACK, max_w);
    text_truncated(ui, rect.center_top() + vec2(0.0, 121.0), Align2::CENTER_TOP, &card.detail, regular(12.0), GRAY_1, max_w);
    response
}
