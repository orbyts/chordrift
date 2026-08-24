//! Spotify OAuth, read-only Web API inventory, and snapshot persistence.

mod auth;
mod client;
mod import;
mod models;

pub use auth::{AuthReport, AuthStatus, SpotifyOAuthConfig, authenticate, logout, status};
pub(crate) use client::{RetryPolicy, retry_policy};
pub use import::{ImportReport, import};
