use std::io::{self, Write};

use clap::{Parser, Subcommand};

use crate::{ChordriftError, Result, config, db, providers::spotify};

/// Chordrift command-line interface.
#[derive(Clone, Debug, Parser)]
#[command(name = "chordrift", version, about)]
pub struct Cli {
    /// Operation to perform.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level Chordrift commands.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Inspect or migrate the canonical database.
    Db {
        /// Database operation to perform.
        #[command(subcommand)]
        command: DbCommand,
    },
    /// Authenticate with Spotify or import its read-only library inventory.
    Spotify {
        /// Spotify operation to perform.
        #[command(subcommand)]
        command: SpotifyCommand,
    },
}

/// Canonical database commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DbCommand {
    /// Check connectivity and report migration state without changing it.
    Status,
    /// Apply pending Chordrift schema migrations.
    Migrate,
}

/// Read-only Spotify account commands.
#[derive(Clone, Debug, Subcommand)]
pub enum SpotifyCommand {
    /// Authorize Chordrift using browser-based OAuth with PKCE.
    Auth {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Verify the stored authorization against Spotify.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Snapshot playlists and saved tracks without modifying Spotify.
    Import {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Remove the local refresh token without revoking Spotify access.
    Logout {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Runs a parsed CLI command and writes its report to standard output.
pub async fn run(cli: Cli) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    run_with_writer(cli, &mut output).await
}

async fn run_with_writer(cli: Cli, output: &mut impl Write) -> Result<()> {
    match cli.command {
        Command::Db { command } => {
            let config = config::database_config_from_env()?;
            let database = db::connect(config).await?;
            let result = run_db_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Spotify { command } => match command {
            SpotifyCommand::Auth { account } => {
                let report = spotify::authenticate(&account).await?;
                write_auth_report(output, &report)?;
            }
            SpotifyCommand::Status { account } => {
                let status = spotify::status(&account).await?;
                write_auth_status(output, &status)?;
            }
            SpotifyCommand::Import { account } => {
                let config = config::database_config_from_env()?;
                let database = db::connect(config).await?;
                let result = async {
                    let status = db::status(&database).await?;
                    if status.pending_migrations != 0 || status.failed_migrations != 0 {
                        return Err(ChordriftError::Configuration(
                            "database migrations are not current; run `chordrift db migrate`"
                                .to_owned(),
                        ));
                    }
                    let report = spotify::import(&account, &database).await?;
                    write_import_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SpotifyCommand::Logout { account } => {
                let removed = spotify::logout(&account)?;
                writeln!(
                    output,
                    "spotify credential: {}",
                    if removed { "removed" } else { "not found" }
                )?;
                writeln!(output, "account: {account}")?;
            }
        },
    }

    Ok(())
}

fn write_auth_report(output: &mut impl Write, report: &spotify::AuthReport) -> Result<()> {
    writeln!(output, "spotify authorization: stored")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "account_id: {}", report.account_id)?;
    writeln!(
        output,
        "display_name: {}",
        report.display_name.as_deref().unwrap_or("unavailable")
    )?;
    writeln!(output, "scopes: {}", report.scopes.join(" "))?;
    writeln!(output, "token_storage: system keychain")?;
    Ok(())
}

fn write_auth_status(output: &mut impl Write, status: &spotify::AuthStatus) -> Result<()> {
    writeln!(output, "spotify authorization: valid")?;
    writeln!(output, "account: {}", status.account_label)?;
    writeln!(output, "account_id: {}", status.account_id)?;
    writeln!(
        output,
        "display_name: {}",
        status.display_name.as_deref().unwrap_or("unavailable")
    )?;
    writeln!(output, "scopes: {}", status.scopes.join(" "))?;
    Ok(())
}

fn write_import_report(output: &mut impl Write, report: &spotify::ImportReport) -> Result<()> {
    writeln!(output, "spotify import: succeeded")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
    writeln!(
        output,
        "playlists: {}/{} imported",
        report.playlists_imported, report.playlists_seen
    )?;
    writeln!(output, "playlists_reused: {}", report.playlists_reused)?;
    writeln!(output, "playlist_entries: {}", report.playlist_entries)?;
    writeln!(output, "saved_tracks: {}", report.saved_tracks)?;
    writeln!(
        output,
        "saved_tracks_reused: {}",
        report.saved_tracks_reused
    )?;
    writeln!(output, "unavailable_items: {}", report.unavailable_items)?;
    writeln!(output, "unsupported_items: {}", report.unsupported_items)?;
    writeln!(
        output,
        "followed_playlists_skipped: {}",
        report.followed_playlists_skipped
    )?;
    writeln!(
        output,
        "inaccessible_collaborative_playlists: {}",
        report.inaccessible_collaborative_playlists
    )?;
    Ok(())
}

async fn run_db_command(
    command: DbCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    if matches!(command, DbCommand::Migrate) {
        let report = db::migrate(database).await?;
        writeln!(
            output,
            "migrations: {} available migration(s) current in {} ms",
            report.available,
            report.elapsed.as_millis()
        )?;
    }

    let status = db::status(database).await?;
    write_status(output, &status)
}

fn write_status(output: &mut impl Write, status: &db::DatabaseStatus) -> Result<()> {
    writeln!(output, "database: chordrift-primary")?;
    writeln!(output, "provider: neon")?;
    writeln!(output, "status: healthy")?;
    writeln!(output, "server: {}", status.server_version)?;
    writeln!(output, "latency_ms: {}", status.latency.as_millis())?;
    writeln!(
        output,
        "migrations: {}/{} applied, {} pending, {} failed",
        status.applied_migrations,
        status.available_migrations,
        status.pending_migrations,
        status.failed_migrations
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser;

    use super::{Cli, Command, DbCommand, SpotifyCommand, write_status};
    use crate::db::DatabaseStatus;

    #[test]
    fn parses_database_status() {
        let cli = Cli::try_parse_from(["chordrift", "db", "status"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Db {
                command: DbCommand::Status
            }
        ));
    }

    #[test]
    fn parses_spotify_import_with_default_account() {
        let cli = Cli::try_parse_from(["chordrift", "spotify", "import"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Spotify {
                command: SpotifyCommand::Import { account }
            } if account == "personal"
        ));
    }

    #[test]
    fn renders_secret_free_status() {
        let mut output = Vec::new();
        write_status(
            &mut output,
            &DatabaseStatus {
                server_version: "18.1".to_owned(),
                latency: Duration::from_millis(12),
                available_migrations: 1,
                applied_migrations: 0,
                pending_migrations: 1,
                failed_migrations: 0,
            },
        )
        .expect("writable buffer");

        let output = String::from_utf8(output).expect("UTF-8 output");
        assert!(output.contains("status: healthy"));
        assert!(output.contains("migrations: 0/1 applied, 1 pending, 0 failed"));
        assert!(!output.contains("postgresql://"));
    }
}
