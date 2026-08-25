#![deny(missing_docs)]
#![forbid(unsafe_code)]

//! Personal music-library intelligence and synchronization infrastructure.
//!
//! Chordrift owns its music domain and PostgreSQL schema. Storexa supplies the
//! lower-level Neon/PostgreSQL connection, pooling, and migration primitives.

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
/// Deterministic, versioned semantic track embeddings.
pub mod embeddings;
/// Cache-first, provenance-aware semantic metadata enrichment.
pub mod enrichment;
/// Privacy-conscious Spotify archive inspection and listening-history import.
pub mod history;
/// Versioned pretrained audio-model inference artifacts.
pub mod model_inference;
/// Account-scoped playlist roles and drift policy.
pub mod playlists;
/// Non-destructive, account-scoped proposed playlist libraries.
pub mod proposals;
/// Streaming-provider adapters.
pub mod providers;
/// Versioned account-specific preference and lifecycle signals.
pub mod signals;
/// Immutable provider synchronization plans that never mutate remote services.
pub mod sync_plan;
/// One-stop canonical track lookup and explainability reports.
pub mod tracks;

mod error;

pub use error::{ChordriftError, Result};
