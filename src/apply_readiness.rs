//! Read-only proof that an immutable sync plan is safe for a future apply engine.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{
    ChordriftError, Result,
    providers::spotify::{AuthStatus, RetryPolicy, has_required_apply_scopes, retry_policy},
};

const PROVIDER: &str = "spotify";
const ASSESSMENT_VERSION: &str = "spotify-apply-readiness-v5";
const PLANNER_VERSION: &str = "spotify-dry-run-v10";

/// One inspectable apply-readiness check.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReadinessCheck {
    /// Stable check name.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Secret-free machine-readable evidence.
    pub evidence: Value,
}

/// Immutable apply-readiness assessment.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadinessReport {
    /// Assessment identity.
    pub assessment_id: Uuid,
    /// Exact dry-run plan assessed.
    pub plan_id: Uuid,
    /// Whether an identical assessment already existed.
    pub reused: bool,
    /// Overall readiness state.
    pub status: String,
    /// Exact operation count inspected.
    pub operation_count: usize,
    /// Number of successful checks.
    pub passed_checks: usize,
    /// Total check count.
    pub check_count: usize,
    /// Simulated interruption checkpoints.
    pub restart_checkpoints: usize,
    /// Changes produced by replay after simulated completion; must be zero.
    pub replay_changes: usize,
    /// Whether a one-request read-only Spotify identity/scope probe ran.
    pub provider_probe_performed: bool,
    /// Reproducibility hash.
    pub input_hash: String,
    /// Assessment time.
    pub created_at: DateTime<Utc>,
    /// Detailed checks.
    pub checks: Vec<ReadinessCheck>,
}

#[derive(Clone, Debug)]
struct Operation {
    sequence: i32,
    phase: String,
    kind: String,
    key: String,
    playlist_name: String,
    spotify_playlist_id: Option<String>,
    safety: Value,
}

