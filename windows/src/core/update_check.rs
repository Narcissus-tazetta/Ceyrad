//! Deciding whether a newer release exists — everything but the network call.
//!
//! macOS gets this from Sparkle: a signed appcast, an update dialog, and an
//! in-place replace-and-relaunch. Windows has no equivalent in this stack, and
//! more to the point there is no code signing certificate — silently swapping a
//! running unsigned binary for a freshly downloaded unsigned binary gives a user
//! nothing to verify and an antivirus heuristic every reason to object. So this
//! only ever *finds out* that a release exists; fetching it stays a deliberate
//! act, on a page the user can look at first.

use std::cmp::Ordering;

use serde::Deserialize;

/// A published release worth telling the user about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The tag as published, `v` prefix and all — this is display text.
    pub tag: String,
    /// The version with any `v` stripped, for comparing.
    pub version: String,
    /// The human page, not the asset: the user decides what to download.
    pub page_url: String,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// The API endpoint for a repository's newest release.
///
/// Derived from the `repository` field in `Cargo.toml` rather than written out
/// again, so a fork or a rename does not leave this pointing at the original.
pub fn latest_release_url(repository: &str) -> Option<String> {
    let trimmed = repository.trim().trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);

    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(format!(
        "https://api.github.com/repos/{owner}/{repo}/releases/latest"
    ))
}

/// The release in a `releases/latest` response, or `None` when there isn't one
/// worth offering.
///
/// GitHub already excludes drafts and pre-releases from this endpoint, but the
/// flags are checked anyway: the cost is two lines, and the failure mode without
/// them is pushing users at a half-finished build.
pub fn parse_latest(body: &str) -> Option<Release> {
    let latest: LatestRelease = serde_json::from_str(body).ok()?;
    if latest.draft || latest.prerelease {
        return None;
    }
    let tag = latest.tag_name?;
    // The only thing this app ever does with the answer is hand `page_url` to
    // the shell, so it is pinned to GitHub here rather than trusted because the
    // transport was. A release without a GitHub page is not one we can offer.
    let page_url = latest.html_url.filter(|url| is_github_url(url))?;
    let version = strip_v(&tag).to_string();
    if version.is_empty() {
        return None;
    }
    Some(Release {
        tag,
        version,
        page_url,
    })
}

fn is_github_url(url: &str) -> bool {
    url.starts_with("https://github.com/")
}

/// Whether `candidate` is strictly newer than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == Ordering::Greater
}

/// Orders two `MAJOR.MINOR.PATCH` versions.
///
/// Hand-rolled rather than pulling in `semver`: the only versions this ever sees
/// are this project's own tags, and a comparison this narrow does not earn a
/// dependency. Missing components count as zero, so `1.2` and `1.2.0` are equal,
/// and a pre-release suffix sorts *below* the release it leads to, as semver
/// says it should — `1.0.0-rc1` must not look like an update over `1.0.0`.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre) = split_pre_release(strip_v(a));
    let (b_core, b_pre) = split_pre_release(strip_v(b));

    let mut a_parts = a_core.split('.');
    let mut b_parts = b_core.split('.');
    for _ in 0..3 {
        let a_part = numeric(a_parts.next());
        let b_part = numeric(b_parts.next());
        match a_part.cmp(&b_part) {
            Ordering::Equal => {}
            other => return other,
        }
    }

    // Equal cores: whichever is not a pre-release wins.
    match (a_pre, b_pre) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        _ => Ordering::Equal,
    }
}

fn strip_v(version: &str) -> &str {
    let trimmed = version.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
}

/// Splits `1.2.3-rc1` into `("1.2.3", true)`. Build metadata (`+…`) is not a
/// precedence signal, so it is dropped without setting the flag.
fn split_pre_release(version: &str) -> (&str, bool) {
    let core = version.split('+').next().unwrap_or(version);
    match core.split_once('-') {
        Some((core, _)) => (core, true),
        None => (core, false),
    }
}

