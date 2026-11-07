//! What kind of file something is, from its extension. Shared by the
//! recovery code (which files on a phone are worth copying) and the UI
//! (filters, icons).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Photos,
    Media,
    Documents,
    Other,
}

impl Category {
    pub const ALL: [Category; 4] = [Category::Photos, Category::Media, Category::Documents, Category::Other];

    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "avif" | "webp" | "bmp" | "tif" | "tiff"
            | "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2" => Category::Photos,
            "mp4" | "mov" | "m4v" | "avi" | "mkv" | "wmv" | "3gp" | "mts" | "m2ts"
            | "m4a" | "mp3" | "wav" | "aac" | "flac" | "ogg" | "wma" => Category::Media,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf"
            | "txt" | "md" | "csv" | "pages" | "numbers" | "key" | "epub" => Category::Documents,
            _ => Category::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Photos    => "Photos",
            Category::Media     => "Videos & Audio",
            Category::Documents => "Documents",
            Category::Other     => "Other",
        }
    }
}

/// Photos, videos, audio and documents: the files people want back.
pub fn is_personal(ext: &str) -> bool {
    Category::from_ext(ext) != Category::Other
}
