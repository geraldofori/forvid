//! Checking for newer Forvid releases on GitHub.
//!
//! Compares the latest published (non-draft, non-prerelease) GitHub release
//! against the running app version. Silently reports "no update" on any
//! failure — update checks must never break the app.

use crate::models::AppUpdate;

const RELEASES_URL: &str =
    "https://api.github.com/repos/geraldofori/forvid/releases/latest";
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
    Some(AppUpdate {
        version: tag.trim_start_matches('v').to_string(),
        url,
    })
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

    /// Live check against GitHub. Ignored by default; run with:
    /// cargo test -- --ignored
    #[test]
    #[ignore = "hits the network; run explicitly"]
    fn update_check_runs() {
        // Whatever the answer (update or not), the call must not error out.
        let _ = check_app_update();
    }
}
