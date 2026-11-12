//! Test-only reconciliation against raw Homebrew records, independent of the DTOs
//! and production intent classifier. Diagnostics contain counts, never package data.
use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::inventory::{HomebrewMetadata, InstallationIntent, Inventory, ResourceKind};

#[derive(Default)]
pub(super) struct Report {
    package_counts: [usize; 2],
    version_records: [usize; 2],
    intents: [[usize; 3]; 2],
    unknown_reasons: BTreeMap<&'static str, [usize; 2]>,
}

impl Report {
    pub(super) fn print(&self) {
        println!("Reconciliation: Homebrew JSON / Den (same command response)");
        for (index, label) in ["Formulae", "Casks"].iter().enumerate() {
            let count = self.package_counts[index];
            println!(
                "{label}: {count} / {count}; {} installed version records matched",
                self.version_records[index]
            );
            for (intent, label) in ["Requested", "Dependency", "Unknown"].iter().enumerate() {
                let count = self.intents[index][intent];
                println!("  {label}: {count} / {count}");
            }
        }
        for (reason, [formulae, casks]) in &self.unknown_reasons {
            println!("Unknown explanation: {reason}: {formulae} formulae, {casks} casks");
        }
        println!(
            "PASS: every package and version matched; all intent classifications reconciled; every Unknown explained. No raw metadata saved."
        );
    }
}

// Each Unknown gets exactly one reason, with conflicting flags taking precedence.
// Evaluate raw booleans directly rather than reusing production classification.
fn expected_intent(records: &[&Value]) -> Result<usize, &'static str> {
    assert!(!records.is_empty(), "installation records must be present");
    let flags: Vec<_> = records
        .iter()
        .map(|record| {
            (
                record["installed_on_request"].as_bool(),
                record["installed_as_dependency"].as_bool(),
            )
        })
        .collect();
    if flags.contains(&(Some(true), Some(true))) {
        return Err("conflicting request/dependency flags");
    }
    if flags.iter().all(|(request, _)| *request == Some(true)) {
        return Ok(0);
    }
    if flags
        .iter()
        .all(|(_, dependency)| *dependency == Some(true))
    {
        return Ok(1);
    }
    if flags.iter().all(|flags| *flags == (None, None)) {
        return Err("request and dependency flags absent");
    }
    if flags.iter().all(|flags| *flags == (Some(false), None)) {
        return Err("not requested, but no explicit dependency flag");
    }
    if flags
        .iter()
        .all(|(request, dependency)| *request == Some(true) || *dependency == Some(true))
    {
        return Err("installed kegs disagree on intent");
    }
    Err("insufficient installation evidence in one or more records")
}

