use std::{
    io::{self, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    ChordriftError, Result, analysis, config, db, embeddings, history, playlists,
    providers::spotify,
};

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
    /// Pull provider changes into Neon and refresh derived state.
    Sync {
        /// Synchronization operation to perform.
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// Calculate or inspect canonical library statistics.
    Analyze {
        /// Analysis operation to perform.
        #[command(subcommand)]
        command: AnalyzeCommand,
    },
    /// Inspect or configure account-scoped playlist roles.
    Playlists {
        /// Playlist operation to perform.
        #[command(subcommand)]
        command: PlaylistCommand,
    },
    /// Inspect or import Spotify account and listening-history archives.
    History {
        /// Archive operation to perform.
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Generate and inspect deterministic personal track embeddings.
    Embeddings {
        /// Embedding operation to perform.
        #[command(subcommand)]
        command: EmbeddingCommand,
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

/// Provider-to-Neon synchronization commands.
#[derive(Clone, Debug, Subcommand)]
pub enum SyncCommand {
    /// Import current Spotify state and refresh canonical analysis.
    Pull {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Canonical analysis commands.
#[derive(Clone, Debug, Subcommand)]
pub enum AnalyzeCommand {
    /// Recalculate account statistics from the latest immutable snapshot.
    Refresh {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Show aggregate library and playlist statistics.
    Summary {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List tracks appearing in multiple current playlists.
    Overlap {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum rows to report.
        #[arg(long, default_value_t = 25)]
        limit: u32,
    },
    /// List duplicate canonical tracks within individual playlists.
    Duplicates {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum rows to report.
        #[arg(long, default_value_t = 25)]
        limit: u32,
    },
}

/// Account-scoped playlist configuration commands.
#[derive(Clone, Debug, Subcommand)]
pub enum PlaylistCommand {
    /// List imported playlists with role, policy, and presence.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List ordered tracks from the latest imported playlist snapshot.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive current playlist name; must be unambiguous.
        #[arg(
            long,
            required_unless_present = "spotify_id",
            conflicts_with = "spotify_id"
        )]
        name: Option<String>,
        /// Stable Spotify playlist ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
    },
    /// Configure one playlist by exact name or stable Spotify ID.
    Configure {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive current playlist name; must be unambiguous.
        #[arg(
            long,
            required_unless_present = "spotify_id",
            conflicts_with = "spotify_id"
        )]
        name: Option<String>,
        /// Stable Spotify playlist ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
        /// Orchestration role.
        #[arg(long, value_enum)]
        role: PlaylistRoleArg,
        /// Drift authority; defaults to neon-wins for managed, provider-wins otherwise.
        #[arg(long, value_enum)]
        drift_policy: Option<DriftPolicyArg>,
    },
    /// Set semantic contribution to embeddings; zero excludes the playlist.
    Weight {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive current playlist name; must be unambiguous.
        #[arg(
            long,
            required_unless_present = "spotify_id",
            conflicts_with = "spotify_id"
        )]
        name: Option<String>,
        /// Stable Spotify playlist ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
        /// Relative semantic contribution from 0 (excluded) through 10.
        #[arg(long)]
        weight: f64,
    },
}

/// Personal embedding commands.
#[derive(Clone, Debug, Subcommand)]
pub enum EmbeddingCommand {
    /// Report source coverage and per-playlist semantic weights.
    Audit {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Generate or reuse an immutable deterministic embedding generation.
    Generate {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Vector dimensions; defaults to 128.
        #[arg(long)]
        dimensions: Option<usize>,
        /// Reproducibility seed; defaults to 42.
        #[arg(long)]
        seed: Option<i64>,
    },
    /// Show the latest persisted embedding generation.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List nearest tracks from the latest generation.
    Neighbors {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive exact track title; must be unambiguous.
        #[arg(
            long,
            required_unless_present = "spotify_id",
            conflicts_with = "spotify_id"
        )]
        name: Option<String>,
        /// Stable Spotify track ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
        /// Maximum neighbors to report.
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
}

/// Spotify archive and listening-history commands.
#[derive(Clone, Debug, Subcommand)]
pub enum HistoryCommand {
    /// Verify an archive and report its safe structural contents without database writes.
    Inspect {
        /// Spotify ZIP archive to inspect.
        #[arg(long)]
        archive: PathBuf,
    },
    /// Import and locally archive every ZIP from the account inbox.
    Ingest {
        /// Local label for this Spotify account and its data folder.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Local ignored data root containing spotify/<account>/inbox.
        #[arg(long, default_value = "data")]
        data_root: PathBuf,
    },
    /// Replay every retained local archive into Neon for disaster recovery.
    Restore {
        /// Local label for this Spotify account and its data folder.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Local ignored data root containing spotify/<account>/archive.
        #[arg(long, default_value = "data")]
        data_root: PathBuf,
    },
    /// Idempotently import useful archive state into Neon.
    Import {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Spotify ZIP archive to import.
        #[arg(long)]
        archive: PathBuf,
    },
    /// Relink newly known Spotify IDs and rebuild per-track listening statistics.
    Refresh {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Summarize all imported listening history for an account.
    Summary {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List the most-listened tracks by total playback duration.
    Top {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum rows to report.
        #[arg(long, default_value_t = 25)]
        limit: u32,
    },
}

/// CLI representation of playlist orchestration roles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PlaylistRoleArg {
    /// Provider-owned playlist mirrored into Neon.
    Observed,
    /// Provider-native discovery inbox.
    Inbox,
    /// Neon-owned canonical output playlist.
    Managed,
}

/// CLI representation of drift authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum DriftPolicyArg {
    /// Import provider edits.
    ProviderWins,
    /// Restore approved Neon state during a future apply operation.
    NeonWins,
    /// Require an explicit resolution.
    Manual,
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
        Command::Sync { command } => match command {
            SyncCommand::Pull { account } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let import = spotify::import(&account, &database).await?;
                    write_import_report(output, &import)?;
                    let summary = analysis::refresh(&database, &account).await?;
                    write_analysis_summary(output, &summary)?;
                    let history = history::refresh(&database, &account).await?;
                    if history.archives != 0 {
                        write_history_summary(output, &history)?;
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
        },
        Command::Analyze { command } => {
            let database = connect_current_database().await?;
            let result = run_analyze_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Playlists { command } => {
            let database = connect_current_database().await?;
            let result = run_playlist_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::History { command } => match command {
            HistoryCommand::Inspect { archive } => {
                let inspection = history::inspect(&archive)?;
                write_archive_inspection(output, &inspection)?;
            }
            HistoryCommand::Import { account, archive } => {
                let database = connect_current_database().await?;
                let result = async {
                    let report = history::import(&database, &account, &archive).await?;
                    write_history_import_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            HistoryCommand::Ingest { account, data_root } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let reports = history::ingest(&database, &account, &data_root).await?;
                    for (index, report) in reports.iter().enumerate() {
                        if index != 0 {
                            writeln!(output)?;
                        }
                        write_history_import_report(output, &report.import)?;
                        writeln!(output, "archived_to: {}", report.archived_to.display())?;
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            HistoryCommand::Restore { account, data_root } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let reports = history::restore(&database, &account, &data_root).await?;
                    for (index, report) in reports.iter().enumerate() {
                        if index != 0 {
                            writeln!(output)?;
                        }
                        write_history_import_report(output, report)?;
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            HistoryCommand::Refresh { account } => {
                let database = connect_current_database().await?;
                let result = async {
                    let summary = history::refresh(&database, &account).await?;
                    write_history_summary(output, &summary)
                }
                .await;
                database.close().await;
                result?;
            }
            HistoryCommand::Summary { account } => {
                let database = connect_current_database().await?;
                let result = async {
                    let summary = history::summary(&database, &account).await?;
                    write_history_summary(output, &summary)
                }
                .await;
                database.close().await;
                result?;
            }
            HistoryCommand::Top { account, limit } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let rows = history::top(&database, &account, limit).await?;
                    writeln!(
                        output,
                        "hours\tplays\tevents\tskips\tcompleted\tmatched\tlast_played\ttrack"
                    )?;
                    for row in rows {
                        writeln!(
                            output,
                            "{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{} — {}",
                            hours(row.total_ms_played),
                            row.play_count,
                            row.event_count,
                            row.skip_count,
                            row.completed_count,
                            row.matched,
                            row.last_played_at.to_rfc3339(),
                            clean_cell(&row.track_name),
                            clean_cell(&row.artist_name)
                        )?;
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
        },
        Command::Embeddings { command } => {
            let database = connect_current_database().await?;
            let result = run_embedding_command(command, output, &database).await;
            database.close().await;
            result?;
        }
    }

    Ok(())
}

fn write_archive_inspection(
    output: &mut impl Write,
    inspection: &history::ArchiveInspection,
) -> Result<()> {
    writeln!(output, "spotify archive: verified")?;
    writeln!(output, "kind: {}", inspection.kind.as_str())?;
    writeln!(output, "filename: {}", inspection.source_filename)?;
    writeln!(output, "sha256: {}", inspection.sha256)?;
    writeln!(output, "source_files: {}", inspection.source_files)?;
    match inspection.kind {
        history::ArchiveKind::ExtendedStreamingHistory => {
            writeln!(output, "audio_events: {}", inspection.audio_events)?;
            writeln!(output, "track_events: {}", inspection.track_events)?;
            writeln!(output, "unique_tracks: {}", inspection.unique_tracks)?;
            writeln!(output, "episode_events: {}", inspection.episode_events)?;
            writeln!(output, "audiobook_events: {}", inspection.audiobook_events)?;
            writeln!(output, "video_events: {}", inspection.video_events)?;
            writeln!(output, "skipped_tracks: {}", inspection.skipped_tracks)?;
            writeln!(
                output,
                "listening_hours: {:.2}",
                hours(inspection.total_ms_played)
            )?;
            writeln!(
                output,
                "first_event_at: {}",
                inspection
                    .first_event_at
                    .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
            )?;
            writeln!(
                output,
                "last_event_at: {}",
                inspection
                    .last_event_at
                    .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
            )?;
        }
        history::ArchiveKind::AccountData => {
            writeln!(output, "playlists: {}", inspection.account_playlists)?;
            writeln!(
                output,
                "playlist_entries: {}",
                inspection.account_playlist_entries
            )?;
            writeln!(
                output,
                "library_tracks: {}",
                inspection.account_library_tracks
            )?;
            writeln!(
                output,
                "simplified_music_events_not_imported: {}",
                inspection.simplified_music_events
            )?;
        }
    }
    Ok(())
}

fn write_history_import_report(
    output: &mut impl Write,
    report: &history::ImportReport,
) -> Result<()> {
    write_archive_inspection(output, &report.inspection)?;
    writeln!(
        output,
        "archive_import: {}",
        if report.reused_archive {
            "already current"
        } else {
            "succeeded"
        }
    )?;
    writeln!(output, "events_inserted: {}", report.events_inserted)?;
    writeln!(
        output,
        "events_already_present: {}",
        report.events_already_present
    )?;
    writeln!(output, "events_matched: {}", report.events_matched)?;
    writeln!(output, "events_unmatched: {}", report.events_unmatched)?;
    writeln!(
        output,
        "privacy: IP addresses and account-profile PII not stored"
    )?;
    Ok(())
}

fn write_history_summary(output: &mut impl Write, summary: &history::HistorySummary) -> Result<()> {
    writeln!(output, "history: current")?;
    writeln!(output, "archives: {}", summary.archives)?;
    writeln!(output, "events: {}", summary.events)?;
    writeln!(output, "unique_tracks: {}", summary.unique_tracks)?;
    writeln!(
        output,
        "matched_unique_tracks: {}",
        summary.matched_unique_tracks
    )?;
    writeln!(
        output,
        "unmatched_unique_tracks: {}",
        summary.unmatched_unique_tracks
    )?;
    writeln!(output, "matched_events: {}", summary.matched_events)?;
    writeln!(output, "unmatched_events: {}", summary.unmatched_events)?;
    writeln!(output, "skipped_events: {}", summary.skipped_events)?;
    writeln!(
        output,
        "listening_hours: {:.2}",
        hours(summary.total_ms_played)
    )?;
    writeln!(
        output,
        "first_event_at: {}",
        summary
            .first_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "last_event_at: {}",
        summary
            .last_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    Ok(())
}

fn hours(milliseconds: i64) -> f64 {
    milliseconds as f64 / 3_600_000.0
}

async fn connect_current_database() -> Result<storexa::Database> {
    let config = config::database_config_from_env()?;
    let database = db::connect(config).await?;
    let status = db::status(&database).await?;
    if status.pending_migrations != 0 || status.failed_migrations != 0 {
        database.close().await;
        return Err(ChordriftError::Configuration(
            "database migrations are not current; run `chordrift db migrate`".to_owned(),
        ));
    }
    Ok(database)
}

async fn run_analyze_command(
    command: AnalyzeCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        AnalyzeCommand::Refresh { account } => {
            let summary = analysis::refresh(database, &account).await?;
            write_analysis_summary(output, &summary)
        }
        AnalyzeCommand::Summary { account } => {
            let summary = analysis::summary(database, &account).await?;
            write_analysis_summary(output, &summary)
        }
        AnalyzeCommand::Overlap { account, limit } => {
            let rows = analysis::overlap(database, &account, limit).await?;
            writeln!(output, "playlists\tentries\tsaved\ttrack")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{} — {}",
                    row.playlist_count,
                    row.total_entries,
                    row.saved,
                    clean_cell(&row.title),
                    clean_cell(&row.artists)
                )?;
            }
            Ok(())
        }
        AnalyzeCommand::Duplicates { account, limit } => {
            let rows = analysis::duplicates(database, &account, limit).await?;
            writeln!(output, "entries\tplaylist\ttrack")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}",
                    row.entries,
                    clean_cell(&row.playlist_name),
                    clean_cell(&row.track_title)
                )?;
            }
            Ok(())
        }
    }
}

async fn run_playlist_command(
    command: PlaylistCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        PlaylistCommand::List { account } => {
            let rows = playlists::list(database, &account).await?;
            writeln!(
                output,
                "role\tpolicy\tembedding_weight\tpresent\titems\tname\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{:.2}\t{}\t{}\t{}\t{}",
                    row.role,
                    row.drift_policy,
                    row.embedding_weight,
                    row.present,
                    row.total_items
                        .map_or_else(|| "-".to_owned(), |value| value.to_string()),
                    clean_cell(&row.name),
                    row.provider_playlist_id
                )?;
            }
            Ok(())
        }
        PlaylistCommand::Tracks {
            account,
            name,
            spotify_id,
        } => {
            let selector = playlist_selector(name, spotify_id);
            let report = playlists::tracks(database, &account, &selector).await?;
            writeln!(output, "playlist: {}", report.playlist.name)?;
            writeln!(
                output,
                "spotify_id: {}",
                report.playlist.provider_playlist_id
            )?;
            writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
            writeln!(output, "tracks: {}", report.tracks.len())?;
            writeln!(output, "position\ttrack\tartists\talbum\tspotify_track_id")?;
            for row in report.tracks {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}",
                    row.position + 1,
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    clean_cell(row.album.as_deref().unwrap_or("-")),
                    row.provider_track_id
                )?;
            }
            Ok(())
        }
        PlaylistCommand::Configure {
            account,
            name,
            spotify_id,
            role,
            drift_policy,
        } => {
            let selector = playlist_selector(name, spotify_id);
            let role = playlist_role(role);
            let drift_policy =
                drift_policy
                    .map(playlist_drift_policy)
                    .unwrap_or_else(|| match role {
                        playlists::PlaylistRole::Managed => playlists::DriftPolicy::NeonWins,
                        playlists::PlaylistRole::Observed | playlists::PlaylistRole::Inbox => {
                            playlists::DriftPolicy::ProviderWins
                        }
                    });
            let updated =
                playlists::configure(database, &account, &selector, role, drift_policy).await?;
            writeln!(output, "playlist: {}", updated.name)?;
            writeln!(output, "spotify_id: {}", updated.provider_playlist_id)?;
            writeln!(output, "role: {}", updated.role)?;
            writeln!(output, "drift_policy: {}", updated.drift_policy)?;
            writeln!(output, "embedding_weight: {:.2}", updated.embedding_weight)?;
            Ok(())
        }
        PlaylistCommand::Weight {
            account,
            name,
            spotify_id,
            weight,
        } => {
            let selector = playlist_selector(name, spotify_id);
            let updated =
                playlists::set_embedding_weight(database, &account, &selector, weight).await?;
            writeln!(output, "playlist: {}", updated.name)?;
            writeln!(output, "spotify_id: {}", updated.provider_playlist_id)?;
            writeln!(output, "embedding_weight: {:.2}", updated.embedding_weight)?;
            Ok(())
        }
    }
}

