//! Apple Music developer/user authorization and read-only catalog matching.

mod auth;
mod client;

pub use auth::{AuthReport, AuthStatus, SetupReport, authenticate, configure, logout, status};
pub use client::{AppleMusicClient, CatalogSong};