pub(super) fn verify(raw: &Value, inventory: &Inventory) -> Report {
    let mut report = Report::default();
    let mut matched = HashSet::new();
    for (kind_index, (array, kind, name_key, full_key)) in [
        ("formulae", ResourceKind::Formula, "name", "full_name"),
        ("casks", ResourceKind::Cask, "token", "full_token"),
    ]
    .into_iter()
    .enumerate()
    {
        let records = raw[array]
            .as_array()
            .expect("Homebrew inventory array required");
        let actual_count = inventory
            .resources
            .iter()
            .filter(|r| r.kind == kind)
            .count();
        assert_eq!(actual_count, records.len(), "package type count mismatch");
        report.package_counts[kind_index] = actual_count;
        let mut expected_counts = [0; 3];
        let mut actual_counts = [0; 3];
        for record in records {
            // Match independently of array ordering; type, name and tap distinguish
            // same-named formulae/casks and third-party taps.
            let candidates: Vec<_> = inventory
                .resources
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    let tap = match &r.homebrew {
                        Some(
                            HomebrewMetadata::Formula { tap, .. }
                            | HomebrewMetadata::Cask { tap, .. },
                        ) => tap.as_deref(),
                        None => return false,
                    };
                    r.kind == kind
                        && Some(r.name.as_str()) == record[name_key].as_str()
                        && tap == record["tap"].as_str()
                })
                .collect();
            assert_eq!(
                candidates.len(),
                1,
                "each raw package must match exactly one Den resource"
            );
            let (index, resource) = candidates[0];
            assert!(
                matched.insert(index),
                "a resource must not match multiple raw packages"
            );
            let metadata = resource
                .homebrew
                .as_ref()
                .expect("source metadata required");
            let installed: Vec<&Value> = if kind == ResourceKind::Formula {
                record["installed"]
                    .as_array()
                    .expect("installed kegs required")
                    .iter()
                    .collect()
            } else {
                vec![record]
            };
            let expected_versions = if kind == ResourceKind::Formula {
                installed
                    .iter()
                    .map(|k| k["version"].as_str().expect("installed version required"))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                record["installed"]
                    .as_str()
                    .expect("recorded cask version required")
                    .to_owned()
            };
            assert!(
                resource.version.as_deref() == Some(expected_versions.as_str()),
                "displayed installed versions mismatch"
            );
            report.version_records[kind_index] += installed.len();
            match metadata {
                HomebrewMetadata::Formula {
                    full_name,
                    installed: kegs,
                    ..
                } => {
                    assert!(
                        kind == ResourceKind::Formula
                            && full_name.as_deref() == record[full_key].as_str(),
                        "formula identity mismatch"
                    );
                    assert_eq!(kegs.len(), installed.len(), "installed keg count mismatch");
                    for (keg, raw_keg) in kegs.iter().zip(&installed) {
                        assert!(
                            Some(keg.version.as_str()) == raw_keg["version"].as_str(),
                            "retained keg version mismatch"
                        );
                        assert!(
                            keg.installed_on_request == raw_keg["installed_on_request"].as_bool()
                                && keg.installed_as_dependency
                                    == raw_keg["installed_as_dependency"].as_bool(),
                            "retained keg flags mismatch"
                        );
                    }
                }
                HomebrewMetadata::Cask {
                    full_token,
                    recorded_version,
                    installed_on_request,
                    installed_as_dependency,
                    ..
                } => {
                    assert!(
                        kind == ResourceKind::Cask
                            && full_token.as_deref() == record[full_key].as_str(),
                        "cask identity mismatch"
                    );
                    assert!(
                        Some(recorded_version.as_str()) == record["installed"].as_str(),
                        "retained cask version mismatch"
                    );
                    assert!(
                        *installed_on_request == record["installed_on_request"].as_bool()
                            && *installed_as_dependency
                                == record["installed_as_dependency"].as_bool(),
                        "retained cask flags mismatch"
                    );
                }
            }
            let expected = match expected_intent(&installed) {
                Ok(intent) => intent,
                Err(reason) => {
                    report.unknown_reasons.entry(reason).or_default()[kind_index] += 1;
                    2
                }
            };
            let actual = match metadata.intent() {
                InstallationIntent::Requested => 0,
                InstallationIntent::Dependency => 1,
                InstallationIntent::Unknown => 2,
            };
            assert_eq!(actual, expected, "per-package installation intent mismatch");
            expected_counts[expected] += 1;
            actual_counts[actual] += 1;
        }
        assert_eq!(
            actual_counts, expected_counts,
            "aggregate intent counts mismatch"
        );
        assert_eq!(
            report
                .unknown_reasons
                .values()
                .map(|counts| counts[kind_index])
                .sum::<usize>(),
            actual_counts[2],
            "every Unknown must have one explanation"
        );
        report.intents[kind_index] = actual_counts;
    }
    assert_eq!(
        matched.len(),
        inventory.resources.len(),
        "unaccounted Den resources"
    );
    report
}

#[test]
fn fixture_reconciliation_is_order_independent_and_accounts_for_unknowns() {
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/brew.json"));
    let raw = serde_json::from_slice(bytes).unwrap();
    let mut inventory = super::parse(bytes).unwrap();
    inventory.resources.reverse();
    let report = verify(&raw, &inventory);
    assert_eq!(report.package_counts, [5, 2]);
    assert_eq!(report.version_records, [6, 2]);
    assert_eq!(report.intents, [[1, 1, 3], [1, 0, 1]]);
    assert_eq!(
        report.unknown_reasons["request and dependency flags absent"],
        [1, 1]
    );
    assert_eq!(
        report.unknown_reasons["not requested, but no explicit dependency flag"],
        [1, 0]
    );
    assert_eq!(
        report.unknown_reasons["installed kegs disagree on intent"],
        [1, 0]
    );
}

#[test]
fn reconciliation_detects_count_version_flag_and_package_mismatches() {
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/brew.json"));
    let raw = serde_json::from_slice(bytes).unwrap();
    for mutation in 0..5 {
        let mut inventory = super::parse(bytes).unwrap();
        match mutation {
            0 => {
                inventory.resources.pop();
            }
            1 => {
                inventory.resources[0].version = Some("wrong".into());
            }
            2 => {
                inventory.resources[0].kind = ResourceKind::Cask;
            }
            3 => {
                inventory.resources[0].name = "wrong".into();
            }
            _ => {
                if let Some(HomebrewMetadata::Formula { installed, .. }) =
                    &mut inventory.resources[0].homebrew
                {
                    installed[0].installed_on_request = Some(false);
                }
            }
        }
        assert!(std::panic::catch_unwind(|| verify(&raw, &inventory)).is_err());
    }
}

#[test]
fn unknown_explanations_cover_conflicts_and_partial_keg_evidence() {
    let conflict = serde_json::json!({"installed_on_request":true,"installed_as_dependency":true});
    let missing = serde_json::json!({});
    let requested = serde_json::json!({"installed_on_request":true});
    assert_eq!(
        expected_intent(&[&conflict]),
        Err("conflicting request/dependency flags")
    );
    assert_eq!(
        expected_intent(&[&requested, &missing]),
        Err("insufficient installation evidence in one or more records")
    );
}