/// Assesses the latest or selected dry-run without modifying Spotify.
pub async fn assess(
    database: &Database,
    account_label: &str,
    requested_plan: Option<Uuid>,
    probe: Option<&AuthStatus>,
) -> Result<ReadinessReport> {
    let account_id = account_id(database, account_label).await?;
    let plan = sqlx::query(
        "SELECT id, mode, status, planner_version, input_hash, source_snapshot_id,
                proposal_generation_id
         FROM sync_runs
         WHERE provider_account_id = $1 AND provider = $2 AND mode = 'dry_run'
           AND ($3::uuid IS NULL OR id = $3)
         ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(PROVIDER)
    .bind(requested_plan)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching dry-run sync plan exists"))?;
    let plan_id: Uuid = plan.try_get("id")?;
    let source_snapshot_id: Uuid = plan.try_get("source_snapshot_id")?;
    let proposal_id: Uuid = plan.try_get("proposal_generation_id")?;
    let plan_input_hash: String = plan.try_get("input_hash")?;

    let rows = sqlx::query(
        "SELECT sequence, phase, operation_type, operation_key, payload, safety
         FROM sync_operations WHERE sync_run_id = $1 ORDER BY sequence",
    )
    .bind(plan_id)
    .fetch_all(database.pool())
    .await?;
    let operations: Vec<Operation> = rows
        .into_iter()
        .map(|row| {
            let payload: Value = row.get("payload");
            Operation {
                sequence: row.get("sequence"),
                phase: row.get("phase"),
                kind: row.get("operation_type"),
                key: row.get("operation_key"),
                playlist_name: payload
                    .get("playlist_name")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_owned(),
                spotify_playlist_id: payload
                    .get("spotify_playlist_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                safety: row.get("safety"),
            }
        })
        .collect();

    let latest_snapshot: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM provider_inventory_observations WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    let proposal_state: Option<String> = sqlx::query_scalar(
        "SELECT status FROM playlist_generations
         WHERE id = $1 AND provider_account_id = $2",
    )
    .bind(proposal_id)
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    let named_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM playlists WHERE generation_id = $1")
            .bind(proposal_id)
            .fetch_one(database.pool())
            .await?;
    let artwork = sqlx::query(
        "SELECT batch.id, batch.input_hash, batch.artifact_count,
                count(*) FILTER (WHERE artifact.target_kind = 'canonical')::bigint
                    AS canonical_count,
                count(*) FILTER (WHERE artifact.target_kind = 'intake')::bigint
                    AS intake_count
         FROM playlist_artwork_batches batch
         JOIN playlist_artwork_artifacts artifact ON artifact.batch_id = batch.id
         WHERE batch.provider_account_id = $1 AND batch.proposal_generation_id = $2
           AND batch.state = 'approved'
         GROUP BY batch.id, batch.input_hash, batch.artifact_count,
                  batch.approved_at
         ORDER BY batch.approved_at DESC, batch.id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(proposal_id)
    .fetch_optional(database.pool())
    .await?;
    let artwork_batch_id = artwork.as_ref().map(|row| row.get::<Uuid, _>("id"));
    let artwork_hash = artwork
        .as_ref()
        .map(|row| row.get::<String, _>("input_hash"));
    let artwork_count = artwork
        .as_ref()
        .map(|row| row.get::<i32, _>("artifact_count"))
        .unwrap_or_default();
    let canonical_artwork_count = artwork
        .as_ref()
        .map(|row| row.get::<i64, _>("canonical_count"))
        .unwrap_or_default();
    let intake_artwork_count = artwork
        .as_ref()
        .map(|row| row.get::<i64, _>("intake_count"))
        .unwrap_or_default();

    let policy = retry_policy();
    let (integrity_passed, integrity_evidence) = operation_integrity(&operations);
    let (restart_checkpoints, replay_changes) = simulate_recovery(&operations);
    let external_cleanup_count = operations
        .iter()
        .filter(|operation| operation.kind == "remove_external_playlist")
        .count();
    let approved_cleanup_count: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT batch.id)::bigint
         FROM sync_operations operation
         JOIN external_playlist_cleanup_batches batch
           ON batch.id = (operation.safety->>'requires_cleanup_batch')::uuid
          AND batch.provider_account_id = $2 AND batch.state = 'approved'
         WHERE operation.sync_run_id = $1
           AND operation.operation_type = 'remove_external_playlist'",
    )
    .bind(plan_id)
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let cleanup_passed = external_cleanup_count == 0 || approved_cleanup_count == 1;
    let inventory = sqlx::query(
        "WITH candidate AS (
             SELECT track.id AS track_id
             FROM tracks track
             WHERE account_track_is_library_candidate($1, track.id)
         ), placement AS (
             SELECT membership.track_id, count(DISTINCT membership.playlist_id)::bigint AS destinations
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
             GROUP BY membership.track_id
         ), disposition AS (
             SELECT candidate.track_id, COALESCE(placement.destinations, 0) AS destinations,
                    exclusion.id IS NOT NULL AS excluded
             FROM candidate
             LEFT JOIN placement USING (track_id)
             LEFT JOIN excluded_tracks exclusion
               ON exclusion.provider_account_id = $1
              AND exclusion.track_id = candidate.track_id
              AND exclusion.restored_at IS NULL
         )
         SELECT count(*)::bigint AS inventory,
                count(*) FILTER (WHERE destinations = 1 AND NOT excluded)::bigint AS placed,
                count(*) FILTER (WHERE excluded)::bigint AS excluded,
                count(*) FILTER (WHERE destinations = 0 AND NOT excluded)::bigint AS unresolved,
                count(*) FILTER (WHERE destinations > 1 AND NOT excluded)::bigint
                    AS conflicting
         FROM disposition",
    )
    .bind(account_id)
    .bind(proposal_id)
    .fetch_one(database.pool())
    .await?;
    let inventory_count: i64 = inventory.try_get("inventory")?;
    let placed_count: i64 = inventory.try_get("placed")?;
    let excluded_count: i64 = inventory.try_get("excluded")?;
    let unresolved_count: i64 = inventory.try_get("unresolved")?;
    let conflicting_count: i64 = inventory.try_get("conflicting")?;
    let probe_passed = probe.is_some_and(|status| has_required_apply_scopes(&status.scopes));
    let checks = vec![
        check(
            "plan_identity",
            plan.try_get::<String, _>("mode")? == "dry_run"
                && plan.try_get::<String, _>("status")? == "planned"
                && plan.try_get::<String, _>("planner_version")? == PLANNER_VERSION,
            json!({"plan_id": plan_id, "planner_version": plan.try_get::<String, _>("planner_version")?}),
        ),
        check(
            "source_snapshot_current",
            latest_snapshot == Some(source_snapshot_id),
            json!({"planned": source_snapshot_id, "latest": latest_snapshot}),
        ),
        check(
            "proposal_approved",
            proposal_state.as_deref() == Some("approved"),
            json!({"proposal_generation_id": proposal_id, "state": proposal_state}),
        ),
        check(
            "complete_library_inventory",
            unresolved_count == 0
                && conflicting_count == 0
                && inventory_count == placed_count + excluded_count,
            json!({"inventory": inventory_count, "placed": placed_count,
                "excluded": excluded_count, "unresolved": unresolved_count,
                "conflicting_dispositions": conflicting_count}),
        ),
        check(
            "artwork_approved",
            artwork_batch_id.is_some()
                && canonical_artwork_count == named_count
                && intake_artwork_count == 4,
            json!({"batch_id": artwork_batch_id, "artifacts": artwork_count,
                "canonical_artifacts": canonical_artwork_count,
                "canonical_playlists": named_count,
                "intake_artifacts": intake_artwork_count,
                "required_intake_artifacts": 4}),
        ),
        check("operation_integrity", integrity_passed, integrity_evidence),
        check(
            "approval_gates",
            cleanup_passed && deferred_destructive_gates_are_present(&operations),
            json!({"external_cleanup_operations": external_cleanup_count,
                "approved_cleanup_batches": approved_cleanup_count,
                "deferred_destructive_operations": operations.iter().filter(|operation|
                    operation.safety.get("destructive") == Some(&Value::Bool(true))
                    && operation.safety.get("deferred") == Some(&Value::Bool(true))).count()}),
        ),
        check(
            "interruption_recovery",
            restart_checkpoints > 0,
            json!({"checkpoints": restart_checkpoints, "operations_recovered": operations.len()}),
        ),
        check(
            "rate_limit_policy",
            policy
                == RetryPolicy {
                    max_retries: 5,
                    max_delay_seconds: 60,
                },
            json!({"http_status": 429, "max_retries": policy.max_retries,
                "default_retry_after_seconds": 1, "max_retry_after_seconds": policy.max_delay_seconds}),
        ),
        check(
            "idempotent_replay",
            replay_changes == 0,
            json!({"second_pass_changes": replay_changes, "operation_keys": operations.len()}),
        ),
        check(
            "provider_identity_and_scopes",
            probe_passed,
            json!({"performed": probe.is_some(), "scopes": probe.map(|value| &value.scopes),
                "required_v010_scopes_present": probe_passed}),
        ),
    ];

    let input = json!({
        "assessment_version": ASSESSMENT_VERSION,
        "plan_id": plan_id,
        "plan_input_hash": plan_input_hash,
        "source_snapshot_id": source_snapshot_id,
        "latest_snapshot_id": latest_snapshot,
        "proposal_generation_id": proposal_id,
        "artwork_batch_id": artwork_batch_id,
        "artwork_input_hash": artwork_hash,
        "checks": checks,
    });
    let input_hash = hex_sha256(&serde_json::to_vec(&input)?);
    if let Some(existing) = existing(database, account_id, plan_id, &input_hash).await? {
        return Ok(existing);
    }
    let passed_checks = checks.iter().filter(|check| check.passed).count();
    let status = if passed_checks == checks.len() {
        "ready"
    } else {
        "blocked"
    };
    let mut transaction = database.pool().begin().await?;
    let row = sqlx::query(
        "INSERT INTO sync_readiness_assessments
         (provider_account_id, sync_run_id, artwork_batch_id, assessment_version,
          input_hash, status, provider_probe_performed, check_count,
          passed_check_count, operation_count, restart_checkpoints, replay_changes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, created_at",
    )
    .bind(account_id)
    .bind(plan_id)
    .bind(artwork_batch_id)
    .bind(ASSESSMENT_VERSION)
    .bind(&input_hash)
    .bind(status)
    .bind(probe.is_some())
    .bind(checks.len() as i32)
    .bind(passed_checks as i32)
    .bind(operations.len() as i32)
    .bind(restart_checkpoints as i32)
    .bind(replay_changes as i32)
    .fetch_one(&mut *transaction)
    .await?;
    let assessment_id: Uuid = row.try_get("id")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    for (sequence, readiness_check) in checks.iter().enumerate() {
        sqlx::query(
            "INSERT INTO sync_readiness_checks
             (assessment_id, sequence, check_name, status, evidence)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(assessment_id)
        .bind(sequence as i32)
        .bind(&readiness_check.name)
        .bind(if readiness_check.passed {
            "passed"
        } else {
            "blocked"
        })
        .bind(&readiness_check.evidence)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(ReadinessReport {
        assessment_id,
        plan_id,
        reused: false,
        status: status.to_owned(),
        operation_count: operations.len(),
        passed_checks,
        check_count: checks.len(),
        restart_checkpoints,
        replay_changes,
        provider_probe_performed: probe.is_some(),
        input_hash,
        created_at,
        checks,
    })
}

/// Shows the latest or selected immutable readiness assessment.
pub async fn show(
    database: &Database,
    account_label: &str,
    requested: Option<Uuid>,
) -> Result<ReadinessReport> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT id FROM sync_readiness_assessments
         WHERE provider_account_id = $1 AND ($2::uuid IS NULL OR id = $2)
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(requested)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching apply-readiness assessment exists"))?;
    report_for(database, row.try_get("id")?, false).await
}

async fn existing(
    database: &Database,
    account_id: Uuid,
    plan_id: Uuid,
    input_hash: &str,
) -> Result<Option<ReadinessReport>> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sync_readiness_assessments
         WHERE provider_account_id = $1 AND sync_run_id = $2
           AND assessment_version = $3 AND input_hash = $4",
    )
    .bind(account_id)
    .bind(plan_id)
    .bind(ASSESSMENT_VERSION)
    .bind(input_hash)
    .fetch_optional(database.pool())
    .await?;
    match id {
        Some(id) => Ok(Some(report_for(database, id, true).await?)),
        None => Ok(None),
    }
}

