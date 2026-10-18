mod commands;
mod download;
mod models;
mod ytdlp;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .manage(download::DownloadManager::default())
        .invoke_handler(tauri::generate_handler![
            commands::check_environment,
            commands::fetch_metadata,
            commands::start_download,
            commands::cancel_download
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
