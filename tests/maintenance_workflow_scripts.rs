#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, path::PathBuf, process::Command};

use serde_json::Value;
use uuid::Uuid;

fn write_fake(path: &Path, body: &str) {
    fs::write(path, body).expect("fake binary is written");
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn temporary_work(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chordrift-{label}-{}", Uuid::new_v4()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn installed_binary_advertises_unified_maintenance() {
    let output = Command::new(env!("CARGO_BIN_EXE_chordrift"))
        .args([
            "capabilities",
            "--require",
            "maintenance.unified-workflow.v1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let manifest: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        manifest["capabilities"]["maintenance.unified-workflow.v1"],
        "available"
    );
}

#[test]
fn unified_review_uses_one_observation_and_never_applies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-maintenance-review");
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
  "sync plan --account personal")
    printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000011' 'operations: 0'
    ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000011 --details")
    printf '%s\n' \
      'plan_id: 00000000-0000-0000-0000-000000000011' \
      'plan_origin: maintenance' \
      'snapshot_current: true' \
      'sequence	phase	operation	playlist	spotify_playlist_id	spotify_track_id	payload	safety'
    ;;
  "intake audit --account personal")
    printf '%s\n' 'state	track	artists	sources	current_destinations	proposal_destinations	events	plays	exclusion_history	exclusion_reason	spotify_id'
    ;;
  "reevaluate status --account personal") printf '%s\n' 'queue: Re-evaluate' 'tracks: 0' ;;
  "playlists tracks --account personal --name Re-evaluate")
    printf '%s\n' 'position	track	artists	album	spotify_track_id'
    ;;
  *) printf 'unexpected fake command: %s\n' "$*" >&2; exit 90 ;;
esac
"##,
    );

    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).unwrap();
    assert_eq!(commands.matches("sync pull --account personal").count(), 1);
    assert!(!commands.contains("sync apply"));
    assert!(!commands.contains("proposals extend"));
    assert!(!commands.contains("reevaluate"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn unified_review_rejects_spin_publication() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-maintenance-origin");
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
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000012' 'operations: 1' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000012 --details")
    printf '%s\n' 'plan_origin: spin_publication' 'snapshot_current: true'
    ;;
  *) exit 90 ;;
esac
"##,
    );
    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(!fs::read_to_string(&log).unwrap().contains("sync apply"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn retired_queue_is_not_consulted_when_cleanup_id_list_is_empty() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-empty-cleanup-list");
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
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000013' 'operations: 1' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000013 --details")
    printf '%s\n' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence	phase	operation	playlist	spotify_playlist_id	spotify_track_id	payload	safety' \
      '0	retirement	archive_playlist	Re-evaluate	queue	-	{"surface":"retired_reevaluate"}	{"queue_empty":true}'
    ;;
  "intake audit --account personal") printf '%s\n' 'state	track	artists	sources	current_destinations	proposal_destinations	events	plays	exclusion_history	exclusion_reason	spotify_id' ;;
  "reevaluate status --account personal") printf '%s\n' 'queue: Re-evaluate' 'tracks: 1' ;;
  "playlists tracks --account personal --name Re-evaluate")
    printf '%s\n' 'position	track	artists	album	spotify_track_id' \
      '1	Fixture Song	Fixture Artist	Fixture Album	fixture-track'
    ;;
  *) exit 90 ;;
esac
"##,
    );
    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("separately reviewed retirement remains pending"));
    assert!(!stdout.contains("Needs a destination"));
    assert!(!stdout.contains("Fixture Song"));
    assert!(
        !fs::read_to_string(&log)
            .unwrap()
            .contains("proposals extend")
    );
    assert!(!fs::read_to_string(&log).unwrap().contains("reevaluate"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn confirmed_cleanup_phase_retains_destructive_gate_without_another_prompt() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-cleanup-phase");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    let plan = "00000000-0000-0000-0000-000000000021";
    let assessment = "00000000-0000-0000-0000-000000000022";
    let apply = "00000000-0000-0000-0000-000000000023";
    write_fake(
        &fake,
        &format!(
            r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{{"schema_version":1}}' ;;
  "sync plan-show --account personal --plan {plan} --details")
    printf '%s\n' 'plan_id: {plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence	phase	operation	playlist	spotify_playlist_id	spotify_track_id	payload	safety' \
      '0	cleanup	remove_track	Re-evaluate	queue	track	{{}}	{{}}'
    ;;
  "sync readiness --account personal --plan {plan} --probe") printf '%s\n' 'assessment_id: {assessment}' 'apply_readiness: ready' ;;
  "sync apply --account personal --assessment {assessment} --phase cleanup --confirm {assessment} --allow-destructive") printf '%s\n' 'apply_run_id: {apply}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync apply-show --account personal --run {apply}") printf '%s\n' 'spotify_apply: succeeded' ;;
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000024' ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##
        ),
    );
    let output = Command::new(root.join("scripts/chordrift-plan-phase.sh"))
        .args([
            "--account",
            "personal",
            "--plan",
            plan,
            "--phase",
            "cleanup",
            "--workflow-confirmation",
            plan,
            "--concise",
        ])
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .contains("--allow-destructive")
    );
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn one_clear_provider_move_is_inferred_without_a_destination_prompt() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-direct-move");
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
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000031' 'operations: 1' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000031 --details")
    printf '%s\n' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence	phase	operation	playlist	spotify_playlist_id	spotify_track_id	payload	safety' \
      '0	reconcile	exclude_track	Old Vibe	old	fixture-track	{}	{}'
    ;;
  "tracks inspect --account personal --spotify-id fixture-track")
    printf '%s\n' 'track: Fixture Song — Fixture Artist' 'current_playlists: 1' \
      '  - New Vibe (position 1, role managed, signal canonical)' 'canonical_placements: 1'
    ;;
  "intake audit --account personal") printf '%s\n' 'state	track	artists	sources	current_destinations	proposal_destinations	events	plays	exclusion_history	exclusion_reason	spotify_id' ;;
  "reevaluate status --account personal") printf '%s\n' 'queue: Re-evaluate' 'tracks: 0' ;;
  "playlists tracks --account personal --name Re-evaluate") printf '%s\n' 'position	track	artists	album	spotify_track_id' ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##,
    );
    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .arg("--review-only")
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fixture Song — Fixture Artist → New Vibe"));
    assert!(!stdout.contains("Needs a destination"));
    assert!(
        !fs::read_to_string(&log)
            .unwrap()
            .contains("proposals assign")
    );
    assert!(!fs::read_to_string(&log).unwrap().contains("reevaluate"));
    fs::remove_dir_all(work).unwrap();
}