/// A component that is not a plain number counts as zero rather than poisoning
/// the whole comparison — an unparseable tag should read as "not newer", never
/// as "newer".
fn numeric(part: Option<&str>) -> u64 {
    part.unwrap_or("0").trim().parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "https://github.com/Narcissus-tazetta/Ceyrad";

    #[test]
    fn the_api_url_is_derived_from_the_repository_field() {
        assert_eq!(
            latest_release_url(REPO).as_deref(),
            Some("https://api.github.com/repos/Narcissus-tazetta/Ceyrad/releases/latest")
        );
    }

    #[test]
    fn the_api_url_tolerates_a_trailing_slash_or_git_suffix() {
        let expected =
            Some("https://api.github.com/repos/Narcissus-tazetta/Ceyrad/releases/latest");
        assert_eq!(
            latest_release_url("https://github.com/Narcissus-tazetta/Ceyrad/").as_deref(),
            expected
        );
        assert_eq!(
            latest_release_url("https://github.com/Narcissus-tazetta/Ceyrad.git").as_deref(),
            expected
        );
    }

    #[test]
    fn a_non_github_repository_yields_no_url() {
        for repository in [
            "",
            "https://example.com/foo/bar",
            "https://github.com/only-an-owner",
            "https://github.com/a/b/c",
        ] {
            assert_eq!(latest_release_url(repository), None, "{repository:?}");
        }
    }

    #[test]
    fn a_release_is_read_from_the_response() {
        let body = r#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/o/r/releases/tag/v0.2.0",
            "draft": false,
            "prerelease": false
        }"#;
        let release = parse_latest(body).expect("a release");
        assert_eq!(release.tag, "v0.2.0");
        assert_eq!(release.version, "0.2.0");
        assert_eq!(
            release.page_url,
            "https://github.com/o/r/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn a_draft_or_prerelease_is_not_offered() {
        let draft = r#"{"tag_name":"v9.0.0","html_url":"https://x","draft":true}"#;
        let pre = r#"{"tag_name":"v9.0.0","html_url":"https://x","prerelease":true}"#;
        assert_eq!(parse_latest(draft), None);
        assert_eq!(parse_latest(pre), None);
    }

    #[test]
    fn an_unusable_response_yields_nothing() {
        for body in [
            "",
            "not json",
            "{}",
            r#"{"tag_name":"v1.0.0"}"#, // no page to send anyone to
            r#"{"html_url":"https://github.com/o/r"}"#, // no version to compare
            r#"{"tag_name":"v","html_url":"https://github.com/o/r"}"#, // nothing after the v
        ] {
            assert_eq!(parse_latest(body), None, "{body:?}");
        }
    }

    #[test]
    fn a_page_url_that_is_not_github_is_refused() {
        // The page url is handed to the shell, so a response that redirects it
        // somewhere else must not produce a release at all.
        for url in [
            "http://github.com/o/r",             // not https
            "https://github.evil.example/o/r",   // not github.com
            "file:///C:/Windows/System32/x.exe", // not even http
            "ms-settings:",                      // a protocol handler
            "\\\\attacker\\share\\payload.exe",  // a UNC path
        ] {
            let body = format!(r#"{{"tag_name":"v9.0.0","html_url":"{url}"}}"#);
            assert_eq!(parse_latest(&body), None, "{url:?}");
        }
    }

    #[test]
    fn a_higher_component_is_newer() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.1.2", "0.1.1"));
    }

    #[test]
    fn the_same_version_is_not_an_update() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(
            !is_newer("v0.1.0", "0.1.0"),
            "the v prefix is not a version"
        );
        assert!(!is_newer("0.1", "0.1.0"), "a missing component is zero");
    }

    #[test]
    fn an_older_version_is_not_an_update() {
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn components_compare_numerically_not_as_text() {
        // "10" sorts before "9" as a string, which would hide every release
        // after the ninth.
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
    }

    #[test]
    fn a_prerelease_sorts_below_the_release_it_leads_to() {
        assert!(is_newer("1.0.0", "1.0.0-rc1"));
        assert!(!is_newer("1.0.0-rc1", "1.0.0"));
        // Still newer than the previous release, though.
        assert!(is_newer("1.0.0-rc1", "0.9.0"));
    }

    #[test]
    fn build_metadata_does_not_affect_precedence() {
        assert_eq!(compare_versions("1.0.0+abc", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn an_unparseable_tag_never_reads_as_newer() {
        // The safe direction: a tag we cannot understand must not nag the user.
        assert!(!is_newer("nightly", env!("CARGO_PKG_VERSION")));
        assert!(!is_newer("", "0.1.0"));
    }
}
