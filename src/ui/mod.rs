use std::path::PathBuf;
use std::sync::mpsc;
use eframe::egui::{self, Color32, Stroke, Vec2};

mod types;
mod render;
mod scan;

use types::*;

pub struct ReclaimerApp {
    // pub(crate) makes these fields accessible to render.rs and scan.rs,
    // which add their own impl blocks to this struct in sibling modules.
    pub(crate) image_path:      Option<PathBuf>,
    pub(crate) phase:           Phase,
    pub(crate) found_files:     Vec<FoundFile>,
    pub(crate) selected_idx:    Option<usize>,
    pub(crate) active_filter:   Filter,
    pub(crate) filter_text:     String,
    pub(crate) status_text:     String,
    pub(crate) dot_state:       DotState,
    pub(crate) scanned_sectors: u64,
    pub(crate) total_sectors:   u64,
    pub(crate) found_count:     u32,
    pub(crate) scan_rx:         Option<mpsc::Receiver<ScanMsg>>,
    pub(crate) file_rx:         Option<mpsc::Receiver<Option<PathBuf>>>,
}

impl ReclaimerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_visuals(&cc.egui_ctx);
        Self {
            image_path:      None,
            phase:           Phase::Idle,
            found_files:     Vec::new(),
            selected_idx:    None,
            active_filter:   Filter::All,
            filter_text:     String::new(),
            status_text:     "Idle \u{2014} load a disk image to begin".into(),
            dot_state:       DotState::Idle,
            scanned_sectors: 0,
            total_sectors:   0,
            found_count:     0,
            scan_rx:         None,
            file_rx:         None,
        }
    }

    pub(crate) fn count(&self, filter: &Filter) -> usize {
        self.found_files.iter().filter(|file| match filter {
            Filter::All       => true,
            Filter::Images    => file.category == Category::Image,
            Filter::Documents => file.category == Category::Document,
            Filter::Media     => file.category == Category::Media,
            Filter::Other     => file.category == Category::Other,
        }).count()
    }

    pub(crate) fn visible_files(&self) -> Vec<(usize, &FoundFile)> {
        let query = self.filter_text.to_lowercase();
        self.found_files
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                let matches_category = match self.active_filter {
                    Filter::All       => true,
                    Filter::Images    => file.category == Category::Image,
                    Filter::Documents => file.category == Category::Document,
                    Filter::Media     => file.category == Category::Media,
                    Filter::Other     => file.category == Category::Other,
                };
                if !matches_category { return false; }
                if !query.is_empty() {
                    return file.filename.to_lowercase().contains(&query)
                        || file.extension.to_lowercase().contains(&query);
                }
                true
            })
            .collect()
    }

    fn open_file_dialog(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.file_rx = Some(rx);
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .add_filter("Disk Images", &["img", "dd", "raw", "iso", "bin", "dmg"])
                .add_filter("All Files", &["*"])
                .pick_file();
            let _ = tx.send(path);
        });
    }

    fn load_image(&mut self, path: PathBuf) {
        self.image_path   = Some(path);
        self.found_files  = Vec::new();
        self.selected_idx = None;
        self.phase        = Phase::Ready;
        self.status_text  = "Image loaded \u{2014} ready to scan".into();
        self.dot_state    = DotState::Idle;
    }

    fn open_result(&self, idx: usize) {
        if let Some(file) = self.found_files.get(idx) {
            let _ = open::that(PathBuf::from("recovered").join(&file.filename));
        }
    }

    fn poll_channels(&mut self, ctx: &egui::Context) {
        // egui accumulates dropped files in input().raw.dropped_files each frame.
        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.load_image(path);
        }

        if let Some(ref rx) = self.file_rx
            && let Ok(maybe_path) = rx.try_recv()
        {
            self.file_rx = None;
            if let Some(path) = maybe_path {
                self.load_image(path);
            }
        }

        let mut msgs = Vec::new();
        if let Some(ref rx) = self.scan_rx {
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
        }

        for msg in msgs {
            match msg {
                ScanMsg::Progress { scanned, total, found } => {
                    self.scanned_sectors = scanned;
                    self.total_sectors   = total;
                    self.found_count     = found;
                    if total > 0 {
                        self.status_text = format!(
                            "Scanning sector {} / {}  \u{2014}  {} found",
                            render::fmt_num(scanned),
                            render::fmt_num(total),
                            found
                        );
                    }
                }
                ScanMsg::Found(file) => {
                    self.found_files.push(file);
                }
                ScanMsg::Done => {
                    self.phase     = Phase::Done;
                    self.scan_rx   = None;
                    let count      = self.found_files.len();
                    self.status_text = if count == 0 {
                        "Scan complete \u{2014} no artifacts found".into()
                    } else {
                        format!("Scan complete \u{2014} {} artifact{} recovered", count, if count == 1 { "" } else { "s" })
                    };
                    self.dot_state = DotState::Done;
                }
                ScanMsg::Error(e) => {
                    self.phase       = Phase::Done;
                    self.scan_rx     = None;
                    self.status_text = format!("Error: {e}");
                    self.dot_state   = DotState::Error;
                }
            }
        }

        if self.phase == Phase::Scanning {
            ctx.request_repaint();
        }
    }
}

