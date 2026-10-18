//! Tauri commands exposed to the frontend.

use crate::models::{EnvironmentStatus, VideoInfo};

/// Report which external binaries were found and their versions.
#[tauri::command]
pub fn check_environment() -> Result<EnvironmentStatus, String> {
    crate::ytdlp::environment_status()
}

/// Fetch video metadata (title, thumbnail, duration, available heights)
/// for a URL without downloading anything.
#[tauri::command]
pub async fn fetch_metadata(url: String) -> Result<VideoInfo, String> {
    // Blocking yt-dlp call: run off the async runtime's main threads.
    tauri::async_runtime::spawn_blocking(move || crate::ytdlp::fetch_metadata(&url))
        .await
        .map_err(|e| format!("metadata task failed: {e}"))?
}