async fn run_embedding_command(
    command: EmbeddingCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        EmbeddingCommand::Audit { account } => {
            let report = embeddings::audit(database, &account).await?;
            writeln!(output, "embeddings: ready")?;
            writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
            writeln!(output, "eligible_tracks: {}", report.eligible_tracks)?;
            writeln!(output, "playlist_tracks: {}", report.playlist_tracks)?;
            writeln!(
                output,
                "artist_related_tracks: {}",
                report.artist_related_tracks
            )?;
            writeln!(
                output,
                "album_related_tracks: {}",
                report.album_related_tracks
            )?;
            writeln!(output, "history_tracks: {}", report.history_tracks)?;
            writeln!(output)?;
            writeln!(output, "weight\ttracks\tplaylist\tspotify_id")?;
            for playlist in report.playlists {
                writeln!(
                    output,
                    "{:.2}\t{}\t{}\t{}",
                    playlist.embedding_weight,
                    playlist.unique_tracks,
                    clean_cell(&playlist.name),
                    playlist.provider_playlist_id
                )?;
            }
            Ok(())
        }
        EmbeddingCommand::Generate {
            account,
            dimensions,
            seed,
        } => {
            let report = embeddings::generate(database, &account, dimensions, seed).await?;
            writeln!(
                output,
                "embedding_generation: {}",
                if report.reused {
                    "already current"
                } else {
                    "created"
                }
            )?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
            writeln!(output, "model: {}", report.model)?;
            writeln!(output, "model_version: {}", report.model_version)?;
            writeln!(output, "dimensions: {}", report.dimensions)?;
            writeln!(output, "seed: {}", report.seed)?;
            writeln!(output, "eligible_tracks: {}", report.eligible_tracks)?;
            writeln!(output, "embedded_tracks: {}", report.embedded_tracks)?;
            writeln!(output, "unembedded_tracks: {}", report.unembedded_tracks)?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            Ok(())
        }
        EmbeddingCommand::Status { account } => {
            let status = embeddings::status(database, &account).await?;
            writeln!(output, "embeddings: current")?;
            writeln!(output, "generation_id: {}", status.generation_id)?;
            writeln!(output, "snapshot_id: {}", status.snapshot_id)?;
            writeln!(output, "model: {}", status.model)?;
            writeln!(output, "model_version: {}", status.model_version)?;
            writeln!(output, "dimensions: {}", status.dimensions)?;
            writeln!(output, "seed: {}", status.seed)?;
            writeln!(output, "tracks: {}", status.track_count)?;
            writeln!(output, "input_hash: {}", status.input_hash)?;
            writeln!(output, "created_at: {}", status.created_at.to_rfc3339())?;
            Ok(())
        }
        EmbeddingCommand::Neighbors {
            account,
            name,
            spotify_id,
            limit,
        } => {
            let selector = match (name, spotify_id) {
                (Some(name), None) => embeddings::TrackSelector::Name(name),
                (None, Some(id)) => embeddings::TrackSelector::ProviderId(id),
                _ => unreachable!("clap enforces exactly one track selector"),
            };
            let report = embeddings::neighbors(database, &account, &selector, limit).await?;
            writeln!(output, "track: {} — {}", report.title, report.artists)?;
            writeln!(output, "spotify_id: {}", report.provider_track_id)?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "similarity\ttrack\tartists\tspotify_track_id")?;
            for neighbor in report.neighbors {
                writeln!(
                    output,
                    "{:.6}\t{}\t{}\t{}",
                    neighbor.similarity,
                    clean_cell(&neighbor.title),
                    clean_cell(&neighbor.artists),
                    neighbor.provider_track_id
                )?;
            }
            Ok(())
        }
    }
}

