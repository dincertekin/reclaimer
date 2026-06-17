// Tauri commands from frontend

use crate::{disk, scanner};
use serde::Serialize;
use std::path::Path;
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;

/// Represents one recovered file JSON
#[derive(Serialize, Clone)]
pub struct FoundFile {
    pub file_type: String,
    pub extension: String,
    pub sector: u64,
    pub offset: u64,
    pub size_bytes: usize,
    pub filename: String,
}

/// Progress update emitted to the frontend while a scan is running
#[derive(Serialize, Clone)]
pub struct ScanProgress {
    pub scanned_sectors: u64,
    pub total_sectors: u64,
    pub found_count: u32,
}

#[tauri::command]
pub fn scan_image(app: tauri::AppHandle, image_path: String) -> Result<Vec<FoundFile>, String> {
    println!("[scan] starting scan on {}", image_path);

    let path = Path::new(&image_path);
    let output_dir = Path::new("recovered");

    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    let mut image = disk::DiskImage::open(path).map_err(|e| {
        let msg = format!("Failed to open image: {}", e);
        eprintln!("[scan] {}", msg);
        msg
    })?;

    let total_sectors = std::fs::metadata(path)
        .map(|m| m.len() / image.sector_size)
        .unwrap_or(0);

    let mut results: Vec<FoundFile> = Vec::new();
    let mut sector_number = 0u64;
    let mut found_count = 0u32;

    loop {
        match image.read_sector(sector_number) {
            Ok(sector) => {
                if let Some(sig) = scanner::detect_signature(&sector) {
                    found_count += 1;
                    println!(
                        "[scan] found {} at sector {} (offset {})",
                        sig.name,
                        sector_number,
                        sector_number * 512
                    );

                    match scanner::extract_file(
                        &sector,
                        sig,
                        &mut image,
                        sector_number,
                        output_dir,
                        found_count,
                    ) {
                        Ok(filename) => {
                            let saved_path = output_dir.join(&filename);
                            let size = std::fs::metadata(&saved_path)
                                .map(|m| m.len() as usize)
                                .unwrap_or(0);

                            results.push(FoundFile {
                                file_type: sig.name.to_string(),
                                extension: sig.extension.to_string(),
                                sector: sector_number,
                                offset: sector_number * 512,
                                size_bytes: size,
                                filename,
                            });
                        }
                        Err(e) => {
                            eprintln!("[scan] extract failed: {}", e);
                        }
                    }
                }

                sector_number += 1;

                if sector_number % 50 == 0 {
                    let _ = app.emit(
                        "scan-progress",
                        ScanProgress {
                            scanned_sectors: sector_number,
                            total_sectors,
                            found_count,
                        },
                    );
                }
            }

            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("[scan] complete. {} files found.", found_count);
                break;
            }

            Err(e) => {
                let msg = format!("Read error at sector {}: {}", sector_number, e);
                eprintln!("[scan] {}", msg);
                return Err(msg);
            }
        }
    }

    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            scanned_sectors: sector_number,
            total_sectors,
            found_count,
        },
    );

    Ok(results)
}

/// Opens a recovered file using the operating system's default
/// application for that file type
#[tauri::command]
pub fn open_file(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let path = Path::new("recovered").join(&filename);

    println!("[open] opening {}", path.display());

    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| {
            let msg = format!("Failed to open file: {}", e);
            eprintln!("[open] {}", msg);
            msg
        })
}
