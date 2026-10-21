//! Types shared across the Tauri command boundary with the frontend.

use serde::{Deserialize, Serialize};

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

/// A single video inside a playlist (from --flat-playlist extraction).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub id: String,
    pub title: String,
    pub duration_seconds: Option<f64>,
    pub url: Option<String>,
}

/// Playlist metadata extracted via `yt-dlp -J --flat-playlist`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub video_count: usize,
    pub entries: Vec<PlaylistEntry>,
}

/// What a URL points at: a single video or a whole playlist.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MediaInfo {
    Video(VideoInfo),
    Playlist(PlaylistInfo),
}

/// What to download, as chosen in the resolution dropdown.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Resolution {
    /// yt-dlp's default: best video + best audio (bv*+ba/b).
    Best,
    /// Best video no better than Np: `-S res:N`.
    Height { height: u32 },
    /// Audio only: `-t mp3` preset alias.
    AudioMp3,
}

/// Parameters for a single download job.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub resolution: Resolution,
    /// Destination directory; defaults to ~/Downloads when absent.
    pub output_dir: Option<String>,
    /// Download the whole playlist the URL points at, into a subfolder
    /// named after the playlist. When false, `--no-playlist` is passed.
    #[serde(default)]
    pub playlist: bool,
}

/// Progress/lifecycle event emitted on `download-progress`.
///
/// Stages:
/// - `downloading` / `stream-finished`: per-stream progress from yt-dlp
///   (a merged download downloads the video stream, then the audio stream).
/// - `completed`: process exited successfully; `filepath` is the final file.
/// - `cancelled`: the user cancelled; `filepath` is absent.
/// - `error`: yt-dlp failed; `error` holds the stderr tail.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: String,
    pub stage: String,
    pub percent: Option<String>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub filepath: Option<String>,
    pub error: Option<String>,
    /// Which video of a playlist is currently downloading (1-based).
    pub playlist_index: Option<u32>,
    /// Total videos in the playlist, when downloading one.
    pub playlist_count: Option<u32>,
    /// On a `completed` event after a playlist run where yt-dlp exited
    /// non-zero (some entries skipped): how many files were saved.
    pub partial_downloaded: Option<u32>,
    /// How many entries the playlist had in that case.
    pub partial_total: Option<u32>,
}

/// Whether a required tool comes from the app's managed downloads, a
/// system install, or is absent entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolPresence {
    Managed,
    System,
    Missing,
}

/// Presence of every tool Forvid needs, plus where managed tools live.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub dir: String,
    pub ytdlp: ToolPresence,
    pub ffmpeg: ToolPresence,
    pub ffprobe: ToolPresence,
    pub deno: ToolPresence,
    /// True when the app cannot run without first downloading tools.
    pub needs_setup: bool,
}

/// Progress for one tool being downloaded/installed (event: tools-progress).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsProgress {
    pub tool: String,
    /// downloading | installing | done | error
    pub stage: String,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

/// A newer Forvid release available on GitHub, when one exists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    /// Version string of the newer release, e.g. "0.2.0".
    pub version: String,
    /// Release page URL (fallback when no asset matches this device).
    pub url: String,
    /// Direct download URL of the asset matching this device, if any.
    pub asset_url: Option<String>,
    /// File name of that asset, e.g. "Forvid_0.3.0_universal.dmg".
    pub asset_name: Option<String>,
}

/// Progress for an app-update download (event: app-update-progress).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}