fn playlist_selector(
    name: Option<String>,
    spotify_id: Option<String>,
) -> playlists::PlaylistSelector {
    match (name, spotify_id) {
        (Some(name), None) => playlists::PlaylistSelector::Name(name),
        (None, Some(id)) => playlists::PlaylistSelector::ProviderId(id),
        _ => unreachable!("clap enforces exactly one playlist selector"),
    }
}

fn playlist_role(value: PlaylistRoleArg) -> playlists::PlaylistRole {
    match value {
        PlaylistRoleArg::Observed => playlists::PlaylistRole::Observed,
        PlaylistRoleArg::Inbox => playlists::PlaylistRole::Inbox,
        PlaylistRoleArg::Managed => playlists::PlaylistRole::Managed,
    }
}

fn playlist_drift_policy(value: DriftPolicyArg) -> playlists::DriftPolicy {
    match value {
        DriftPolicyArg::ProviderWins => playlists::DriftPolicy::ProviderWins,
        DriftPolicyArg::NeonWins => playlists::DriftPolicy::NeonWins,
        DriftPolicyArg::Manual => playlists::DriftPolicy::Manual,
    }
}

fn write_analysis_summary(
    output: &mut impl Write,
    summary: &analysis::AnalysisSummary,
) -> Result<()> {
    writeln!(output, "analysis: current")?;
    writeln!(output, "snapshot_id: {}", summary.snapshot_id)?;
    writeln!(output, "playlists: {}", summary.playlists)?;
    writeln!(output, "playlist_entries: {}", summary.playlist_entries)?;
    writeln!(
        output,
        "unique_playlist_tracks: {}",
        summary.unique_playlist_tracks
    )?;
    writeln!(output, "saved_tracks: {}", summary.saved_tracks)?;
    writeln!(output, "overlapping_tracks: {}", summary.overlapping_tracks)?;
    writeln!(output, "duplicate_entries: {}", summary.duplicate_entries)?;
    Ok(())
}