impl eframe::App for ReclaimerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));

        self.poll_channels(ui.ctx());

        // All draw_* methods take &self (immutable). Actions are collected as
        // return values and applied to &mut self after all rendering is done.
        let open_clicked = egui::Panel::top("toolbar")
            .exact_size(48.0)
            .frame(egui::Frame::NONE.fill(SIDEBAR_BG))
            .show_inside(ui, |ui| self.draw_toolbar(ui))
            .inner;

        egui::Panel::bottom("statusbar")
            .exact_size(34.0)
            .frame(egui::Frame::NONE.fill(SIDEBAR_BG))
            .show_inside(ui, |ui| self.draw_statusbar(ui));

        let (filter_change, result_action) = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG))
            .show_inside(ui, |ui| self.draw_workspace(ui))
            .inner;

        // Drag-over overlay — rendered last so it sits on top of all other panels.
        if ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
            let window_rect = ui.max_rect();
            ui.painter().rect_filled(window_rect, 0.0, Color32::from_rgba_unmultiplied(96, 165, 250, 18));
            ui.painter().rect_stroke(
                window_rect.shrink(3.0),
                egui::CornerRadius::ZERO,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                window_rect.center() + Vec2::new(0.0, 40.0),
                egui::Align2::CENTER_CENTER,
                "Drop to load image",
                egui::FontId::proportional(20.0),
                Color32::from_rgba_unmultiplied(96, 165, 250, 220),
            );
        }

        if open_clicked { self.open_file_dialog(); }
        if let Some(new_filter) = filter_change {
            self.active_filter = new_filter;
            self.selected_idx  = None;
        }
        if let Some(action) = result_action {
            match action {
                ResultAction::Select(idx) => {
                    // Second click on an already-selected row deselects it.
                    self.selected_idx = if self.selected_idx == Some(idx) { None } else { Some(idx) };
                }
                ResultAction::StartScan      => self.start_scan(ui.ctx().clone()),
                ResultAction::Open(idx)      => self.open_result(idx),
                ResultAction::CopyPath(idx)  => {
                    if let Some(file) = self.found_files.get(idx) {
                        ui.ctx().copy_text(format!("recovered/{}", file.filename));
                    }
                }
            }
        }
    }
}

fn setup_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    style.visuals.dark_mode  = true;
    style.visuals.window_fill = BG;
    style.visuals.panel_fill  = PANEL;

    style.visuals.widgets.noninteractive.bg_fill   = PANEL;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SEC);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    style.visuals.widgets.inactive.bg_fill   = PANEL_RAISED;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SEC);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_MID);

    style.visuals.widgets.hovered.bg_fill   = Color32::from_rgb(37, 52, 82);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_MID);

    style.visuals.widgets.active.bg_fill   = Color32::from_rgba_unmultiplied(96, 165, 250, 30);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(96, 165, 250, 30);
    style.visuals.selection.stroke  = Stroke::new(1.0, ACCENT);

    style.visuals.window_shadow   = egui::Shadow::NONE;
    style.spacing.item_spacing    = Vec2::new(6.0, 4.0);
    style.spacing.button_padding  = Vec2::new(16.0, 10.0);

    ctx.set_global_style(style);
}
