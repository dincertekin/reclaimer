use eframe::egui::Color32;

// Dim-slate palette: comfortable at any hour without being harsh at midnight
// or washed out under bright daylight. Backgrounds sit at ~8-15% luminosity;
// text at ~85-90% — contrast ratio ≈ 8:1 across the board.
pub const BG:           Color32 = Color32::from_rgb( 17,  24,  39); // #111827 gray-900
pub const PANEL:        Color32 = Color32::from_rgb( 26,  34,  54); // #1A2236
pub const PANEL_RAISED: Color32 = Color32::from_rgb( 30,  42,  66); // #1E2A42
pub const SIDEBAR_BG:   Color32 = Color32::from_rgb( 13,  21,  37); // #0D1525
pub const ACCENT:       Color32 = Color32::from_rgb( 96, 165, 250); // #60A5FA blue-400
pub const AMBER:        Color32 = Color32::from_rgb(251, 191,  36); // #FBBF24 amber-400
pub const TEXT:         Color32 = Color32::from_rgb(226, 232, 240); // #E2E8F0 slate-200
pub const TEXT_SEC:     Color32 = Color32::from_rgb(148, 163, 184); // #94A3B8 slate-400
pub const TEXT_DIM:     Color32 = Color32::from_rgb( 71,  85, 105); // #475569 slate-600
pub const GREEN:        Color32 = Color32::from_rgb( 74, 222, 128); // #4ADE80 green-400
pub const PURPLE:       Color32 = Color32::from_rgb(167, 139, 250); // #A78BFA violet-400
pub const DANGER:       Color32 = Color32::from_rgb(248, 113, 113); // #F87171 red-400
pub const BORDER:       Color32 = Color32::from_rgb( 30,  41,  59); // #1E293B slate-800
pub const BORDER_MID:   Color32 = Color32::from_rgb( 51,  65,  85); // #334155 slate-700

#[derive(Clone, Debug)]
pub struct FoundFile {
    pub file_type:  String,
    pub extension:  String,
    pub sector:     u64,
    pub offset:     u64,
    pub size_bytes: usize,
    pub filename:   String,
    pub category:   Category,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Category {
    Image,
    Document,
    Media,
    Other,
}

impl Category {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "heic" | "raw" | "cr2"
            | "nef" | "arw" | "dng" => Category::Image,
            "mp4" | "mov" | "avi" | "mkv" | "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a"
            | "m4v" | "wma" | "wmv" | "3gp" => Category::Media,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt"
            | "csv" | "pages" | "numbers" => Category::Document,
            _ => Category::Other,
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Category::Image    => AMBER,
            Category::Document => PURPLE,
            Category::Media    => GREEN,
            Category::Other    => TEXT_SEC,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Filter { All, Images, Documents, Media, Other }

#[derive(Clone, PartialEq, Debug)]
pub enum Phase { Idle, Ready, Scanning, Done }

pub enum ScanMsg {
    Progress { scanned: u64, total: u64, found: u32 },
    Found(FoundFile),
    Done,
    Error(String),
}

#[derive(Clone, PartialEq)]
pub enum DotState { Idle, Scanning, Done, Error }

impl DotState {
    pub fn color(&self) -> Color32 {
        match self {
            DotState::Idle     => TEXT_DIM,
            DotState::Scanning => ACCENT,
            DotState::Done     => GREEN,
            DotState::Error    => DANGER,
        }
    }
}

pub enum ResultAction {
    Select(usize),
    Open(usize),
    CopyPath(usize),
    StartScan,
}