fn clean_cell(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], " ")
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

    use super::{
        Cli, Command, DbCommand, EmbeddingCommand, HistoryCommand, PlaylistCommand,
        PlaylistRoleArg, SpotifyCommand, SyncCommand, write_status,
    };
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
    fn parses_simple_pull_sync() {
        let cli = Cli::try_parse_from(["chordrift", "sync", "pull"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::Pull { account }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_account_scoped_inbox_configuration() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "playlists",
            "configure",
            "--name",
            "Discovery",
            "--role",
            "inbox",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Playlists {
                command: PlaylistCommand::Configure {
                    account,
                    name: Some(name),
                    role: PlaylistRoleArg::Inbox,
                    ..
                }
            } if account == "personal" && name == "Discovery"
        ));
    }

    #[test]
    fn parses_playlist_tracks_by_name() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "playlists",
            "tracks",
            "--name",
            "Smooth Morning Coffee (Curated)",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Playlists {
                command: PlaylistCommand::Tracks {
                    account,
                    name: Some(name),
                    ..
                }
            } if account == "personal" && name == "Smooth Morning Coffee (Curated)"
        ));
    }

    #[test]
    fn parses_playlist_embedding_weight() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "playlists",
            "weight",
            "--name",
            "All my saved songs",
            "--weight",
            "0",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Playlists {
                command: PlaylistCommand::Weight {
                    account,
                    name: Some(name),
                    weight,
                    ..
                }
            } if account == "personal" && name == "All my saved songs" && weight == 0.0
        ));
    }

    #[test]
    fn parses_default_embedding_generation() {
        let cli =
            Cli::try_parse_from(["chordrift", "embeddings", "generate"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Embeddings {
                command: EmbeddingCommand::Generate {
                    account,
                    dimensions: None,
                    seed: None
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_embedding_neighbors_by_spotify_id() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "embeddings",
            "neighbors",
            "--spotify-id",
            "track123",
            "--limit",
            "5",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Embeddings {
                command: EmbeddingCommand::Neighbors {
                    spotify_id: Some(id),
                    limit: 5,
                    ..
                }
            } if id == "track123"
        ));
    }

    #[test]
    fn parses_extended_history_import() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "history",
            "import",
            "--archive",
            "data/history.zip",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::History {
                command: HistoryCommand::Import { account, archive }
            } if account == "personal" && archive == std::path::Path::new("data/history.zip")
        ));
    }

    #[test]
    fn parses_default_history_inbox_ingestion() {
        let cli = Cli::try_parse_from(["chordrift", "history", "ingest"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::History {
                command: HistoryCommand::Ingest { account, data_root }
            } if account == "personal" && data_root == std::path::Path::new("data")
        ));
    }

    #[test]
    fn parses_default_history_restore() {
        let cli = Cli::try_parse_from(["chordrift", "history", "restore"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::History {
                command: HistoryCommand::Restore { account, data_root }
            } if account == "personal" && data_root == std::path::Path::new("data")
        ));
    }

    #[test]
    fn parses_history_top() {
        let cli = Cli::try_parse_from(["chordrift", "history", "top", "--limit", "10"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::History {
                command: HistoryCommand::Top { account, limit }
            } if account == "personal" && limit == 10
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
