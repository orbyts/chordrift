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
    assert_eq!(
        manifest["capabilities"]["maintenance.bulk-plan-preview.v1"],
        "available"
    );
    assert_eq!(
        manifest["capabilities"]["maintenance.direct-managed-intake.v1"],
        "available"
    );
    assert_eq!(
        manifest["capabilities"]["maintenance.artwork-carry-forward.v1"],
        "available"
    );
    assert_eq!(
        manifest["capabilities"]["maintenance.provider-order-intent.v1"],
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
    assert!(!fs::read_to_string(&log).unwrap().contains("tracks inspect"));
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
      '0	reconcile	exclude_track	Old Vibe	old	fixture-track	{}	{}	Fixture Song	Fixture Artist	direct_move	Old Vibe	New Vibe'
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
    assert!(stdout.contains("Fixture Song — Fixture Artist · Old Vibe → New Vibe"));
    assert!(!stdout.contains("Needs a destination"));
    assert!(
        !fs::read_to_string(&log)
            .unwrap()
            .contains("proposals assign")
    );
    assert!(!fs::read_to_string(&log).unwrap().contains("reevaluate"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn provider_drift_removal_is_recognized_as_a_direct_move_before_apply() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-provider-drift-move");
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
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000041' 'operations: 1' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000041 --details")
    printf '%b\n' 'plan_id: 00000000-0000-0000-0000-000000000041' \
      'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety' \
      '0\treconcile\tremove_track\tNew Vibe\tnew\tfixture-track\t{"reason":"managed_provider_drift"}\t{}\tFixture Song\tFixture Artist\tdirect_move\tOld Vibe\tNew Vibe'
    ;;
  "tracks inspect --account personal --spotify-id fixture-track")
    printf '%s\n' 'track: Fixture Song — Fixture Artist' 'current_playlists: 1' \
      '  - New Vibe (position 1, role managed, signal canonical)' \
      'canonical_placements: 1' \
      '  - Old Vibe (position 4, key playlist-old, source approved)'
    ;;
  "intake audit --account personal")
    printf '%b\n' 'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id'
    ;;
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fixture Song — Fixture Artist · Old Vibe → New Vibe"));
    assert!(!stdout.contains("fixture-track"));
    assert!(!fs::read_to_string(&log).unwrap().contains("sync apply"));
    assert!(!fs::read_to_string(&log).unwrap().contains("tracks inspect"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn provider_drift_move_updates_intent_without_a_provider_apply() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-provider-drift-intent");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    let first_plan = "00000000-0000-0000-0000-000000000061";
    let second_plan = "00000000-0000-0000-0000-000000000062";
    let proposal = "00000000-0000-0000-0000-000000000063";
    write_fake(
        &fake,
        &format!(
            r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{{"schema_version":1}}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal")
    count=$(grep -c '^sync plan --account personal$' "$FAKE_CHORDRIFT_LOG")
    if [ "$count" -eq 1 ]; then
      printf '%s\n' 'plan_id: {first_plan}' 'operations: 2'
    else
      printf '%s\n' 'plan_id: {second_plan}' 'operations: 0'
    fi
    ;;
  "sync plan-show --account personal --plan {first_plan} --details")
    printf '%b\n' 'plan_id: {first_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety' \
      '0\treconcile\tremove_track\tNew Vibe\tnew\tfixture-track\t{{"reason":"managed_provider_drift"}}\t{{}}\tFixture Song\tFixture Artist\tdirect_move\tOld Vibe\tNew Vibe' \
      '1\treconcile\tremove_track\tNew Vibe\tnew\tfixture-track-2\t{{"reason":"managed_provider_drift"}}\t{{}}\tSecond Song\tFixture Artist\tdirect_move\tOld Vibe\tNew Vibe'
    ;;
  "sync plan-show --account personal --plan {second_plan} --details")
    printf '%b\n' 'plan_id: {second_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety'
    ;;
  "tracks inspect --account personal --spotify-id fixture-track")
    printf '%s\n' 'track: Fixture Song — Fixture Artist' 'current_playlists: 1' \
      '  - New Vibe (position 1, role managed, signal canonical)' \
      'canonical_placements: 1' \
      '  - Old Vibe (position 4, key playlist-old, source approved)'
    ;;
  "tracks inspect --account personal --spotify-id fixture-track-2")
    printf '%s\n' 'track: Second Song — Fixture Artist' 'current_playlists: 1' \
      '  - New Vibe (position 2, role managed, signal canonical)' \
      'canonical_placements: 1' \
      '  - Old Vibe (position 5, key playlist-old, source approved)'
    ;;
  "intake audit --account personal")
    printf '%b\n' 'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id'
    ;;
  "proposals status --account personal")
    printf '%s\n' 'proposal: proposed' 'generation_id: {proposal}' 'coverage_complete: true'
    ;;
  "proposals list --account personal")
    printf '%b\n' 'position\tcount\tstable_key\tname' '1\t1\tplaylist-new\tNew Vibe'
    ;;
  "proposals assign --account personal --spotify-id fixture-track --spotify-id fixture-track-2 --playlist playlist-new --reason Inferred from direct provider move")
    printf '%s\n' 'proposal: proposed'
    ;;
  "proposals approve --account personal --confirm {proposal}") printf '%s\n' 'proposal: approved' ;;
  "artwork status --account personal") printf '%s\n' 'proposal_generation_id: {proposal}' ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##
        ),
    );

    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .args(["--confirmed-plan", first_plan])
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).unwrap();
    assert_eq!(
        commands
            .matches("proposals assign --account personal")
            .count(),
        1
    );
    assert!(commands.contains(
        "--spotify-id fixture-track --spotify-id fixture-track-2 --playlist playlist-new"
    ));
    assert!(!commands.contains("sync readiness"));
    assert!(!commands.contains("sync apply"));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Detected move: Fixture Song — Fixture Artist · Old Vibe → New Vibe")
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Recording 2 inferred move(s) in Chordrift")
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Recorded 2 move(s) in Chordrift"));
    assert!(!commands.contains("tracks inspect"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn direct_managed_addition_records_existing_destination_without_provider_apply() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("direct-managed-intake");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    let first_plan = "00000000-0000-0000-0000-000000000071";
    let second_plan = "00000000-0000-0000-0000-000000000072";
    let proposal = "00000000-0000-0000-0000-000000000073";
    write_fake(
        &fake,
        &format!(
            r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{{"schema_version":1}}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal")
    count=$(grep -c '^sync plan --account personal$' "$FAKE_CHORDRIFT_LOG")
    if [ "$count" -eq 1 ]; then
      printf '%s\n' 'plan_id: {first_plan}' 'operations: 0'
    else
      printf '%s\n' 'plan_id: {second_plan}' 'operations: 0'
    fi
    ;;
  "sync plan-show --account personal --plan {first_plan} --details")
    printf '%b\n' 'plan_id: {first_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety'
    ;;
  "sync plan-show --account personal --plan {second_plan} --details")
    printf '%b\n' 'plan_id: {second_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety'
    ;;
  "intake audit --account personal")
    printf '%b\n' \
      'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id' \
      'direct_managed_addition\tNew Song\tFixture Artist\tNew Vibe\tNew Vibe\t\t0\t0\tfalse\t-\tfixture-new-track'
    ;;
  "proposals status --account personal")
    printf '%s\n' 'proposal: proposed' 'generation_id: {proposal}' 'coverage_complete: true'
    ;;
  "proposals list --account personal")
    printf '%b\n' 'position\tcount\tstable_key\tname' '1\t1\tplaylist-new\tNew Vibe'
    ;;
  "proposals assign --account personal --spotify-id fixture-new-track --playlist playlist-new --reason Inferred from direct provider move")
    printf '%s\n' 'proposal: proposed'
    ;;
  "proposals approve --account personal --confirm {proposal}") printf '%s\n' 'proposal: approved' ;;
  "artwork status --account personal") printf '%s\n' 'proposal_generation_id: old-proposal' ;;
  artwork\ import\ --account\ personal\ --manifest\ *drift-atlas-v5-indian-surfaces/.chordrift-maintain.*)
    printf '%s\n' 'batch_id: 00000000-0000-0000-0000-000000000074'
    ;;
  "artwork approve --account personal --confirm 00000000-0000-0000-0000-000000000074")
    printf '%s\n' 'artwork: approved'
    ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##
        ),
    );

    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .args(["--confirmed-plan", first_plan])
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).unwrap();
    assert!(commands.contains(
        "proposals assign --account personal --spotify-id fixture-new-track --playlist playlist-new"
    ));
    assert!(!commands.contains("sync apply"));
    assert!(!commands.contains("tracks restore"));
    assert!(commands.contains("drift-atlas-v5-indian-surfaces/.chordrift-maintain."));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Detected direct intake: New Song — Fixture Artist → New Vibe")
    );
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn excluded_track_in_managed_destination_is_not_restored_automatically() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("excluded-managed-intake");
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
  "sync plan --account personal") printf '%s\n' 'plan_id: 00000000-0000-0000-0000-000000000081' 'operations: 1' ;;
  "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000081 --details")
    printf '%b\n' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety' \
      '0\treconcile\tremove_track\tNew Vibe\tnew\tfixture-excluded\t{}\t{}\tExcluded Song\tFixture Artist\tordinary\t-\t-'
    ;;
  "intake audit --account personal")
    printf '%b\n' \
      'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id' \
      'previously_excluded\tExcluded Song\tFixture Artist\tNew Vibe\tNew Vibe\t\t0\t0\ttrue\tuser exclusion\tfixture-excluded'
    ;;
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
    let commands = fs::read_to_string(&log).unwrap();
    assert!(!commands.contains("proposals assign"));
    assert!(!commands.contains("tracks restore"));
    assert!(!commands.contains("sync apply"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Needs a destination"));
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn membership_equal_reorder_is_accepted_in_neon_without_provider_apply() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("provider-order-intent");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    let first_plan = "00000000-0000-0000-0000-000000000091";
    let second_plan = "00000000-0000-0000-0000-000000000092";
    let proposal = "00000000-0000-0000-0000-000000000093";
    write_fake(
        &fake,
        &format!(
            r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{{"schema_version":1}}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal")
    count=$(grep -c '^sync plan --account personal$' "$FAKE_CHORDRIFT_LOG")
    if [ "$count" -eq 1 ]; then printf '%s\n' 'plan_id: {first_plan}' 'operations: 1';
    else printf '%s\n' 'plan_id: {second_plan}' 'operations: 0'; fi
    ;;
  "sync plan-show --account personal --plan {first_plan} --details")
    printf '%b\n' 'plan_id: {first_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety' \
      '0\tpublish\treorder_playlist\tCelluloid Mehfil\tplaylist\t-\t{{"track_count":7}}\t{{"membership_unchanged":true}}\t-\t-\tordinary\t-\t-'
    ;;
  "sync plan-show --account personal --plan {second_plan} --details")
    printf '%b\n' 'plan_id: {second_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety'
    ;;
  "intake audit --account personal")
    printf '%b\n' 'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id'
    ;;
  "proposals status --account personal")
    count=$(grep -c '^proposals status --account personal$' "$FAKE_CHORDRIFT_LOG")
    if [ "$count" -eq 1 ]; then
      printf '%s\n' 'proposal: approved' 'generation_id: old-proposal' 'coverage_complete: true'
    else
      printf '%s\n' 'proposal: proposed' 'generation_id: {proposal}' 'coverage_complete: true'
    fi
    ;;
  "proposals extend --account personal --min-similarity 1") printf '%s\n' 'proposal: proposed' ;;
  "proposals list --account personal")
    printf '%b\n' 'position\tcount\tstable_key\tname' '1\t7\tplaylist-celluloid\tCelluloid Mehfil'
    ;;
  "proposals align-provider-order --account personal --playlist playlist-celluloid")
    printf '%s\n' 'proposal_order: aligned'
    ;;
  "proposals approve --account personal --confirm {proposal}") printf '%s\n' 'proposal: approved' ;;
  "artwork status --account personal") printf '%s\n' 'proposal_generation_id: {proposal}' ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##
        ),
    );
    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .args(["--confirmed-plan", first_plan])
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).unwrap();
    assert!(commands.contains("proposals align-provider-order --account personal"));
    assert!(!commands.contains("sync apply"));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Accepting current Spotify order: Celluloid Mehfil")
    );
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn one_confirmation_never_applies_work_from_the_next_plan() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = temporary_work("unified-confirmation-boundary");
    let fake = work.join("chordrift-fake");
    let log = work.join("commands.log");
    let first_plan = "00000000-0000-0000-0000-000000000051";
    let second_plan = "00000000-0000-0000-0000-000000000052";
    let assessment = "00000000-0000-0000-0000-000000000053";
    let apply = "00000000-0000-0000-0000-000000000054";
    write_fake(
        &fake,
        &format!(
            r##"#!/bin/sh
set -eu
printf '%s\n' "$*" >>"$FAKE_CHORDRIFT_LOG"
case "$*" in
  capabilities*) printf '%s\n' '{{"schema_version":1}}' ;;
  "sync pull --account personal") printf '%s\n' 'sync: current' ;;
  "sync plan --account personal")
    count=$(grep -c '^sync plan --account personal$' "$FAKE_CHORDRIFT_LOG")
    if [ "$count" -eq 1 ]; then
      printf '%s\n' 'plan_id: {first_plan}' 'operations: 1'
    else
      printf '%s\n' 'plan_id: {second_plan}' 'operations: 1'
    fi
    ;;
  "sync plan-show --account personal --plan {first_plan} --details")
    printf '%b\n' 'plan_id: {first_plan}' 'plan_origin: maintenance' 'snapshot_current: true' \
      'sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety' \
      '0\treconcile\texclude_track\tOld Vibe\told\tfixture-track\t{{}}\t{{}}\tFixture Song\tFixture Artist\tordinary\t-\t-'
    ;;
  "tracks inspect --account personal --spotify-id fixture-track")
    printf '%s\n' 'track: Fixture Song — Fixture Artist' 'current_playlists: 0' 'canonical_placements: 0'
    ;;
  "intake audit --account personal")
    printf '%b\n' 'state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id'
    ;;
  "sync readiness --account personal --plan {first_plan} --probe")
    printf '%s\n' 'assessment_id: {assessment}' 'apply_readiness: ready'
    ;;
  "sync apply --account personal --assessment {assessment} --phase reconcile --confirm {assessment}")
    printf '%s\n' 'apply_run_id: {apply}'
    ;;
  "sync apply-show --account personal --run {apply}") printf '%s\n' 'spotify_apply: succeeded' ;;
  *) printf 'unexpected: %s\n' "$*" >&2; exit 90 ;;
esac
"##
        ),
    );

    let output = Command::new(root.join("scripts/chordrift-maintain.sh"))
        .args(["--confirmed-plan", first_plan])
        .env("CHORDRIFT_BIN", &fake)
        .env("FAKE_CHORDRIFT_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = fs::read_to_string(&log).unwrap();
    assert_eq!(commands.matches("sync apply --account personal").count(), 1);
    assert_eq!(commands.matches("sync plan --account personal").count(), 2);
    assert!(
        !commands.contains(
            "sync plan-show --account personal --plan 00000000-0000-0000-0000-000000000052"
        )
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fixture Song — Fixture Artist"));
    assert!(!stdout.contains("fixture-track"));
    assert!(!stdout.contains(first_plan));
    assert!(!commands.contains("tracks inspect"));
    fs::remove_dir_all(work).unwrap();
}
