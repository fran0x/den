//! Newer versions Den can name without contacting anything: Homebrew's own data (as
//! current as Homebrew's last metadata refresh) and a tool's own update check, such as
//! Codex's `version.json`. Nothing is fetched and no tool is run.
use crate::inventory::{Inventory, Resource, ResourceKind};
use std::time::{Duration, SystemTime};

/// A version string worth comparing or showing: starts with a digit, short, and only
/// letters, digits and `.` `-` `+` `_`.
pub fn plausible(version: &str) -> bool {
    version.len() <= 40
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// Numeric parts of a plain version: `26.13.0_2` → `[26, 13, 0, 2]`. Pre-releases and
/// other text give `None`, so they are never claimed as newer.
fn parts(version: &str) -> Option<Vec<u64>> {
    version.split(['.', '_']).map(|p| p.parse().ok()).collect()
}

/// True only when both are plain versions and `candidate` is newer than `installed`.
pub fn is_newer(installed: &str, candidate: &str) -> bool {
    match (parts(installed), parts(candidate)) {
        (Some(installed), Some(candidate)) => candidate > installed,
        _ => false,
    }
}

/// The installed Homebrew formula or cask with this name.
pub fn homebrew<'a>(inventory: &'a Inventory, name: &str, cask: bool) -> Option<&'a Resource> {
    let kind = if cask {
        ResourceKind::Cask
    } else {
        ResourceKind::Formula
    };
    inventory
        .resources
        .iter()
        .find(|r| r.kind == kind && r.name == name)
}

/// The newer version Homebrew reports for an installed package, if any.
pub fn from_homebrew(resource: &Resource) -> Option<String> {
    resource
        .homebrew
        .as_ref()
        .and_then(|m| m.update())
        .map(str::to_owned)
}

/// The newer version of a tool Homebrew installed, from Homebrew's data or the tool's own
/// update check (`own`), whichever names the newer one. `None` when Homebrew did not
/// install it: without the installed version, nothing can be compared.
pub fn tool(inventory: &Inventory, names: &[&str], own: Option<&str>) -> Option<String> {
    let resource = names.iter().find_map(|name| {
        homebrew(inventory, name, true).or_else(|| homebrew(inventory, name, false))
    })?;
    let installed = resource.version.as_deref()?;
    let own = own.filter(|v| plausible(v) && is_newer(installed, v));
    match (from_homebrew(resource), own) {
        (Some(brew), Some(own)) if is_newer(&brew, own) => Some(own.to_owned()),
        (Some(brew), _) => Some(brew),
        (None, own) => own.map(str::to_owned),
    }
}

/// "just now", "5 min ago", "3 h ago" or "2 days ago".
pub fn age(when: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(when).unwrap_or(Duration::ZERO).as_secs();
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::HomebrewMetadata;

    #[test]
    fn only_plain_newer_versions_count() {
        assert!(is_newer("0.167.0", "0.167.2"));
        assert!(is_newer("26.13.0_2", "26.14.0"));
        assert!(is_newer("1.9", "1.10"));
        assert!(!is_newer("26.13.0_2", "26.13.0"), "a revision is not older");
        assert!(!is_newer("0.167.2", "0.167.2"));
        assert!(!is_newer("2.0.0", "1.9.9"));
        assert!(
            !is_newer("1.0.0", "2.0.0-beta"),
            "pre-releases are never claimed"
        );
        assert!(!is_newer("latest", "2.0"));
        assert!(plausible("2.1.309") && plausible("26.13.0_2"));
        assert!(!plausible("v1") && !plausible("1.0 \u{1b}[31m") && !plausible(""));
    }

    fn cask(name: &str, version: &str, latest: &str, outdated: bool) -> Resource {
        let json = serde_json::json!({
            "id": format!("cask:{name}"), "name": name, "kind": "cask",
            "version": version, "path": null, "description": "", "origin": "", "reason": "",
            "homebrew": {"type": "cask", "full_token": name, "display_names": [],
                "tap": null, "homepage": null, "recorded_version": version,
                "installed_on_request": true, "latest": latest, "outdated": outdated}
        });
        serde_json::from_value(json).unwrap()
    }
    fn inventory(resources: Vec<Resource>) -> Inventory {
        Inventory {
            schema_version: 1,
            resources,
        }
    }

    #[test]
    fn a_tool_takes_the_newest_of_homebrew_and_its_own_check() {
        // Homebrew does not flag it, but the tool's own check knows a newer version.
        let i = inventory(vec![cask("codex", "0.167.0", "0.167.0", false)]);
        assert_eq!(
            tool(&i, &["codex"], Some("0.167.2")).as_deref(),
            Some("0.167.2")
        );
        assert_eq!(tool(&i, &["codex"], Some("0.167.0")), None);
        assert_eq!(tool(&i, &["codex"], None), None);
        // Homebrew flags it: the newer of the two wins.
        let i = inventory(vec![cask("codex", "0.167.0", "0.168.0", true)]);
        assert_eq!(
            tool(&i, &["codex"], Some("0.167.2")).as_deref(),
            Some("0.168.0")
        );
        assert_eq!(
            tool(&i, &["codex"], Some("0.169.0")).as_deref(),
            Some("0.169.0")
        );
        // Not installed with Homebrew: the installed version is unknown, so nothing.
        assert_eq!(tool(&inventory(vec![]), &["codex"], Some("9.0.0")), None);
        assert!(matches!(
            i.resources[0].homebrew,
            Some(HomebrewMetadata::Cask { .. })
        ));
    }

    #[test]
    fn ages_read_naturally() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let ago = |s| age(now - Duration::from_secs(s), now);
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(300), "5 min ago");
        assert_eq!(ago(3 * 3600 + 5), "3 h ago");
        assert_eq!(ago(2 * 86_400), "2 days ago");
        assert_eq!(age(now + Duration::from_secs(60), now), "just now");
    }
}