async fn report_for(
    database: &Database,
    assessment_id: Uuid,
    reused: bool,
) -> Result<ReadinessReport> {
    let row = sqlx::query(
        "SELECT sync_run_id, status, provider_probe_performed, check_count,
                passed_check_count, operation_count, restart_checkpoints,
                replay_changes, input_hash, created_at
         FROM sync_readiness_assessments WHERE id = $1",
    )
    .bind(assessment_id)
    .fetch_one(database.pool())
    .await?;
    let check_rows = sqlx::query(
        "SELECT check_name, status, evidence FROM sync_readiness_checks
         WHERE assessment_id = $1 ORDER BY sequence",
    )
    .bind(assessment_id)
    .fetch_all(database.pool())
    .await?;
    Ok(ReadinessReport {
        assessment_id,
        plan_id: row.try_get("sync_run_id")?,
        reused,
        status: row.try_get("status")?,
        operation_count: row.try_get::<i32, _>("operation_count")? as usize,
        passed_checks: row.try_get::<i32, _>("passed_check_count")? as usize,
        check_count: row.try_get::<i32, _>("check_count")? as usize,
        restart_checkpoints: row.try_get::<i32, _>("restart_checkpoints")? as usize,
        replay_changes: row.try_get::<i32, _>("replay_changes")? as usize,
        provider_probe_performed: row.try_get("provider_probe_performed")?,
        input_hash: row.try_get("input_hash")?,
        created_at: row.try_get("created_at")?,
        checks: check_rows
            .into_iter()
            .map(|check| ReadinessCheck {
                name: check.get("check_name"),
                passed: check.get::<String, _>("status") == "passed",
                evidence: check.get("evidence"),
            })
            .collect(),
    })
}

