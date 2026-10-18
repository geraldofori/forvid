//! Resolution of the external binaries Forvid depends on, and construction of
//! the process environment they need.
//!
//! # The Finder PATH trap
//!
//! Apps launched from Finder (or Spotlight) on macOS inherit a minimal PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`) that does NOT include Homebrew's
//! `/opt/homebrew/bin`. Resolving binaries via PATH lookup alone therefore
//! works in `tauri dev` from a terminal but fails for the installed app.
//!
//! The fix applied here:
//! 1. Binaries are resolved by probing well-known absolute paths.
//! 2. Spawned children receive a repaired PATH (see [`Binaries::child_path`])
//!    so yt-dlp can in turn find its own runtime dependencies: ffmpeg
//!    (merging) and deno (JavaScript runtime required by yt-dlp-ejs for
//!    full YouTube support).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories probed for binaries, highest priority first.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs
}

fn find_binary(name: &str) -> Option<PathBuf> {
    candidate_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Resolved locations of everything needed to run yt-dlp correctly.
#[derive(Debug, Clone)]
pub struct Binaries {
    pub ytdlp: PathBuf,
    /// Directory containing ffmpeg, passed to yt-dlp via `--ffmpeg-location`.
    pub ffmpeg_dir: Option<PathBuf>,
    /// Directory containing deno, added to the child PATH (yt-dlp-ejs).
    pub deno_dir: Option<PathBuf>,
}

impl Binaries {
    /// Resolve all binaries. Fails only if yt-dlp itself is missing;
    /// ffmpeg and deno are optional (yt-dlp degrades gracefully, but we
    /// surface their absence in [`crate::models::EnvironmentStatus`]).
    pub fn resolve() -> Result<Self, String> {
        let ytdlp = find_binary("yt-dlp").ok_or_else(|| {
            "yt-dlp was not found. Install it with: brew install yt-dlp ffmpeg".to_string()
        })?;
        let ffmpeg_dir = find_binary("ffmpeg").and_then(|p| p.parent().map(Path::to_path_buf));
        let deno_dir = find_binary("deno").and_then(|p| p.parent().map(Path::to_path_buf));
        Ok(Self {
            ytdlp,
            ffmpeg_dir,
            deno_dir,
        })
    }

    /// PATH value for spawned yt-dlp children, guaranteeing Homebrew tools
    /// are visible even under a Finder-launched minimal environment.
    pub fn child_path(&self) -> String {
        let mut dirs: Vec<String> = Vec::new();
        for dir in [&self.ffmpeg_dir, &self.deno_dir].into_iter().flatten() {
            dirs.push(dir.to_string_lossy().into_owned());
        }
        if let Some(parent) = self.ytdlp.parent() {
            dirs.push(parent.to_string_lossy().into_owned());
        }
        if let Ok(existing) = std::env::var("PATH") {
            dirs.push(existing);
        }
        dirs.push("/usr/bin:/bin:/usr/sbin:/sbin".to_string());
        dirs.join(":")
    }
}

/// Run a binary with args and return the first line of stdout, if it
/// executed successfully. Used for version probes.
pub fn probe_version(binary: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(binary).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .next()
        .map(str::to_owned)
}

/// Reject anything that is not an http(s) URL before it reaches a process.
/// URLs are always passed to yt-dlp as an argument array entry (never
/// through a shell), so this is defense-in-depth rather than injection
/// prevention.
fn validate_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(())
    } else {
        Err("URL must start with http:// or https://".to_string())
    }
}

/// Last non-empty stderr lines, for surfacing yt-dlp failures to the UI.
fn stderr_tail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = tail.len().saturating_sub(6);
    tail[start..].join("\n")
}

