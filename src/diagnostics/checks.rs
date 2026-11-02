//! Conservative interpretation of bounded Homebrew reports; original text is retained.
use crate::graph::Graph;
use crate::{
    homebrew::{CollectionError, CommandOutput},
    inventory::{HomebrewMetadata, Inventory, ResourceKind, safe_text},
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Missing,
    Leaves,
    Cleanup,
}
impl Check {
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Missing => &["missing"],
            Self::Leaves => &["leaves"],
            Self::Cleanup => &["autoremove", "--dry-run"],
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Missing => 0,
            Self::Leaves => 1,
            Self::Cleanup => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Informational,
    Confirmed,
    Review,
    Failed,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Informational => "Informational",
            Self::Confirmed => "Confirmed finding",
            Self::Review => "Review required",
            Self::Failed => "Check failed",
        }
    }
}
#[derive(Debug, Clone)]
pub struct ReportEntry {
    pub label: String,
    /// Exact installed identity only. Never guess at aliases or formula/cask collisions.
    pub package_id: Option<String>,
    pub edge: Option<usize>,
}
#[derive(Debug, Clone)]
pub struct Report {
    pub status: Status,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub entries: Vec<ReportEntry>,
    pub count: Option<usize>,
    pub epoch: u64,
    pub checked_at: u64,
}
fn sanitize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(safe_text)
        .collect::<Vec<_>>()
        .join("\n")
}
fn package_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 8192
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@+._/-".contains(&b))
}
fn error_heading(s: &str) -> bool {
    s.split_once(':').is_some_and(|(heading, _)| {
        ["error", "warning", "fatal"]
            .iter()
            .any(|value| heading.eq_ignore_ascii_case(value))
    })
}
fn exact(inventory: &Inventory, name: &str, formula_only: bool) -> Option<usize> {
    let mut found = inventory.resources.iter().enumerate().filter(|(_, r)| {
        if formula_only && r.kind != ResourceKind::Formula {
            return false;
        }
        let full = match &r.homebrew {
            Some(HomebrewMetadata::Formula { full_name, .. }) => full_name.as_deref(),
            Some(HomebrewMetadata::Cask { full_token, .. }) => full_token.as_deref(),
            _ => return false,
        };
        // Use recorded full names, not a short-name fallback for third-party taps.
        full == Some(name)
            || r.id.strip_prefix("brew:formula:") == Some(name)
            || r.id.strip_prefix("brew:cask:") == Some(name)
    });
    let first = found.next()?.0;
    found.next().is_none().then_some(first)
}