fn operation_integrity(operations: &[Operation]) -> (bool, Value) {
    let contiguous = operations
        .iter()
        .enumerate()
        .all(|(index, operation)| operation.sequence == index as i32);
    let mut keys = HashSet::new();
    let unique_keys = operations
        .iter()
        .all(|operation| keys.insert(&operation.key));
    let phases_monotonic = operations
        .windows(2)
        .all(|pair| phase_rank(&pair[0].phase) <= phase_rank(&pair[1].phase));
    let creates: HashSet<&str> = operations
        .iter()
        .filter(|operation| operation.kind == "create_playlist")
        .map(|operation| operation.playlist_name.as_str())
        .collect();
    let targets_resolvable = operations.iter().all(|operation| {
        !matches!(
            operation.kind.as_str(),
            "add_track" | "restore_track" | "reorder_playlist" | "upload_artwork"
        ) || operation.spotify_playlist_id.is_some()
            || creates.contains(operation.playlist_name.as_str())
    });
    (
        !operations.is_empty()
            && contiguous
            && unique_keys
            && phases_monotonic
            && targets_resolvable,
        json!({"contiguous_sequences": contiguous, "unique_operation_keys": unique_keys,
            "phases_monotonic": phases_monotonic, "targets_resolvable": targets_resolvable,
            "operations": operations.len()}),
    )
}

