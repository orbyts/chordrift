#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

use uuid::Uuid;

#[test]
fn installed_binary_helper_reaches_every_product_view_without_provider_commands() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = std::env::temp_dir().join(format!("chordrift-v02011-{}", Uuid::new_v4()));
    fs::create_dir(&work).expect("temporary test directory is created");
    let fake_binary = work.join("chordrift-fake");
    let log = work.join("commands.log");
    fs::write(
        &fake_binary,
        r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  "product --help") exit 0 ;;
  *"product onboarding capture"*"--mode inventory-only"*)
    printf '%s\n' 'session_id: 00000000-0000-0000-0000-000000000011'
    ;;
  *"product onboarding capture"*"--mode enriched"*)
    printf '%s\n' 'session_id: 00000000-0000-0000-0000-000000000012'
    ;;
  *"product onboarding audit"*"--mode inventory-only"*)
    printf '%s\n' \
      'audit_fingerprint: dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd' \
      'inventory_findings_fingerprint: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    ;;
  *"product onboarding audit"*"--mode enriched"*)
    printf '%s\n' \
      'audit_fingerprint: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' \
      'inventory_findings_fingerprint: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
      'strengthened_conclusions: 2'
    ;;
  "product collections list"*) printf '%s\n' 'collections: 2' ;;
  "product recipes show"*) printf '%s\n' 'product_view: recipe_revision' ;;
  "product recipes execute"*) printf '%s\n' 'draft_fingerprint: draft' ;;
  "product spins preview"*)
    printf '%s\n' \
      'spin_id: 00000000-0000-0000-0000-000000000013' \
      'preview_fingerprint: cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'
    ;;
  "product spins show"*)
    printf '%s\n' 'preview_fingerprint: cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'
    ;;
  *) printf 'unexpected fake command: %s\n' "$*" >&2; exit 90 ;;
esac
"##,
    )
    .expect("fake binary is written");
    let mut permissions = fs::metadata(&fake_binary)
        .expect("fake binary metadata exists")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_binary, permissions).expect("fake binary is executable");

    let account = "00000000-0000-0000-0000-000000000001";
    let revision = "00000000-0000-0000-0000-000000000002";
    let output = Command::new(root.join("scripts/chordrift-product-rehearsal.sh"))
        .args([
            "--account",
            account,
            "--recipe-revision",
            revision,
            "--onboarding-fixture",
            "onboarding.json",
            "--spin-fixture",
            "spin.json",
        ])
        .env("CHORDRIFT_BIN", &fake_binary)
        .env("CHORDRIFT_PRODUCT_REHEARSAL", "1")
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .expect("helper executes");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("helper output is UTF-8");
    assert!(stdout.contains("Rehearsal complete"));
    assert!(stdout.contains("Provider writes: disabled"));

    let commands = fs::read_to_string(&log).expect("fake command log exists");
    for expected in [
        "product onboarding capture",
        "product onboarding audit",
        "product collections list",
        "product recipes show",
        "product recipes execute",
        "product spins preview",
        "product spins show",
    ] {
        assert!(
            commands.contains(expected),
            "missing {expected}: {commands}"
        );
    }
    assert!(!commands.contains("spotify"));
    assert!(!commands.contains("sync apply"));
    assert!(!commands.contains("db migrate"));

    fs::remove_dir_all(work).expect("temporary test directory is removed");
}
