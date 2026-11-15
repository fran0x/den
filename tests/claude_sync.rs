// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Skills synced from claude.ai: Claude Code keeps them in
//! `~/.claude/skills/synced/<bucket>`, next to a `.bucket-<bucket>` marker, with a
//! `manifest.json`. They are listed as claude.ai sync skills, never as one broken skill
//! named `synced`, and checked against the manifest. Nothing is changed.
use den::areas::agents::{
    claude,
    skills::{Scope, Severity, Validity},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const BUCKET: &str = "afc2860e_d045bfb5";

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "den-claude-sync-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("project")).unwrap();
        Self(p)
    }
    fn skills(&self) -> PathBuf {
        self.0.join("home/.claude/skills")
    }
    fn skill(&self, dir: &Path, name: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(
            dir.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill {name}.\n---\nBody\n"),
        )
        .unwrap();
    }
    /// A sync folder with these skills and, when given, this manifest.
    fn synced(&self, skills: &[&str], manifest: Option<&str>, marker: bool) -> PathBuf {
        let synced = self.skills().join("synced");
        let bucket = synced.join(BUCKET);
        fs::create_dir_all(bucket.join(".staging")).unwrap();
        if marker {
            fs::write(synced.join(format!(".bucket-{BUCKET}")), "").unwrap();
        }
        fs::write(bucket.join(".last-complete-round"), "x").unwrap();
        for s in skills {
            self.skill(&bucket, s);
        }
        if let Some(m) = manifest {
            fs::write(bucket.join("manifest.json"), m).unwrap();
        }
        bucket
    }
    fn collect(&self) -> claude::Inventory {
        let context = claude::Context::fixture(&self.0).unwrap();
        claude::discover(&context, &AtomicBool::new(false)).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn manifest(names: &[&str]) -> String {
    let skills: Vec<String> = names
        .iter()
        .map(|n| {
            format!(
                r#"{{"skillId":"id-{n}","name":"{n}","description":"d","source":"anthropic","updatedAt":"2026-11-14T20:07:00Z"}}"#
            )
        })
        .collect();
    format!(r#"{{"lastUpdated":1,"skills":[{}]}}"#, skills.join(","))
}
fn names(i: &claude::Inventory, scope: Scope) -> Vec<String> {
    let mut n: Vec<_> = i
        .skills
        .skills
        .iter()
        .filter(|s| s.scope == scope)
        .map(|s| s.name.clone())
        .collect();
    n.sort();
    n
}

#[test]
fn synced_skills_are_listed_as_claude_ai_sync_not_as_a_broken_skill() {
    let t = Temp::new();
    t.skill(&t.skills(), "mine");
    t.synced(&["docx", "pdf"], Some(&manifest(&["pdf", "docx"])), true);
    let i = t.collect();
    assert_eq!(names(&i, Scope::User), ["mine"]);
    assert_eq!(names(&i, Scope::Synced), ["docx", "pdf"]);
    for s in &i.skills.skills {
        assert_eq!(s.metadata, Validity::Valid, "{}", s.name);
        assert!(s.findings.is_empty(), "{}: {:?}", s.name, s.findings);
    }
    assert!(
        i.coverage
            .iter()
            .any(|c| c == "claude.ai sync: its manifest lists 2 skills, all present."),
        "{:?}",
        i.coverage
    );
}

#[test]
fn the_manifest_is_compared_with_the_folders() {
    let t = Temp::new();
    t.synced(&["pdf", "stray"], Some(&manifest(&["pdf", "xlsx"])), true);
    let i = t.collect();
    let stray = i.skills.skills.iter().find(|s| s.name == "stray").unwrap();
    assert!(
        stray
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning
                && f.message.starts_with("Not in claude.ai sync's manifest"))
    );
    assert!(
        i.coverage
            .iter()
            .any(|c| c == "claude.ai sync: its manifest lists 2 skills; not here yet: xlsx."),
        "{:?}",
        i.coverage
    );
}

#[test]
fn a_missing_or_malformed_manifest_is_said_and_the_skills_still_listed() {
    for (manifest, why) in [
        (None, "manifest.json missing or unreadable"),
        (
            Some("{not json"),
            "manifest.json is not in the expected format",
        ),
        (
            Some(r#"{"skills":"x"}"#),
            "manifest.json is not in the expected format",
        ),
    ] {
        let t = Temp::new();
        let bucket = t.synced(&["pdf"], manifest, true);
        let i = t.collect();
        assert_eq!(names(&i, Scope::Synced), ["pdf"]);
        assert!(
            i.coverage
                .iter()
                .any(|c| c.starts_with(&format!("claude.ai sync: {why}"))),
            "{:?}",
            i.coverage
        );
        // Nothing is changed.
        assert_eq!(
            fs::read_to_string(bucket.join("manifest.json"))
                .ok()
                .as_deref(),
            manifest
        );
        assert!(bucket.join(".staging").is_dir());
    }
}

#[test]
fn without_the_marker_synced_is_an_ordinary_skill_folder() {
    let t = Temp::new();
    t.synced(&["pdf"], Some(&manifest(&["pdf"])), false);
    let i = t.collect();
    assert!(names(&i, Scope::Synced).is_empty());
    let synced = i.skills.skills.iter().find(|s| s.name == "synced").unwrap();
    assert_eq!(synced.scope, Scope::User);
    assert_eq!(synced.metadata, Validity::Invalid);
    assert!(!i.coverage.iter().any(|c| c.starts_with("claude.ai sync:")));
}

#[cfg(unix)]
#[test]
fn a_linked_sync_folder_is_not_followed() {
    let t = Temp::new();
    let elsewhere = t.0.join("elsewhere");
    t.skill(&elsewhere.join(BUCKET), "pdf");
    fs::create_dir_all(t.skills()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.skills().join("synced")).unwrap();
    fs::write(elsewhere.join(format!(".bucket-{BUCKET}")), "").unwrap();
    let i = t.collect();
    assert!(names(&i, Scope::Synced).is_empty());
}
