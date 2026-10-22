//! The GitHub releases check: on launch (quietly) and from GlyphClub ▸
//! Check for Updates…

pub const RELEASES_URL: &str = "https://github.com/tonykastaneda/GlyphClub/releases/latest";
pub const REPO_URL: &str = "https://github.com/tonykastaneda/GlyphClub";

pub enum UpdateCheck {
    Available(String),
    UpToDate,
    Failed,
}

/// Blocking; run it off the main thread.
pub fn check() -> UpdateCheck {
    match fetch_latest_release_version() {
        Some(latest) if is_newer_version(&latest, env!("CARGO_PKG_VERSION")) => UpdateCheck::Available(latest),
        Some(_) => UpdateCheck::UpToDate,
        None => UpdateCheck::Failed,
    }
}

fn fetch_latest_release_version() -> Option<String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .build();
    let agent: ureq::Agent = config.into();
    let body: serde_json::Value = agent
        .get("https://api.github.com/repos/tonykastaneda/GlyphClub/releases/latest")
        // GitHub's API rejects requests with no User-Agent at all.
        .header("User-Agent", "GlyphClub-UpdateCheck")
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    let tag = body.get("tag_name")?.as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}
