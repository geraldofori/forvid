//! App-managed download tools.
//!
//! Forvid can run fully self-sufficiently: on first launch it downloads
//! yt-dlp, ffmpeg/ffprobe and deno into its own tools directory and prefers
//! those over system installs, so non-technical users never touch a package
//! manager. Binaries are downloaded directly from their official sources
//! (GitHub releases for yt-dlp/deno, martin-riedl.de static builds for
//! macOS ffmpeg) and are therefore not quarantined by Gatekeeper.
//!
//! Linux note: only yt-dlp is auto-managed (reliable static ffmpeg tarballs
//! for Linux are not zip-based); ffmpeg/deno are expected from the distro.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::models::{ToolPresence, ToolStatus, ToolsProgress};
use crate::ytdlp::{binary_names, find_binary};

/// Tauri event emitted while tools are being set up.
pub const TOOLS_EVENT: &str = "tools-progress";

/// Directory where managed tools are installed.
/// `FORVID_TOOLS_DIR` overrides it (used by tests).
pub fn tools_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("FORVID_TOOLS_DIR") {
        return PathBuf::from(custom);
    }
    default_tools_dir(std::env::home_dir())
}

fn default_tools_dir(home: Option<PathBuf>) -> PathBuf {
    let home = home.unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Application Support")
            .join("forvid")
            .join("bin")
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("forvid")
            .join("bin")
    }
    #[cfg(target_os = "linux")]
    {
        home.join(".local").join("share").join("forvid").join("bin")
    }
}

/// Find a binary by name inside one specific directory.
pub(crate) fn find_in(dir: &Path, name: &str) -> Option<PathBuf> {
    binary_names(name)
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// One downloadable tool package. A package may be a raw binary or a zip
/// whose members are matched by file name (so archive-internal folder
/// layouts don't matter).
struct ToolSpec {
    id: &'static str,
    url: &'static str,
    zip: bool,
    targets: &'static [&'static str],
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            id: "yt-dlp",
            url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
            zip: false,
            targets: &["yt-dlp"],
        },
        ToolSpec {
            id: "ffmpeg",
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip",
            zip: true,
            targets: &["ffmpeg"],
        },
        ToolSpec {
            id: "ffprobe",
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffprobe.zip",
            zip: true,
            targets: &["ffprobe"],
        },
        ToolSpec {
            id: "deno",
            url: "https://github.com/denoland/deno/releases/latest/download/deno-aarch64-apple-darwin.zip",
            zip: true,
            targets: &["deno"],
        },
    ]
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            id: "yt-dlp",
            url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
            zip: false,
            targets: &["yt-dlp"],
        },
        ToolSpec {
            id: "ffmpeg",
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/x64/release/ffmpeg.zip",
            zip: true,
            targets: &["ffmpeg"],
        },
        ToolSpec {
            id: "ffprobe",
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/x64/release/ffprobe.zip",
            zip: true,
            targets: &["ffprobe"],
        },
        ToolSpec {
            id: "deno",
            url: "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-apple-darwin.zip",
            zip: true,
            targets: &["deno"],
        },
    ]
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            id: "yt-dlp",
            url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
            zip: false,
            targets: &["yt-dlp.exe"],
        },
        // One zip contains both ffmpeg.exe and ffprobe.exe under bin/.
        ToolSpec {
            id: "ffmpeg",
            url: "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
            zip: true,
            targets: &["ffmpeg.exe", "ffprobe.exe"],
        },
        ToolSpec {
            id: "deno",
            url: "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip",
            zip: true,
            targets: &["deno.exe"],
        },
    ]
}

#[cfg(target_os = "linux")]
fn tool_specs() -> Vec<ToolSpec> {
    // ffmpeg/deno are expected from the distro on Linux (no reliable
    // zip-based static ffmpeg source); only yt-dlp is auto-managed.
    vec![ToolSpec {
        id: "yt-dlp",
        url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
        zip: false,
        targets: &["yt-dlp"],
    }]
}

/// Report which tools are available and from where (managed vs system).
pub fn tools_status() -> ToolStatus {
    let dir = tools_dir();
    let presence = |name: &str| -> ToolPresence {
        if find_in(&dir, name).is_some() {
            ToolPresence::Managed
        } else if find_binary(name).is_some() {
            ToolPresence::System
        } else {
            ToolPresence::Missing
        }
    };

    let ytdlp = presence("yt-dlp");
    let ffmpeg = presence("ffmpeg");
    let ffprobe = presence("ffprobe");
    let deno = presence("deno");

    let needs_setup = matches!(ytdlp, ToolPresence::Missing)
        || matches!(ffmpeg, ToolPresence::Missing)
        // deno is auto-managed on macOS/Windows, so require it there.
        || (cfg!(not(target_os = "linux")) && matches!(deno, ToolPresence::Missing));

    ToolStatus {
        dir: dir.to_string_lossy().into_owned(),
        ytdlp,
        ffmpeg,
        ffprobe,
        deno,
        needs_setup,
    }
}

/// Download and install every missing managed tool, emitting progress.
pub fn setup_tools(emit: impl Fn(ToolsProgress) + Send + Sync + 'static) -> Result<(), String> {
    let dir = tools_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    for spec in tool_specs() {
        if spec.targets.iter().all(|t| find_in(&dir, t).is_some()) {
            emit(ToolsProgress::simple(&spec.id, "done"));
            continue;
        }
        download_spec(&dir, &spec, &emit)
            .map_err(|e| format!("failed to set up {}: {e}", spec.id))?;
    }
    Ok(())
}

