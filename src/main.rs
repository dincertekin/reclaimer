mod commands;
mod disk;
mod scanner;

fn main() {
    println!("[reclaimer] starting up");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::scan_image,
            commands::open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