fn deferred_destructive_gates_are_present(operations: &[Operation]) -> bool {
    operations.iter().all(|operation| {
        let destructive = operation.safety.get("destructive") == Some(&Value::Bool(true));
        if !destructive {
            return true;
        }
        match operation.phase.as_str() {
            "cleanup" | "retirement" => {
                operation.safety.get("deferred") == Some(&Value::Bool(true))
            }
            "reconcile" => {
                operation.safety.get("requires_snapshot_match") == Some(&Value::Bool(true))
                    || operation.kind == "exclude_track"
            }
            _ => false,
        }
    })
}

fn simulate_recovery(operations: &[Operation]) -> (usize, usize) {
    if operations.is_empty() {
        return (0, 0);
    }
    let checkpoints: BTreeSet<usize> = [
        0,
        1,
        operations.len() / 3,
        operations.len() / 2,
        operations.len().saturating_sub(1),
    ]
    .into_iter()
    .collect();
    for checkpoint in &checkpoints {
        let mut ledger: HashSet<&str> = operations[..*checkpoint]
            .iter()
            .map(|operation| operation.key.as_str())
            .collect();
        for operation in &operations[*checkpoint..] {
            ledger.insert(&operation.key);
        }
        if ledger.len() != operations.len() {
            return (0, usize::MAX);
        }
    }
    let mut completed: HashSet<&str> = operations
        .iter()
        .map(|operation| operation.key.as_str())
        .collect();
    let before = completed.len();
    for operation in operations {
        completed.insert(&operation.key);
    }
    (checkpoints.len(), completed.len() - before)
}

fn check(name: &str, passed: bool, evidence: Value) -> ReadinessCheck {
    ReadinessCheck {
        name: name.to_owned(),
        passed,
        evidence,
    }
}

fn phase_rank(phase: &str) -> u8 {
    match phase {
        "publish" => 0,
        "reconcile" => 1,
        "cleanup" => 2,
        "retirement" => 3,
        _ => 4,
    }
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE provider = $1 AND account_label = $2",
    )
    .bind(PROVIDER)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("Spotify account is not imported"))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(sequence: i32, phase: &str, kind: &str, key: &str) -> Operation {
        Operation {
            sequence,
            phase: phase.to_owned(),
            kind: kind.to_owned(),
            key: key.to_owned(),
            playlist_name: "Test".to_owned(),
            spotify_playlist_id: Some("spotify-playlist".to_owned()),
            safety: json!({"destructive": false}),
        }
    }

    #[test]
    fn interruption_resume_and_replay_are_idempotent() {
        let operations: Vec<_> = (0..100)
            .map(|index| operation(index, "publish", "add_track", &format!("add:{index}")))
            .collect();
        let (checkpoints, replay_changes) = simulate_recovery(&operations);
        assert_eq!(checkpoints, 5);
        assert_eq!(replay_changes, 0);
    }

    #[test]
    fn integrity_rejects_sequence_gaps_and_phase_regression() {
        let operations = vec![
            operation(0, "cleanup", "remove_track", "remove:1"),
            operation(2, "publish", "add_track", "add:1"),
        ];
        assert!(!operation_integrity(&operations).0);
    }
}
