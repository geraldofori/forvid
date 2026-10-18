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
}