/// Fetch metadata for a video URL without downloading it.
///
/// Uses `yt-dlp -j` (--dump-json), which prints the full info JSON and
/// implies --simulate. Playlists are rejected in v1 (`--no-playlist`).
pub fn fetch_metadata(url: &str) -> Result<crate::models::VideoInfo, String> {
    let url = url.trim();
    validate_url(url)?;

    let bins = Binaries::resolve()?;
    let output = Command::new(&bins.ytdlp)
        .args(["-j", "--no-playlist", "--no-warnings", url])
        .env("PATH", bins.child_path())
        .output()
        .map_err(|e| format!("failed to spawn yt-dlp: {e}"))?;

    if !output.status.success() {
        return Err(format!("yt-dlp failed:\n{}", stderr_tail(&output)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse yt-dlp metadata: {e}"))?;
    parse_video_info(&json)
}

/// Reduce yt-dlp's info JSON to the fields the UI uses.
fn parse_video_info(v: &serde_json::Value) -> Result<crate::models::VideoInfo, String> {
    let str_field = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_owned);

    let id = str_field("id").ok_or("yt-dlp returned metadata without an id")?;
    let title = str_field("title").unwrap_or_else(|| "Unknown title".to_string());

    // Heights of formats that actually contain a video stream, deduped.
    let mut heights: Vec<u32> = v
        .get("formats")
        .and_then(|f| f.as_array())
        .map(|formats| {
            formats
                .iter()
                .filter(|f| f.get("vcodec").and_then(|c| c.as_str()) != Some("none"))
                .filter_map(|f| f.get("height").and_then(|h| h.as_u64()))
                .map(|h| h as u32)
                .collect()
        })
        .unwrap_or_default();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();

    Ok(crate::models::VideoInfo {
        id,
        title,
        thumbnail: str_field("thumbnail"),
        duration_seconds: v.get("duration").and_then(|d| d.as_f64()),
        duration_string: str_field("duration_string"),
        uploader: str_field("uploader").or_else(|| str_field("channel")),
        webpage_url: str_field("webpage_url").unwrap_or_default(),
        available_heights: heights,
    })
}

/// Collect the full environment report for the frontend.
pub fn environment_status() -> Result<crate::models::EnvironmentStatus, String> {
    let bins = Binaries::resolve()?;

    let ytdlp_version = probe_version(&bins.ytdlp, &["--version"]);
    let (ffmpeg_path, ffmpeg_version) = match &bins.ffmpeg_dir {
        Some(dir) => {
            let path = dir.join("ffmpeg");
            (
                Some(path.to_string_lossy().into_owned()),
                probe_version(&path, &["-version"]),
            )
        }
        None => (None, None),
    };
    let (deno_path, deno_version) = match &bins.deno_dir {
        Some(dir) => {
            let path = dir.join("deno");
            (
                Some(path.to_string_lossy().into_owned()),
                probe_version(&path, &["--version"]),
            )
        }
        None => (None, None),
    };

    Ok(crate::models::EnvironmentStatus {
        ytdlp_path: Some(bins.ytdlp.to_string_lossy().into_owned()),
        ytdlp_version,
        ffmpeg_path,
        ffmpeg_version,
        deno_path,
        deno_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_ytdlp_on_this_machine() {
        let bins = Binaries::resolve().expect("yt-dlp must be installed");
        assert!(bins.ytdlp.is_file());
    }

    #[test]
    fn child_path_contains_homebrew() {
        let bins = Binaries {
            ytdlp: PathBuf::from("/opt/homebrew/bin/yt-dlp"),
            ffmpeg_dir: Some(PathBuf::from("/opt/homebrew/bin")),
            deno_dir: Some(PathBuf::from("/opt/homebrew/bin")),
        };
        assert!(bins.child_path().contains("/opt/homebrew/bin"));
    }

    #[test]
    fn reports_versions() {
        let status = environment_status().expect("environment must resolve");
        assert_eq!(status.ytdlp_version.is_some(), true);
        assert_eq!(status.ffmpeg_version.is_some(), true);
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/x").is_err());
        assert!(validate_url("not a url").is_err());
        assert!(validate_url("https://www.youtube.com/watch?v=abc").is_ok());
    }

    #[test]
    fn parses_video_info_from_ytdlp_json() {
        let json = serde_json::json!({
            "id": "jNQXAC9IVRw",
            "title": "Me at the zoo",
            "thumbnail": "https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg",
            "duration": 19.0,
            "duration_string": "19",
            "uploader": "jawed",
            "webpage_url": "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            "formats": [
                {"format_id": "18", "height": 360, "vcodec": "avc1.42001E"},
                {"format_id": "394", "height": 144, "vcodec": "av01.0.00M.10"},
                {"format_id": "233", "vcodec": "none", "acodec": "mp4a.40.2"},
                {"format_id": "17", "height": 144, "vcodec": "mp4v.20.3"},
                {"format_id": "136", "height": 720, "vcodec": "avc1.4d401f"}
            ]
        });

        let info = parse_video_info(&json).expect("fixture must parse");
        assert_eq!(info.id, "jNQXAC9IVRw");
        assert_eq!(info.title, "Me at the zoo");
        assert_eq!(info.uploader.as_deref(), Some("jawed"));
        // Audio-only format excluded, duplicate 144 deduped, descending order
        assert_eq!(info.available_heights, vec![720, 360, 144]);
    }

    /// Live smoke test against YouTube. Ignored by default; run with:
    /// cargo test -- --ignored
    #[test]
    #[ignore = "hits the network; run explicitly"]
    fn fetches_metadata_live() {
        let info = fetch_metadata("https://www.youtube.com/watch?v=jNQXAC9IVRw")
            .expect("live metadata fetch must succeed");
        assert_eq!(info.id, "jNQXAC9IVRw");
        assert!(!info.available_heights.is_empty());
    }
}
