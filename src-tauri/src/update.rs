//! Checking for newer Forvid releases on GitHub.
//!
//! Compares the latest published (non-draft, non-prerelease) GitHub release
//! against the running app version. Silently reports "no update" on any
//! failure — update checks must never break the app.

use crate::models::AppUpdate;

const RELEASES_URL: &str =
    "https://api.github.com/repos/geraldofori/forvid/releases/latest";

/// Progress for an app-update download (event name).
pub const UPDATE_EVENT: &str = "app-update-progress";
/// GitHub's API rejects requests without a User-Agent.
const USER_AGENT: &str = concat!("forvid/", env!("CARGO_PKG_VERSION"));

/// Returns `Some(AppUpdate)` when the latest release is newer than the
/// running binary, `None` when up to date or on any error.
pub fn check_app_update() -> Option<AppUpdate> {
    let current = parse_version(env!("CARGO_PKG_VERSION"))?;

    let mut response = ureq::get(RELEASES_URL)
        .header("User-Agent", USER_AGENT)
        .call()
        .ok()?;
    let text = response.body_mut().read_to_string().ok()?;
    let body: serde_json::Value = serde_json::from_str(&text).ok()?;

    let tag = body.get("tag_name")?.as_str()?;
    let latest = parse_version(tag)?;
    if latest <= current {
        return None;
    }

    let url = body
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("https://github.com/geraldofori/forvid/releases")
        .to_string();
    let (asset_name, asset_url) = body
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| pick_asset(assets))
        .map(|(n, u)| (Some(n), Some(u)))
        .unwrap_or((None, None));
    Some(AppUpdate {
        version: tag.trim_start_matches('v').to_string(),
        url,
        asset_url,
        asset_name,
    })
}

/// Choose the release asset matching this device's OS/architecture,
/// compiled per platform. Returns (file name, direct download URL).
#[cfg(target_os = "macos")]
fn pick_asset(assets: &[serde_json::Value]) -> Option<(String, String)> {
    // Universal dmg first (works on every Mac), then an arch-specific dmg.
    find_asset(assets, &|n| n.contains("universal") && n.ends_with(".dmg"))
        .or_else(|| find_asset(assets, &|n| n.ends_with(".dmg") && n.contains(std::env::consts::ARCH)))
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn pick_asset(assets: &[serde_json::Value]) -> Option<(String, String)> {
    find_asset(assets, &|n| n.contains("x64") && n.ends_with("-setup.exe"))
        .or_else(|| find_asset(assets, &|n| n.contains("x64") && n.ends_with(".msi")))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn pick_asset(assets: &[serde_json::Value]) -> Option<(String, String)> {
    find_asset(assets, &|n| n.ends_with(".AppImage"))
        .or_else(|| find_asset(assets, &|n| n.ends_with(".deb")))
}

#[cfg(not(any(
    target_os = "macos",
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
fn pick_asset(_assets: &[serde_json::Value]) -> Option<(String, String)> {
    None
}

#[cfg(any(
    target_os = "macos",
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn find_asset(
    assets: &[serde_json::Value],
    matches: &dyn Fn(&str) -> bool,
) -> Option<(String, String)> {
    for asset in assets {
        let name = asset.get("name")?.as_str()?;
        if matches(name) {
            let url = asset.get("browser_download_url")?.as_str()?.to_string();
            return Some((name.to_string(), url));
        }
    }
    None
}

/// Download an update asset into ~/Downloads, reporting progress via `emit`.
/// Returns the destination path on success.
pub fn download_app_update(
    asset_url: &str,
    asset_name: &str,
    emit: impl Fn(crate::models::AppUpdateProgress) + Send + Sync + 'static,
) -> Result<String, String> {
    use std::io::{Read, Write};

    // Sanitize: keep only the file name component.
    let file_name = asset_name
        .rsplit(['/', '\\'])
        .next()
        .filter(|n| !n.is_empty())
        .ok_or("invalid asset name")?;
    let dest = std::env::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Downloads")
        .join(file_name);

    let response = ureq::get(asset_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let total_bytes = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let mut downloaded: u64 = 0;
    {
        let mut file = std::fs::File::create(&dest)
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        let mut reader = response.into_body().into_reader();
        let mut buf = [0u8; 64 * 1024];
        let mut last_emit = std::time::Instant::now();
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
            if last_emit.elapsed().as_millis() >= 250 {
                emit(crate::models::AppUpdateProgress {
                    downloaded_bytes: Some(downloaded),
                    total_bytes,
                });
                last_emit = std::time::Instant::now();
            }
        }
    }
    emit(crate::models::AppUpdateProgress {
        downloaded_bytes: Some(downloaded),
        total_bytes,
    });
    Ok(dest.to_string_lossy().into_owned())
}

/// Parse "v1.2.3" / "1.2.3" into a comparable tuple; prerelease suffixes
/// ("1.2.3-beta") are ignored. Missing components count as 0.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("v2"), Some((2, 0, 0)));
        assert_eq!(parse_version("not-a-version"), None);
    }

    #[test]
    fn orders_versions() {
        assert!(parse_version("0.2.0").unwrap() > parse_version("0.1.0").unwrap());
        assert!(parse_version("0.1.1").unwrap() > parse_version("0.1.0").unwrap());
        assert!(parse_version("0.1.0").unwrap() == parse_version("v0.1.0").unwrap());
    }

    #[cfg(any(
        target_os = "macos",
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn picks_the_right_asset_for_this_platform() {
        let assets = serde_json::json!([
            {"name": "Forvid_0.3.0_amd64.AppImage", "browser_download_url": "https://example/appimage"},
            {"name": "Forvid_0.3.0_universal.dmg", "browser_download_url": "https://example/dmg"},
            {"name": "Forvid_0.3.0_x64-setup.exe", "browser_download_url": "https://example/exe"},
            {"name": "Forvid_0.3.0_amd64.deb", "browser_download_url": "https://example/deb"}
        ]);
        let assets = assets.as_array().unwrap();
        let (name, _url) = pick_asset(assets).expect("an asset must match");
        #[cfg(target_os = "macos")]
        assert_eq!(name, "Forvid_0.3.0_universal.dmg");
        #[cfg(target_os = "windows")]
        assert_eq!(name, "Forvid_0.3.0_x64-setup.exe");
        #[cfg(target_os = "linux")]
        assert_eq!(name, "Forvid_0.3.0_amd64.AppImage");
    }

    /// Live check against GitHub. Ignored by default; run with:
    /// cargo test -- --ignored
    #[test]
    #[ignore = "hits the network; run explicitly"]
    fn update_check_runs() {
        // Whatever the answer (update or not), the call must not error out.
        let _ = check_app_update();
    }
}
