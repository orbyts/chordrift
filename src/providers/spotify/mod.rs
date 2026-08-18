//! Spotify OAuth, read-only Web API inventory, and snapshot persistence.

mod auth;
mod client;
mod import;
mod models;

pub use auth::{AuthReport, AuthStatus, SpotifyOAuthConfig, authenticate, logout, status};
pub use import::{ImportReport, import};
