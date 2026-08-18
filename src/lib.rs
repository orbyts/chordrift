#![deny(missing_docs)]
#![forbid(unsafe_code)]

//! Personal music-library intelligence and synchronization infrastructure.
//!
//! Chordrift owns its music domain and PostgreSQL schema. Storexa supplies the
//! lower-level Neon/PostgreSQL connection, pooling, and migration primitives.

/// Command-line parsing and execution.
pub mod cli;
/// Application-owned database configuration.
pub mod config;
/// System credential storage with provider and account isolation.
pub mod credentials;
/// Storexa-backed database lifecycle and migration inspection.
pub mod db;
/// Streaming-provider adapters.
pub mod providers;

mod error;

pub use error::{ChordriftError, Result};
