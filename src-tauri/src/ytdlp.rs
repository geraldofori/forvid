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
///
/// PATH is searched first (covers most Linux/Windows installs and any
/// terminal use). OS-specific well-known locations follow, because apps
/// launched from a GUI (Finder, Start menu, .desktop) can inherit a
/// minimal PATH that hides package-manager binaries.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/snap/bin"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            // `winget install yt-dlp.yt-dlp` links land here
            dirs.push(PathBuf::from(&local).join(r"Microsoft\WinGet\Links"));
        }
        dirs.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
    }

    if let Some(home) = std::env::home_dir() {
        #[cfg(not(windows))]
        dirs.push(home.join(".local").join("bin"));
        #[cfg(windows)]
        dirs.push(home.join("scoop").join("shims"));
    }
    dirs
}

/// Platform-specific executable names, tried in order (Windows has .exe).
pub(crate) fn binary_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![format!("{name}.exe"), name.to_string()]
    }
    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}

pub(crate) fn find_binary(name: &str) -> Option<PathBuf> {
    candidate_dirs()
        .into_iter()
        .flat_map(|dir| binary_names(name).into_iter().map(move |n| dir.join(n)))
        .find(|path| path.is_file())
}

/// Per-OS install guidance shown when yt-dlp is missing.
#[cfg(target_os = "macos")]
const INSTALL_HINT: &str = "brew install yt-dlp ffmpeg";
#[cfg(target_os = "linux")]
const INSTALL_HINT: &str =
    "pip install yt-dlp && sudo apt install ffmpeg  (or your distro's packages)";
