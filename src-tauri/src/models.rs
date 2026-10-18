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
