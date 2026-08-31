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
/// Shared application invocation boundary used by every client.
pub mod application;
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
/// Authenticated remote and explicit in-process development client transports.
pub mod client_transport;
/// Reproducible account-scoped vibe cluster generations.
pub mod clusters;
/// Application-owned database configuration.
pub mod config;
/// Versioned, provider-neutral application contract shared by every client.
pub mod contract;
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
/// Provider-neutral product-domain identities, values, and invariants.
pub mod domain;
/// Restart-safe authenticated operation queue and lifecycle persistence.
pub mod durable_operations;
/// Deterministic, versioned semantic track embeddings.
pub mod embeddings;
/// Cache-first, provenance-aware semantic metadata enrichment.
pub mod enrichment;
/// Privacy-conscious Spotify archive inspection and listening-history import.
pub mod history;
/// Production hosted-service assembly, OIDC login, and thin browser client.
pub mod hosted;
/// Separate durable provider-operation worker used by the hosted authority.
pub mod hosted_worker;
/// Authenticated HTTP adapter for typed application commands and queries.
pub mod http_transport;
/// Product identities, account authorization, and revocable bearer sessions.
pub mod identity;
/// Read-only current-provider intake joined with durable intent and history.
pub mod intake;
/// Wrapper-neutral ordinary-maintenance workflow shared by every client.
pub mod maintenance;
/// Restart-safe PostgreSQL persistence for wrapper-neutral maintenance sessions.
pub mod maintenance_store;
/// Versioned pretrained audio-model inference artifacts.
pub mod model_inference;
/// Provider-read-only onboarding input capture and durable provenance.
pub mod onboarding;
/// Read-only current-inventory audit for a captured onboarding session.
pub mod onboarding_audit;
/// Account-scoped playlist roles and drift policy.
pub mod playlists;
mod presentation;
/// Provider-write-free application values used by the CLI-first product rehearsal.
pub mod product_rehearsal;
/// Non-destructive, account-scoped proposed playlist libraries.
pub mod proposals;
/// Encrypted server-side provider credential storage and lifecycle.
pub mod provider_vault;
/// Streaming-provider adapters.
pub mod providers;
/// Provider-neutral Discovery + Rediscovery recipe execution draft.
pub mod recipe_execution;
/// Durable zero-signal routing playlists for ongoing listening review.
pub mod routes;
/// Authenticated transport-neutral Rust application authority.
pub mod service;
/// Versioned account-specific preference and lifecycle signals.
pub mod signals;
/// Deterministic ordered Spin previews and migration-0046 persistence.
pub mod spin_preview;
/// Approved Spin publication planning and provider-neutral verification.
pub mod spin_publication;
/// Immutable provider synchronization plans that never mutate remote services.
pub mod sync_plan;
mod terminal;
/// One-stop canonical track lookup and explainability reports.
pub mod tracks;

mod error;

pub use error::{ChordriftError, Result};
