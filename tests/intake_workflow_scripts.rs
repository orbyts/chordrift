#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, path::PathBuf, process::Command};

use serde_json::Value;
use uuid::Uuid;

fn write_fake(path: &Path, body: &str) {
    fs::write(path, body).expect("fake binary is written");
    let mut permissions = fs::metadata(path)
        .expect("fake binary metadata exists")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("fake binary is executable");
}

fn temporary_work(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chordrift-{label}-{}", Uuid::new_v4()));
    fs::create_dir(&path).expect("temporary test directory is created");
    path
}

#[test]
fn installed_binary_capability_manifest_is_machine_readable() {
    let output = Command::new(env!("CARGO_BIN_EXE_chordrift"))
        .args([
            "capabilities",
            "--require",
            "maintenance.intake-workflow.v1",
            "--require",
            "maintenance.enumerated-playlist-additions.v1",
            "--require",
            "plan-origin.v1",
        ])
        .output()
        .expect("capability command executes");
    assert!(output.status.success());
    let manifest: Value = serde_json::from_slice(&output.stdout).expect("manifest is JSON");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(
        manifest["capabilities"]["maintenance.intake-workflow.v1"],
        "available"
    );
}

#[test]
fn review_only_wizard_uses_compatible_fake_binary_without_apply() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("wizard-compatible");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    write_fake(
        &fake,
        r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{"schema_version":1}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000011' 'operations: 0' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000011 --details")
    printf '%s\n' \
      'plan_id: 00000000-0000-0000-0000-000000000011' \
      'plan_origin: maintenance' \
      'snapshot_current: true' \
      'sequence	phase	operation	playlist	spotify_playlist_id	spotify_track_id	payload	safety'
    ;;
  "intake audit --account personal")
    printf '%s\n' \
      'intake audit: current' \
      'items: 0' \
      'previously_excluded: 0' \
      'known_from_history: 0' \
      'genuinely_new: 0' \
      'suggested_in_draft: 0' \
      'state	track	artists	sources	current_destinations	proposal_destinations	events	plays	exclusion_history	exclusion_reason	spotify_id'
    ;;
  *) printf 'unexpected fake command: %s\n' "$*" >&2; exit 90 ;;
esac
"##,
    );

    let output = Command::new(root.join("scripts/chordrift-intake-wizard.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .expect("review-only wizard executes");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).expect("fake command log exists");
    assert!(commands.lines().next().is_some_and(|line| {
        line.starts_with("capabilities --require maintenance.intake-workflow.v1")
    }));
    assert!(commands.contains("intake audit --account personal"));
    assert!(!commands.contains("sync apply"));
    assert!(!commands.contains("spotify "));
    fs::remove_dir_all(work).expect("temporary test directory is removed");
}

#[test]
fn every_recovered_helper_fails_closed_on_missing_capability() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (script, arguments) in [
        ("chordrift-intake-wizard.sh", vec!["--review-only"]),
        ("chordrift-cluster-unresolved.sh", vec![]),
        (
            "chordrift-manual-place.sh",
            vec![
                "--to",
                "Fixture",
                "--spotify-id",
                "abc123",
                "--reason",
                "fixture",
            ],
        ),
        (
            "chordrift-plan-phase.sh",
            vec![
                "--plan",
                "00000000-0000-0000-0000-000000000011",
                "--phase",
                "publish",
            ],
        ),
    ] {
        let work = temporary_work("missing-capability");
        let fake = work.join("chordrift-fake");
        let log = work.join("commands.log");
        write_fake(
            &fake,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>\"$FAKE_CHORDRIFT_LOG\"\nexit 64\n",
        );
        let output = Command::new(root.join("scripts").join(script))
            .args(arguments)
            .env("CHORDRIFT_BIN", &fake)
            .env("FAKE_CHORDRIFT_LOG", &log)
            .output()
            .expect("helper executes");
        assert_eq!(output.status.code(), Some(64), "{script}");
        let commands = fs::read_to_string(&log).expect("fake command log exists");
        assert_eq!(commands.lines().count(), 1, "{script}: {commands}");
        assert!(
            commands.starts_with("capabilities --require"),
            "{script}: {commands}"
        );
        fs::remove_dir_all(work).expect("temporary test directory is removed");
    }
}

#[test]
fn intake_wizard_rejects_spin_publication_plan_origin() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("spin-origin");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    write_fake(
        &fake,
        r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{"schema_version":1}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000012' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000012 --details")
    printf '%s\n' \
      'plan_id: 00000000-0000-0000-0000-000000000012' \
      'plan_origin: spin_publication' \
      'snapshot_current: true'
    ;;
  *) printf 'unexpected fake command: %s\n' "$*" >&2; exit 90 ;;
esac
"##,
    );

    let output = Command::new(root.join("scripts/chordrift-intake-wizard.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .expect("wizard executes");
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("spin_publication"));
    let commands = fs::read_to_string(&log).expect("fake command log exists");
    assert!(!commands.contains("intake audit"));
    assert!(!commands.contains("sync apply"));
    fs::remove_dir_all(work).expect("temporary test directory is removed");
}
