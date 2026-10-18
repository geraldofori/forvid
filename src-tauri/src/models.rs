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
}