#[cfg(target_os = "windows")]
const INSTALL_HINT: &str = "winget install yt-dlp.yt-dlp && winget install Gyan.FFmpeg";

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
    /// Resolve all binaries, preferring system installs (faster startup —
    /// the managed yt-dlp is a PyInstaller binary that unpacks itself on
    /// every invocation) and falling back to the app's managed downloads.
    /// Fails only if yt-dlp is missing from both; ffmpeg and deno are
    /// optional (yt-dlp degrades gracefully, but we surface their absence
    /// in [`crate::models::EnvironmentStatus`]).
    pub fn resolve() -> Result<Self, String> {
        let managed = crate::tools::tools_dir();
        let find = |name: &str| find_binary(name).or_else(|| crate::tools::find_in(&managed, name));
        let ytdlp = find("yt-dlp")
            .ok_or_else(|| format!("yt-dlp was not found. Install it with: {INSTALL_HINT}"))?;
        let ffmpeg_dir = find("ffmpeg").and_then(|p| p.parent().map(Path::to_path_buf));
        let deno_dir = find("deno").and_then(|p| p.parent().map(Path::to_path_buf));
        Ok(Self {
            ytdlp,
            ffmpeg_dir,
            deno_dir,
        })
    }

    /// PATH value for spawned yt-dlp children, guaranteeing the resolved
    /// tools (and base system dirs) are visible even under a GUI-launched
    /// minimal environment. Uses the OS path separator via `join_paths`.
    pub fn child_path(&self) -> String {
        let mut dirs: Vec<PathBuf> = Vec::new();
        for dir in [&self.ffmpeg_dir, &self.deno_dir].into_iter().flatten() {
            dirs.push(dir.clone());
        }
        if let Some(parent) = self.ytdlp.parent() {
            dirs.push(parent.to_path_buf());
        }
        if let Ok(existing) = std::env::var("PATH") {
            dirs.extend(std::env::split_paths(&existing));
        }
        #[cfg(not(windows))]
        for base in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            dirs.push(PathBuf::from(base));
        }
        #[cfg(windows)]
        if let Ok(sysroot) = std::env::var("SystemRoot") {
            dirs.push(PathBuf::from(sysroot).join("System32"));
        }
        std::env::join_paths(dirs)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
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
pub(crate) fn validate_url(url: &str) -> Result<(), String> {
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

/// Fetch metadata for a URL without downloading it.
///
/// Uses `yt-dlp -J --flat-playlist`: one JSON document for the whole URL.
/// Single videos are fully extracted (formats included, so quality options
/// are real); playlist URLs return their entries without per-video
/// extraction, which keeps even large playlists fast.
pub fn fetch_metadata(url: &str) -> Result<crate::models::MediaInfo, String> {
    let url = url.trim();
    validate_url(url)?;

    let bins = Binaries::resolve()?;
    let output = Command::new(&bins.ytdlp)
        .args(["-J", "--flat-playlist", "--no-warnings", url])
        .env("PATH", bins.child_path())
        .output()
        .map_err(|e| format!("failed to spawn yt-dlp: {e}"))?;

    if !output.status.success() {
        return Err(format!("yt-dlp failed:\n{}", stderr_tail(&output)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse yt-dlp metadata: {e}"))?;

    if json.get("_type").and_then(|t| t.as_str()) == Some("playlist") {
        parse_playlist_info(&json).map(crate::models::MediaInfo::Playlist)
    } else {
        parse_video_info(&json).map(crate::models::MediaInfo::Video)
    }
}

/// Reduce yt-dlp's flat-playlist JSON to the fields the UI uses.
fn parse_playlist_info(v: &serde_json::Value) -> Result<crate::models::PlaylistInfo, String> {
    let str_field = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_owned);

    let entries: Vec<crate::models::PlaylistEntry> = v
        .get("entries")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    let field = |key: &str| e.get(key).and_then(|x| x.as_str()).map(str::to_owned);
                    crate::models::PlaylistEntry {
                        id: field("id").unwrap_or_default(),
                        title: field("title").unwrap_or_else(|| "Unknown title".to_string()),
                        duration_seconds: e.get("duration").and_then(|d| d.as_f64()),
                        url: field("url").or_else(|| field("webpage_url")),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let video_count = entries.len();
    Ok(crate::models::PlaylistInfo {
        id: str_field("id").unwrap_or_default(),
        title: str_field("title").unwrap_or_else(|| "Untitled playlist".to_string()),
        uploader: str_field("uploader").or_else(|| str_field("channel")),
        thumbnail: str_field("thumbnail"),
        video_count,
        entries,
    })
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
    fn parses_playlist_info_from_flat_json() {
        let json = serde_json::json!({
            "_type": "playlist",
            "id": "PLabc123",
            "title": "My Playlist",
            "uploader": "by Some Channel",
            "thumbnail": null,
            "entries": [
                {"_type": "url", "id": "kPa7bsKwL-c", "title": "Song One",
                 "url": "https://www.youtube.com/watch?v=kPa7bsKwL-c", "duration": 210.0},
                {"_type": "url", "id": "abc123def45", "title": "Song Two",
                 "url": "https://www.youtube.com/watch?v=abc123def45"}
            ]
        });

        let info = parse_playlist_info(&json).expect("fixture must parse");
        assert_eq!(info.id, "PLabc123");
        assert_eq!(info.title, "My Playlist");
        assert_eq!(info.uploader.as_deref(), Some("by Some Channel"));
        assert_eq!(info.video_count, 2);
        assert_eq!(info.entries[0].duration_seconds, Some(210.0));
        assert_eq!(
            info.entries[0].url.as_deref(),
            Some("https://www.youtube.com/watch?v=kPa7bsKwL-c")
        );
        assert_eq!(info.entries[1].duration_seconds, None);
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
        let crate::models::MediaInfo::Video(video) = info else {
            panic!("a watch URL must resolve to a video");
        };
        assert_eq!(video.id, "jNQXAC9IVRw");
        assert!(!video.available_heights.is_empty());
    }

    /// Live playlist smoke test. Ignored by default.
    #[test]
    #[ignore = "hits the network; run explicitly"]
    fn fetches_playlist_live() {
        let info = fetch_metadata(
            "https://www.youtube.com/playlist?list=PLMC9KNkIncKtPzgY-5rmhvj7fax8fdxoj",
        )
        .expect("live playlist fetch must succeed");
        let crate::models::MediaInfo::Playlist(playlist) = info else {
            panic!("a playlist URL must resolve to a playlist");
        };
        assert!(playlist.video_count > 1);
        assert!(!playlist.title.is_empty());
    }
}
