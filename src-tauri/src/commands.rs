//! Tauri commands exposed to the frontend.

use tauri::{AppHandle, State};

use crate::download::DownloadManager;
use crate::models::{DownloadRequest, EnvironmentStatus, MediaInfo};

/// Report which external binaries were found and their versions.
#[tauri::command]
pub fn check_environment() -> Result<EnvironmentStatus, String> {
    crate::ytdlp::environment_status()
}

/// Fetch metadata for a URL without downloading anything. Resolves to a
/// single video (with real quality options) or a whole playlist.
#[tauri::command]
pub async fn fetch_metadata(url: String) -> Result<MediaInfo, String> {
    // Blocking yt-dlp call: run off the async runtime's main threads.
    tauri::async_runtime::spawn_blocking(move || crate::ytdlp::fetch_metadata(&url))
        .await
        .map_err(|e| format!("metadata task failed: {e}"))?
}

/// Start a download. Returns the job id; progress arrives as
/// `download-progress` events until a terminal stage
/// (`completed` | `cancelled` | `error`).
#[tauri::command]
pub fn start_download(
    app: AppHandle,
    state: State<'_, DownloadManager>,
    request: DownloadRequest,
) -> Result<String, String> {
    state.start(request, move |event| {
        use tauri::Emitter;
        app.emit(crate::download::PROGRESS_EVENT, &event).ok();
    })
}

/// Cancel a running download job.
#[tauri::command]
pub fn cancel_download(state: State<'_, DownloadManager>, job_id: String) -> Result<(), String> {
    state.cancel(&job_id)
}