impl Report {
    pub fn interpret(
        check: Check,
        result: Result<CommandOutput, CollectionError>,
        inventory: &Inventory,
        graph: &Graph,
        epoch: u64,
    ) -> Self {
        let mut report = Self {
            status: Status::Failed,
            message: "Preview unavailable".into(),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
            entries: vec![],
            count: None,
            epoch,
            checked_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let output = match result {
            Ok(output) if output.stdout.len() <= 256 * 1024 && output.stderr.len() <= 64 * 1024 => {
                output
            }
            Ok(_) => {
                report.message = "Check failed: output limit exceeded; preview unavailable".into();
                return report;
            }
            Err(error) => {
                report.message = format!("Check failed: {error}; preview unavailable");
                return report;
            }
        };
        report.code = output.code;
        report.stdout = sanitize_output(&output.stdout);
        report.stderr = sanitize_output(&output.stderr);
        let valid_utf8 = std::str::from_utf8(&output.stdout).is_ok();
        let lines: Vec<_> = report
            .stdout
            .lines()
            .filter(|s| !s.trim().is_empty())
            .collect();
        if output.code != Some(0) && !(check == Check::Missing && output.code == Some(1)) {
            report.message = format!(
                "Check failed (exit {:?}); preview unavailable. Original output retained.",
                output.code
            );
            return report;
        }
        // Unexpected diagnostics cannot establish an empty successful result.
        let clean = valid_utf8 && report.stderr.trim().is_empty();
        match check {
            Check::Missing => {
                if output.code == Some(0) && lines.is_empty() && clean {
                    report.status = Status::Informational;
                    report.count = Some(0);
                    report.message = "Homebrew reported no missing dependencies.".into();
                } else if output.code == Some(1) && !lines.is_empty() && clean {
                    let mut entries = vec![];
                    let mut understood = true;
                    for line in lines {
                        if error_heading(line) {
                            understood = false;
                            break;
                        }
                        let (source, dependencies) = match line.split_once(": ") {
                            Some((source, deps)) if package_name(source) => {
                                (exact(inventory, source, false), deps)
                            }
                            None if inventory.resources.len() == 1 => (Some(0), line),
                            _ => {
                                understood = false;
                                break;
                            }
                        };
                        if dependencies.split_whitespace().next().is_none() {
                            understood = false;
                            break;
                        }
                        for dependency in dependencies.split_whitespace() {
                            if !package_name(dependency) {
                                understood = false;
                                break;
                            }
                            let edges: Vec<_> = source
                                .into_iter()
                                .flat_map(|n| graph.forward[n].iter().copied())
                                .filter(|&i| graph.edges[i].reference == dependency)
                                .collect();
                            entries.push(ReportEntry {
                                label: format!(
                                    "{}: {dependency}",
                                    line.split_once(": ")
                                        .map_or("Installed package", |(n, _)| n)
                                ),
                                package_id: source.map(|n| inventory.resources[n].id.clone()),
                                edge: (edges.len() == 1).then(|| edges[0]),
                            });
                        }
                    }
                    if understood {
                        report.status = Status::Confirmed;
                        report.count = Some(entries.len());
                        report.message = format!(
                            "Homebrew reports {} missing dependency references (exit 1 is expected for findings).",
                            entries.len()
                        );
                        report.entries = entries;
                    } else {
                        report.message = "Check failed: nonzero missing output is not recognized; inspect original report. No missing count established.".into();
                    }
                } else {
                    report.status = if output.code == Some(0) {
                        Status::Review
                    } else {
                        Status::Failed
                    };
                    report.message = "Missing check output is unexpected; inspect original report. No missing count established.".into();
                }
            }
            Check::Leaves | Check::Cleanup => {
                let names = if check == Check::Leaves {
                    lines
                        .iter()
                        .all(|name| package_name(name))
                        .then_some(lines.clone())
                } else if lines.is_empty() {
                    Some(vec![])
                } else {
                    // Recognize only the exact known dry-run grammar with a matching count.
                    // Future formats remain authoritative text, not invented empty results.
                    let header = lines[0].strip_prefix("==> ").unwrap_or(lines[0]);
                    let count = header
                        .strip_prefix("Would autoremove ")
                        .and_then(|s| s.split_once(' '))
                        .and_then(|(n, tail)| {
                            matches!(tail, "unneeded formula:" | "unneeded formulae:")
                                .then(|| n.parse::<usize>().ok())
                                .flatten()
                        });
                    count
                        .filter(|&n| {
                            n > 0
                                && n == lines.len() - 1
                                && lines[1..].iter().all(|s| package_name(s))
                        })
                        .map(|_| lines[1..].to_vec())
                };
                if let Some(names) = names.filter(|_| clean) {
                    let unique: std::collections::HashSet<_> = names.iter().copied().collect();
                    if unique.len() == names.len() {
                        report.count = Some(names.len());
                        report.status = if check == Check::Cleanup && !names.is_empty() {
                            Status::Confirmed
                        } else {
                            Status::Informational
                        };
                        report.message = if check == Check::Cleanup && names.is_empty() {
                            "Homebrew found no formulae eligible for automatic removal.".into()
                        } else {
                            format!(
                                "Homebrew reports {} {}. This is not a safety assessment.",
                                names.len(),
                                if check == Check::Leaves {
                                    "leaves"
                                } else {
                                    "autoremove-eligible formulae"
                                }
                            )
                        };
                        report.entries = names
                            .into_iter()
                            .map(|name| ReportEntry {
                                label: name.into(),
                                package_id: exact(inventory, name, true)
                                    .map(|n| inventory.resources[n].id.clone()),
                                edge: None,
                            })
                            .collect();
                    }
                }
                if report.count.is_none() {
                    report.status = Status::Review;
                    report.message = "Command succeeded; output format is unexpected. Review original output; candidate count is unknown.".into();
                }
            }
        }
        report
    }
}

#[derive(Debug, Default)]
pub struct CheckState {
    pub running: bool,
    pub report: Option<Report>,
}
