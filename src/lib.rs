#![deny(missing_docs)]
#![forbid(unsafe_code)]

//! Personal music-library intelligence and synchronization infrastructure.
//!
//! Chordrift owns its music domain and PostgreSQL schema. Storexa supplies the
//! lower-level Neon/PostgreSQL connection, pooling, and migration primitives.

/// Read-only saved-album inventory and account-scoped cleanup policy.
pub mod albums;
/// Account-scoped canonical library analysis.
pub mod analysis;
/// Gated, resumable execution of approved Spotify synchronization plans.
pub mod apply;
/// Read-only proof that an immutable sync plan is safe for a future apply engine.
pub mod apply_readiness;
/// Local-only, content-addressed playlist artwork approval records.
pub mod artwork;
/// Durable metadata and last-known contents for externally owned playlists.
pub mod bookmarks;
/// Revisioned user-authored track classifications and CSV review batches.
pub mod classifications;
/// Command-line parsing and execution.
pub mod cli;
/// Reproducible account-scoped vibe cluster generations.
pub mod clusters;
/// Application-owned database configuration.
pub mod config;
/// System credential storage with provider and account isolation.
pub mod credentials;
/// Storexa-backed database lifecycle and migration inspection.
pub mod db;
/// Exact-confirmed removal of superseded database-v1 storage.
pub mod db_cleanup;
/// Read-only database invariants, storage measurement, and compaction planning.
pub mod db_reports;
/// Exact-confirmed database-v2 evidence and checkpoint migration.
pub mod db_v2_migration;
/// Deterministic, versioned semantic track embeddings.
pub mod embeddings;
/// Cache-first, provenance-aware semantic metadata enrichment.
pub mod enrichment;
/// Privacy-conscious Spotify archive inspection and listening-history import.
pub mod history;
/// Read-only current-provider intake joined with durable intent and history.
pub mod intake;
/// Versioned pretrained audio-model inference artifacts.
pub mod model_inference;
/// Account-scoped playlist roles and drift policy.
pub mod playlists;
mod presentation;
/// Non-destructive, account-scoped proposed playlist libraries.
pub mod proposals;
/// Streaming-provider adapters.
pub mod providers;
/// Durable zero-signal routing playlists for ongoing listening review.
pub mod routes;
/// Versioned account-specific preference and lifecycle signals.
pub mod signals;
/// Immutable provider synchronization plans that never mutate remote services.
pub mod sync_plan;
mod terminal;
/// One-stop canonical track lookup and explainability reports.
pub mod tracks;

mod error;

pub use error::{ChordriftError, Result};
