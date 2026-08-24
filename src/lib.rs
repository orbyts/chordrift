#![deny(missing_docs)]
#![forbid(unsafe_code)]

//! Personal music-library intelligence and synchronization infrastructure.
//!
//! Chordrift owns its music domain and PostgreSQL schema. Storexa supplies the
//! lower-level Neon/PostgreSQL connection, pooling, and migration primitives.

/// Account-scoped canonical library analysis.
pub mod analysis;
/// Command-line parsing and execution.
pub mod cli;
/// Application-owned database configuration.
pub mod config;
/// System credential storage with provider and account isolation.
pub mod credentials;
/// Storexa-backed database lifecycle and migration inspection.
pub mod db;
/// Deterministic, versioned personal track embeddings.
pub mod embeddings;
/// Privacy-conscious Spotify archive inspection and listening-history import.
pub mod history;
/// Account-scoped playlist roles and drift policy.
pub mod playlists;
/// Streaming-provider adapters.
pub mod providers;

mod error;

pub use error::{ChordriftError, Result};