impl ToolsProgress {
    fn simple(tool: &str, stage: &str) -> Self {
        Self {
            tool: tool.to_string(),
            stage: stage.to_string(),
            downloaded_bytes: None,
            total_bytes: None,
            error: None,
        }
    }
}

fn download_spec(
    dir: &Path,
    spec: &ToolSpec,
    emit: &impl Fn(ToolsProgress),
) -> Result<(), String> {
    emit(ToolsProgress::simple(spec.id, "downloading"));

    let response = ureq::get(spec.url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let total_bytes = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let tmp = dir.join(format!(".download-{}", spec.id));
    let mut downloaded: u64 = 0;
    {
        let mut file =
            File::create(&tmp).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        let mut reader = response.into_body().into_reader();
        let mut buf = [0u8; 64 * 1024];
        let mut last_emit = Instant::now();
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("download interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("write failed: {e}"))?;
            downloaded += n as u64;
            if last_emit.elapsed().as_millis() >= 300 {
                emit(ToolsProgress {
                    downloaded_bytes: Some(downloaded),
                    total_bytes,
                    ..ToolsProgress::simple(spec.id, "downloading")
                });
                last_emit = Instant::now();
            }
        }
    }

    emit(ToolsProgress::simple(spec.id, "installing"));
    let result = if spec.zip {
        extract_zip(&tmp, dir, spec.targets)
    } else {
        install_raw(&tmp, dir, spec.targets[0])
    };
    if tmp.exists() {
        fs::remove_file(&tmp).ok();
    }
    result?;

    emit(ToolsProgress {
        downloaded_bytes: Some(downloaded),
        total_bytes,
        ..ToolsProgress::simple(spec.id, "done")
    });
    Ok(())
}

/// Move a downloaded raw binary into place and make it executable.
fn install_raw(tmp: &Path, dir: &Path, target: &str) -> Result<(), String> {
    let dest = dir.join(target);
    fs::rename(tmp, &dest).map_err(|e| format!("cannot install {target}: {e}"))?;
    make_executable(&dest).map_err(|e| format!("cannot chmod {target}: {e}"))
}

/// Extract the named binaries from a downloaded zip (matched by file name,
/// regardless of the archive's internal layout) and make them executable.
fn extract_zip(zip_path: &Path, dir: &Path, targets: &[&str]) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("cannot open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid zip: {e}"))?;

    let mut installed = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("cannot read zip entry: {e}"))?;
        let Some(file_name) = entry.name().rsplit('/').next().map(str::to_owned) else {
            continue;
        };
        if !targets.contains(&file_name.as_str()) {
            continue;
        }
        let dest = dir.join(&file_name);
        let mut out =
            File::create(&dest).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("extract failed: {e}"))?;
        drop(out);
        make_executable(&dest).map_err(|e| format!("cannot chmod {file_name}: {e}"))?;
        installed += 1;
    }

    if installed == targets.len() {
        Ok(())
    } else {
        Err(format!(
            "zip contained {installed} of {} expected tools",
            targets.len()
        ))
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_uses_home() {
        let dir = default_tools_dir(Some(PathBuf::from("/home/testuser")));
        let s = dir.to_string_lossy();
        assert!(s.contains("forvid"));
        assert!(s.contains("bin"));
    }

    #[test]
    fn spec_table_is_sane() {
        let specs = tool_specs();
        assert!(!specs.is_empty());
        for spec in &specs {
            assert!(spec.url.starts_with("https://"));
            assert!(!spec.targets.is_empty());
        }
    }

    #[test]
    fn status_reports_tools() {
        // On the dev machine, brew yt-dlp is System presence.
        let status = tools_status();
        assert!(!status.dir.is_empty());
    }

    /// Real download gate: installs all managed tools into a temp dir and
    /// verifies each one executes. Ignored by default; run with:
    /// FORVID_TOOLS_DIR=/tmp/forvid-tools cargo test -- --ignored
    #[test]
    #[ignore = "downloads ~100MB from the network; run explicitly"]
    fn downloads_and_installs_all_tools() {
        let dir = std::env::temp_dir().join("forvid-tools-test");
        // SAFETY: single ignored test; no other thread reads this var concurrently.
        unsafe { std::env::set_var("FORVID_TOOLS_DIR", &dir) };
        setup_tools(|_| {}).expect("setup must succeed");

        let spec_ids: Vec<&str> = tool_specs().iter().map(|s| s.id).collect();
        assert!(spec_ids.contains(&"yt-dlp"));

        for spec in tool_specs() {
            for target in spec.targets {
                let bin = dir.join(target);
                assert!(bin.is_file(), "missing installed tool: {target}");
                // ffmpeg/ffprobe use the documented single-dash -version;
                // passing --version makes ffmpeg print its version yet
                // still exit with code 8.
                let version_arg = if target.starts_with("ff") {
                    "-version"
                } else {
                    "--version"
                };
                // First execution of a freshly written binary can be
                // briefly blocked by macOS provenance validation — retry.
                let mut ok = false;
                for attempt in 0..3 {
                    if attempt > 0 {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    let output = std::process::Command::new(&bin)
                        .arg(version_arg)
                        .output()
                        .unwrap_or_else(|e| panic!("cannot run {target}: {e}"));
                    if output.status.success() {
                        ok = true;
                        break;
                    }
                }
                assert!(ok, "{target} {version_arg} failed after retries");
            }
        }
    }
}
