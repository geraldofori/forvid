//! Types shared across the Tauri command boundary with the frontend.

use serde::Serialize;

/// Availability and versions of the external binaries Forvid shells out to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatus {
    pub ytdlp_path: Option<String>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub ffmpeg_version: Option<String>,
    pub deno_path: Option<String>,
    pub deno_version: Option<String>,
}

/// Video metadata extracted via `yt-dlp -j`, reduced to what the UI needs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration_seconds: Option<f64>,
    pub duration_string: Option<String>,
    pub uploader: Option<String>,
    pub webpage_url: String,
    /// Distinct video heights actually offered for this video, descending.
    pub available_heights: Vec<u32>,
}
