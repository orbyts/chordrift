use std::{
    io::{self, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    ChordriftError, Result, analysis, bookmarks, clusters, config, db, embeddings, enrichment,
    history, model_inference, playlists, proposals, providers::spotify, signals, sync_plan,
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
    /// Inspect externally owned playlists retained as Neon bookmarks.
    Bookmarks {
        /// Bookmark operation to perform.
        #[command(subcommand)]
        command: BookmarkCommand,
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
    /// Generate and inspect reproducible vibe clusters.
    Clusters {
        /// Cluster operation to perform.
        #[command(subcommand)]
        command: ClusterCommand,
    },
    /// Build, name, inspect, and explicitly approve a proposed playlist library.
    Proposals {
        /// Proposal operation to perform.
        #[command(subcommand)]
        command: ProposalCommand,
    },
    /// Generate and inspect account-specific preference and lifecycle signals.
    Signals {
        /// Signal operation to perform.
        #[command(subcommand)]
        command: SignalCommand,
    },
    /// Resolve and cache independent semantic metadata for canonical tracks.
    Enrich {
        /// Enrichment operation to perform.
        #[command(subcommand)]
        command: EnrichmentCommand,
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
    /// Build or reuse an immutable plan without contacting Spotify.
    Plan {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact approved proposal; defaults to the latest approved proposal.
        #[arg(long)]
        proposal: Option<uuid::Uuid>,
    },
    /// Show the latest or selected immutable dry-run plan.
    PlanShow {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact plan ID; defaults to the newest plan.
        #[arg(long)]
        plan: Option<uuid::Uuid>,
        /// Print every exact operation after the summary.
        #[arg(long)]
        details: bool,
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
    /// Configure semantic, behavioral, intake, or lifecycle evidence.
    Signals {
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
        /// Evidence class, independent of sync authority.
        #[arg(long, value_enum)]
        class: PlaylistSignalClassArg,
        /// Optional behavioral evidence supplied by this playlist.
        #[arg(long, value_enum)]
        behavior: Option<BehavioralSignalArg>,
        /// Relative semantic contribution from 0 (excluded) through 10.
        #[arg(long)]
        semantic_weight: Option<f64>,
        /// When a temporary intake may be cleared.
        #[arg(long, value_enum)]
        clear_policy: Option<ClearPolicyArg>,
    },
}

/// External playlist bookmark commands.
#[derive(Clone, Debug, Subcommand)]
pub enum BookmarkCommand {
    /// List present and archived external playlist bookmarks.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List the newest readable contents retained for one bookmark.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive bookmark name; must be unambiguous.
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
    /// Snapshot every present bookmark into an immutable cleanup review batch.
    CleanupPlan {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Show the latest or selected cleanup batch and every exact candidate.
    CleanupShow {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact batch ID; defaults to the latest batch.
        #[arg(long)]
        batch: Option<uuid::Uuid>,
    },
    /// Approve one exact cleanup batch without contacting Spotify.
    CleanupApprove {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact cleanup batch ID printed by cleanup-plan or cleanup-show.
        #[arg(long)]
        confirm: uuid::Uuid,
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
        /// Vector dimensions; defaults to 1024.
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

/// Reproducible vibe clustering commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ClusterCommand {
    /// Generate or reuse clusters from the latest embedding generation.
    Generate {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Requested cluster count.
        #[arg(long, default_value_t = 12)]
        count: u32,
        /// Leave tracks below this centroid cosine similarity unassigned.
        #[arg(long, default_value_t = 0.05, allow_hyphen_values = true)]
        min_similarity: f64,
        /// Do not persist playlist-like clusters smaller than this.
        #[arg(long, default_value_t = 10)]
        min_cluster_size: u32,
        /// Reproducibility seed; defaults to 42.
        #[arg(long)]
        seed: Option<i64>,
    },
    /// Show the latest cluster generation.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List cluster sizes and representative tracks.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List tracks assigned to one machine cluster label.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Machine label reported by `clusters list`.
        #[arg(long)]
        cluster: String,
        /// Maximum tracks to report.
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
}

/// Non-destructive proposed playlist library commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProposalCommand {
    /// Generate or reuse a proposal from the latest cluster generation.
    Generate {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Show the latest proposal, naming, and coverage state.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List stable proposed playlists.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List tracks in one stable proposed playlist.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable key reported by `proposals list`.
        #[arg(long)]
        playlist: String,
        /// Maximum tracks to report.
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    /// Show per-source retirement coverage.
    Coverage {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List retirement-source tracks missing from the latest proposal.
    Missing {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum tracks to report.
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    /// Create a stable manual destination in the latest proposal.
    CategoryCreate {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// User-facing category name; it may be revised later.
        #[arg(long)]
        name: String,
        /// Short description of the intended sound.
        #[arg(long)]
        description: String,
        /// Semantic tag; repeat this option 2-6 times.
        #[arg(long, required = true)]
        tag: Vec<String>,
    },
    /// Assign or move one track to a stable proposed playlist.
    Assign {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact Spotify track ID; repeat to assign several tracks in one session.
        #[arg(long, required = true)]
        spotify_id: Vec<String>,
        /// Stable destination key reported by `proposals list`.
        #[arg(long)]
        playlist: String,
        /// Auditable explanation for the manual decision.
        #[arg(long)]
        reason: String,
    },
    /// Remove one assignment and return the track to internal review.
    Review {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact Spotify track ID.
        #[arg(long)]
        spotify_id: String,
        /// Auditable explanation for requesting review.
        #[arg(long)]
        reason: String,
    },
    /// Export a strict, privacy-minimized JSON naming context.
    NamingExport {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Destination JSON file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Import versioned names, descriptions, tags, and generator provenance.
    NamingImport {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Strict naming-result JSON artifact.
        #[arg(long)]
        file: PathBuf,
    },
    /// Approve a fully named proposal only when retirement coverage is complete.
    Approve {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact generation ID printed by `proposals status`.
        #[arg(long)]
        confirm: uuid::Uuid,
    },
}

/// Account-specific preference and lifecycle signal commands.
#[derive(Clone, Debug, Subcommand)]
pub enum SignalCommand {
    /// Generate or reuse immutable signals from current provider state and history.
    Generate {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Show the latest persisted signal generation.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Independent semantic enrichment commands.
#[derive(Clone, Debug, Subcommand)]
pub enum EnrichmentCommand {
    /// Resolve a bounded ISRC-first batch against MusicBrainz.
    Musicbrainz {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum pending tracks to process in this invocation.
        #[arg(long, default_value_t = 25)]
        limit: u32,
        /// Reprocess existing matches from cached responses; does not force redownloads.
        #[arg(long)]
        refresh: bool,
    },
    /// Resolve matched MusicBrainz artists to primary associated areas.
    Artists {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum distinct pending artists to process in this invocation.
        #[arg(long, default_value_t = 25)]
        limit: u32,
    },
    /// Import independently produced pretrained audio-model artifacts.
    ModelImport {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Strict JSON artifact manifest containing no audio or local paths.
        #[arg(long)]
        file: PathBuf,
    },
    /// Show imported pretrained audio-model coverage.
    ModelStatus {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Show current enrichment and cache coverage without network requests.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
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

/// CLI representation of playlist evidence classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PlaylistSignalClassArg {
    /// User-curated legacy vibe evidence.
    SemanticLegacy,
    /// Spotify-owned behavioral evidence.
    ProviderCurated,
    /// User-owned temporary intake.
    Intake,
    /// Chordrift-managed canonical output.
    Canonical,
    /// Temporary transfer infrastructure.
    Transport,
    /// Excluded from analysis.
    Ignored,
}

/// CLI representation of optional behavioral evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum BehavioralSignalArg {
    /// Current high rotation.
    Rotation,
    /// Provider discovery.
    Discovery,
    /// Explicit prompted interest.
    Prompted,
    /// Social or friend recommendation.
    Recommendation,
}

/// CLI representation of intake clearing safeguards.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ClearPolicyArg {
    /// Never clear automatically.
    Never,
    /// Clear only after published canonical placement is verified.
    AfterVerifiedAssignment,
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
            SyncCommand::Plan { account, proposal } => {
                let database = connect_current_database().await?;
                let result = async {
                    let report = sync_plan::create(&database, &account, proposal).await?;
                    write_sync_plan_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::PlanShow {
                account,
                plan,
                details,
            } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let (report, snapshot_current, operations) =
                        sync_plan::show(&database, &account, plan).await?;
                    write_sync_plan_report(output, &report)?;
                    writeln!(output, "snapshot_current: {snapshot_current}")?;
                    if details {
                        writeln!(
                            output,
                            "sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety"
                        )?;
                        for operation in operations {
                            writeln!(
                                output,
                                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                                operation.sequence,
                                operation.phase,
                                operation.operation_type,
                                clean_cell(&operation.playlist_name),
                                operation.spotify_playlist_id.as_deref().unwrap_or("-"),
                                operation.spotify_track_id.as_deref().unwrap_or("-"),
                                clean_cell(&operation.payload.to_string()),
                                clean_cell(&operation.safety.to_string())
                            )?;
                        }
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
        Command::Bookmarks { command } => {
            let database = connect_current_database().await?;
            let result = run_bookmark_command(command, output, &database).await;
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
        Command::Clusters { command } => {
            let database = connect_current_database().await?;
            let result = run_cluster_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Proposals { command } => {
            let database = connect_current_database().await?;
            let result = run_proposal_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Signals { command } => {
            let database = connect_current_database().await?;
            let result = run_signal_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Enrich { command } => {
            let database = connect_current_database().await?;
            let result = run_enrichment_command(command, output, &database).await;
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
                "role\tdrift\tsignal_class\tbehavior\tsemantic_weight\tclear_policy\tpresent\titems\tname\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{}\t{}\t{}",
                    row.role,
                    row.drift_policy,
                    row.signal_class,
                    row.behavioral_signal.as_deref().unwrap_or("-"),
                    row.semantic_weight,
                    row.clear_policy,
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
            writeln!(output, "signal_class: {}", updated.signal_class)?;
            writeln!(output, "semantic_weight: {:.2}", updated.semantic_weight)?;
            Ok(())
        }
        PlaylistCommand::Signals {
            account,
            name,
            spotify_id,
            class,
            behavior,
            semantic_weight,
            clear_policy: clear,
        } => {
            let selector = playlist_selector(name, spotify_id);
            let updated = playlists::configure_signals(
                database,
                &account,
                &selector,
                playlist_signal_class(class),
                behavior.map(behavioral_signal),
                semantic_weight,
                clear.map(clear_policy),
            )
            .await?;
            writeln!(output, "playlist: {}", updated.name)?;
            writeln!(output, "spotify_id: {}", updated.provider_playlist_id)?;
            writeln!(output, "signal_class: {}", updated.signal_class)?;
            writeln!(
                output,
                "behavioral_signal: {}",
                updated.behavioral_signal.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "semantic_weight: {:.2}", updated.semantic_weight)?;
            writeln!(output, "clear_policy: {}", updated.clear_policy)?;
            Ok(())
        }
    }
}

async fn run_bookmark_command(
    command: BookmarkCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        BookmarkCommand::List { account } => {
            let rows = bookmarks::list(database, &account).await?;
            writeln!(
                output,
                "present\trelationship\tcontent\titems\towner\tname\tlast_changed\tspotify_id\turl"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.present,
                    row.relationship,
                    row.content_status,
                    row.item_count,
                    clean_cell(
                        row.owner_display_name
                            .as_deref()
                            .unwrap_or(&row.owner_provider_id)
                    ),
                    clean_cell(&row.name),
                    row.last_changed_at.to_rfc3339(),
                    row.provider_playlist_id,
                    row.provider_url.as_deref().unwrap_or("-")
                )?;
            }
            Ok(())
        }
        BookmarkCommand::Tracks {
            account,
            name,
            spotify_id,
        } => {
            let selector = bookmark_selector(name, spotify_id);
            let report = bookmarks::tracks(database, &account, &selector).await?;
            writeln!(output, "bookmark: {}", clean_cell(&report.bookmark.name))?;
            writeln!(
                output,
                "owner: {}",
                clean_cell(
                    report
                        .bookmark
                        .owner_display_name
                        .as_deref()
                        .unwrap_or(&report.bookmark.owner_provider_id)
                )
            )?;
            writeln!(
                output,
                "spotify_id: {}",
                report.bookmark.provider_playlist_id
            )?;
            writeln!(output, "present: {}", report.bookmark.present)?;
            writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
            writeln!(output, "captured_at: {}", report.captured_at.to_rfc3339())?;
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
        BookmarkCommand::CleanupPlan { account } => {
            let batch = bookmarks::cleanup_plan(database, &account).await?;
            writeln!(
                output,
                "cleanup_plan: {}",
                if batch.reused {
                    "already current"
                } else {
                    "created"
                }
            )?;
            write_cleanup_batch(output, &batch)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        BookmarkCommand::CleanupShow { account, batch } => {
            let (batch, items) = bookmarks::cleanup_show(database, &account, batch).await?;
            write_cleanup_batch(output, &batch)?;
            writeln!(
                output,
                "content\titems\towner_id\tname\tspotify_id\texpected_snapshot"
            )?;
            for item in items {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    item.content_status,
                    item.item_count,
                    clean_cell(&item.owner_provider_id),
                    clean_cell(&item.name),
                    item.provider_playlist_id,
                    item.provider_snapshot_id.as_deref().unwrap_or("-")
                )?;
            }
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        BookmarkCommand::CleanupApprove { account, confirm } => {
            let batch = bookmarks::cleanup_approve(database, &account, confirm).await?;
            write_cleanup_batch(output, &batch)?;
            writeln!(output, "approval: recorded")?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
    }
}

fn write_cleanup_batch(output: &mut impl Write, batch: &bookmarks::CleanupBatch) -> Result<()> {
    writeln!(output, "batch_id: {}", batch.batch_id)?;
    writeln!(output, "source_snapshot_id: {}", batch.source_snapshot_id)?;
    writeln!(output, "state: {}", batch.state)?;
    writeln!(output, "candidates: {}", batch.candidate_count)?;
    writeln!(output, "input_hash: {}", batch.input_hash)?;
    writeln!(output, "created_at: {}", batch.created_at.to_rfc3339())?;
    writeln!(
        output,
        "approved_at: {}",
        batch
            .approved_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    Ok(())
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
            writeln!(
                output,
                "semantic_fact_tracks: {}",
                report.semantic_fact_tracks
            )?;
            writeln!(
                output,
                "acoustic_embedding_tracks: {}",
                report.acoustic_embedding_tracks
            )?;
            writeln!(output)?;
            writeln!(output, "semantic_weight\ttracks\tplaylist\tspotify_id")?;
            for playlist in report.playlists {
                writeln!(
                    output,
                    "{:.2}\t{}\t{}\t{}",
                    playlist.semantic_weight,
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

async fn run_cluster_command(
    command: ClusterCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        ClusterCommand::Generate {
            account,
            count,
            min_similarity,
            min_cluster_size,
            seed,
        } => {
            let report = clusters::generate(
                database,
                &account,
                count,
                min_similarity,
                min_cluster_size,
                seed,
            )
            .await?;
            writeln!(output, "clusters: generated")?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(
                output,
                "embedding_generation_id: {}",
                report.embedding_generation_id
            )?;
            writeln!(output, "reused: {}", report.reused)?;
            writeln!(output, "tracks: {}", report.track_count)?;
            writeln!(output, "clusters: {}", report.cluster_count)?;
            writeln!(output, "unassigned: {}", report.unassigned_count)?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            Ok(())
        }
        ClusterCommand::Status { account } => {
            let report = clusters::status(database, &account).await?;
            writeln!(output, "clusters: current")?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(
                output,
                "embedding_generation_id: {}",
                report.embedding_generation_id
            )?;
            writeln!(
                output,
                "algorithm: {}@{}",
                report.algorithm, report.algorithm_version
            )?;
            writeln!(output, "tracks: {}", report.track_count)?;
            writeln!(output, "clusters: {}", report.cluster_count)?;
            writeln!(output, "unassigned: {}", report.unassigned_count)?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            writeln!(output, "created_at: {}", report.created_at.to_rfc3339())?;
            Ok(())
        }
        ClusterCommand::List { account } => {
            let rows = clusters::list(database, &account).await?;
            writeln!(
                output,
                "tracks\trepresentative_score\tcluster\trepresentative\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{:.4}\t{}\t{} — {}\t{}",
                    row.track_count,
                    row.representative_score,
                    clean_cell(&row.machine_label),
                    clean_cell(&row.representative_title),
                    clean_cell(&row.representative_artists),
                    row.representative_spotify_id
                )?;
            }
            Ok(())
        }
        ClusterCommand::Tracks {
            account,
            cluster,
            limit,
        } => {
            let rows = clusters::tracks(database, &account, &cluster, limit).await?;
            writeln!(output, "rank\tscore\ttrack\tartists\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{:.4}\t{}\t{}\t{}",
                    row.representative_rank,
                    row.membership_score,
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
    }
}

async fn run_proposal_command(
    command: ProposalCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        ProposalCommand::Generate { account } => {
            let report = proposals::generate(database, &account).await?;
            writeln!(
                output,
                "proposal: {}",
                if report.reused {
                    "already current"
                } else {
                    "created"
                }
            )?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(
                output,
                "cluster_generation_id: {}",
                report.cluster_generation_id
            )?;
            writeln!(output, "playlists: {}", report.playlist_count)?;
            writeln!(output, "assigned_tracks: {}", report.assigned_track_count)?;
            writeln!(output, "required_tracks: {}", report.required_track_count)?;
            writeln!(
                output,
                "represented_tracks: {}",
                report.represented_track_count
            )?;
            writeln!(output, "coverage_complete: {}", report.coverage_complete)?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::Status { account } => {
            let status = proposals::status(database, &account).await?;
            writeln!(output, "proposal: {}", status.state)?;
            writeln!(output, "generation_id: {}", status.generation_id)?;
            writeln!(
                output,
                "cluster_generation_id: {}",
                status.cluster_generation_id
            )?;
            writeln!(output, "playlists: {}", status.playlist_count)?;
            writeln!(output, "named_playlists: {}", status.named_playlist_count)?;
            writeln!(output, "required_tracks: {}", status.required_track_count)?;
            writeln!(
                output,
                "represented_tracks: {}",
                status.represented_track_count
            )?;
            writeln!(output, "coverage_complete: {}", status.coverage_complete)?;
            writeln!(output, "input_hash: {}", status.input_hash)?;
            writeln!(
                output,
                "naming_context_hash: {}",
                status.naming_context_hash.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "created_at: {}", status.created_at.to_rfc3339())?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::List { account } => {
            let rows = proposals::list(database, &account).await?;
            writeln!(
                output,
                "tracks\tnamed\tstable_key\tname\ttags\tmachine_label"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    row.track_count,
                    row.named,
                    row.stable_key,
                    clean_cell(&row.name),
                    clean_cell(&row.tags.join(",")),
                    row.machine_label
                )?;
            }
            Ok(())
        }
        ProposalCommand::Tracks {
            account,
            playlist,
            limit,
        } => {
            let rows = proposals::tracks(database, &account, &playlist, limit).await?;
            writeln!(output, "position\ttrack\tartists\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    row.position,
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        ProposalCommand::Coverage { account } => {
            let rows = proposals::coverage(database, &account).await?;
            writeln!(
                output,
                "required\trepresented\tmissing\tclass\tplaylist\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    row.required_tracks,
                    row.represented_tracks,
                    row.missing_tracks,
                    row.signal_class,
                    clean_cell(&row.source_name),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        ProposalCommand::Missing { account, limit } => {
            let rows = proposals::missing(database, &account, limit).await?;
            writeln!(output, "track\tartists\tsource_playlists\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    clean_cell(&row.source_playlists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        ProposalCommand::CategoryCreate {
            account,
            name,
            description,
            tag,
        } => {
            let category =
                proposals::create_category(database, &account, &name, &description, &tag).await?;
            writeln!(output, "manual_category: created")?;
            writeln!(output, "generation_id: {}", category.generation_id)?;
            writeln!(output, "stable_key: {}", category.stable_key)?;
            writeln!(output, "name: {}", clean_cell(&category.name))?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::Assign {
            account,
            spotify_id,
            playlist,
            reason,
        } => {
            for spotify_id in spotify_id {
                let report =
                    proposals::assign(database, &account, &spotify_id, &playlist, &reason).await?;
                write_assignment_report(output, &report)?;
            }
            Ok(())
        }
        ProposalCommand::Review {
            account,
            spotify_id,
            reason,
        } => {
            let report = proposals::needs_review(database, &account, &spotify_id, &reason).await?;
            write_assignment_report(output, &report)
        }
        ProposalCommand::NamingExport { account, file } => {
            let context = proposals::naming_context(database, &account).await?;
            let bytes = serde_json::to_vec_pretty(&context)?;
            std::fs::write(&file, bytes)?;
            writeln!(output, "naming_context: exported")?;
            writeln!(output, "generation_id: {}", context.generation_id)?;
            writeln!(output, "context_sha256: {}", context.context_sha256)?;
            writeln!(output, "playlists: {}", context.playlists.len())?;
            writeln!(output, "file: {}", file.display())?;
            Ok(())
        }
        ProposalCommand::NamingImport { account, file } => {
            let bytes = std::fs::read(&file)?;
            let artifact: proposals::NamingArtifact = serde_json::from_slice(&bytes)?;
            let count = proposals::import_names(database, &account, artifact, &bytes).await?;
            writeln!(output, "naming_artifact: imported")?;
            writeln!(output, "playlists_named: {count}")?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::Approve { account, confirm } => {
            let status = proposals::approve(database, &account, confirm).await?;
            writeln!(output, "proposal: {}", status.state)?;
            writeln!(output, "generation_id: {}", status.generation_id)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
    }
}

fn write_assignment_report(
    output: &mut impl Write,
    report: &proposals::AssignmentReport,
) -> Result<()> {
    writeln!(output, "manual_assignment: recorded")?;
    writeln!(output, "track: {}", clean_cell(&report.title))?;
    writeln!(output, "spotify_id: {}", report.spotify_id)?;
    writeln!(
        output,
        "destination: {}",
        report.destination.as_deref().unwrap_or("needs-review")
    )?;
    writeln!(
        output,
        "represented_tracks: {}",
        report.represented_track_count
    )?;
    writeln!(output, "missing_tracks: {}", report.missing_track_count)?;
    writeln!(output, "spotify_writes: disabled")?;
    Ok(())
}

async fn run_signal_command(
    command: SignalCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        SignalCommand::Generate { account } => {
            let report = signals::generate(database, &account).await?;
            writeln!(
                output,
                "signal_generation: {}",
                if report.reused {
                    "already current"
                } else {
                    "created"
                }
            )?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
            writeln!(output, "tracks: {}", report.track_count)?;
            writeln!(output, "history_tracks: {}", report.history_tracks)?;
            writeln!(output, "saved_tracks: {}", report.saved_tracks)?;
            writeln!(output, "rotation_tracks: {}", report.rotation_tracks)?;
            writeln!(output, "discovery_tracks: {}", report.discovery_tracks)?;
            writeln!(output, "prompted_tracks: {}", report.prompted_tracks)?;
            writeln!(output, "intake_tracks: {}", report.intake_tracks)?;
            writeln!(
                output,
                "recommendation_tracks: {}",
                report.recommendation_tracks
            )?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            Ok(())
        }
        SignalCommand::Status { account } => {
            let status = signals::status(database, &account).await?;
            writeln!(output, "signals: current")?;
            writeln!(output, "generation_id: {}", status.generation_id)?;
            writeln!(output, "snapshot_id: {}", status.snapshot_id)?;
            writeln!(output, "model: {}", status.model)?;
            writeln!(output, "model_version: {}", status.model_version)?;
            writeln!(output, "tracks: {}", status.track_count)?;
            writeln!(output, "input_hash: {}", status.input_hash)?;
            writeln!(output, "created_at: {}", status.created_at.to_rfc3339())?;
            Ok(())
        }
    }
}

async fn run_enrichment_command(
    command: EnrichmentCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        EnrichmentCommand::Musicbrainz {
            account,
            limit,
            refresh,
        } => {
            let report = enrichment::musicbrainz(database, &account, limit, refresh).await?;
            writeln!(output, "enrichment: succeeded")?;
            writeln!(output, "run_id: {}", report.run_id)?;
            writeln!(output, "source: musicbrainz")?;
            writeln!(output, "tracks_considered: {}", report.tracks_considered)?;
            writeln!(output, "requests_made: {}", report.requests_made)?;
            writeln!(output, "cache_hits: {}", report.cache_hits)?;
            writeln!(output, "matched_tracks: {}", report.matched_tracks)?;
            writeln!(output, "ambiguous_tracks: {}", report.ambiguous_tracks)?;
            writeln!(output, "unmatched_tracks: {}", report.unmatched_tracks)?;
            writeln!(output, "error_tracks: {}", report.error_tracks)?;
            writeln!(output, "facts_written: {}", report.facts_written)?;
            Ok(())
        }
        EnrichmentCommand::Artists { account, limit } => {
            let report = enrichment::artist_areas(database, &account, limit).await?;
            writeln!(output, "artist_area_enrichment: succeeded")?;
            writeln!(output, "run_id: {}", report.run_id)?;
            writeln!(output, "source: musicbrainz")?;
            writeln!(output, "artists_considered: {}", report.artists_considered)?;
            writeln!(
                output,
                "track_artists_considered: {}",
                report.track_artists_considered
            )?;
            writeln!(output, "requests_made: {}", report.requests_made)?;
            writeln!(output, "cache_hits: {}", report.cache_hits)?;
            writeln!(output, "resolved_artists: {}", report.resolved_artists)?;
            writeln!(output, "unknown_artists: {}", report.unknown_artists)?;
            writeln!(output, "error_artists: {}", report.error_artists)?;
            writeln!(output, "facts_written: {}", report.facts_written)?;
            Ok(())
        }
        EnrichmentCommand::ModelImport { account, file } => {
            let report = model_inference::import(database, &account, &file).await?;
            writeln!(output, "model_inference_import: succeeded")?;
            writeln!(output, "import_id: {}", report.import_id)?;
            writeln!(output, "reused: {}", report.reused)?;
            writeln!(output, "model: {}", report.model)?;
            writeln!(output, "model_version: {}", report.model_version)?;
            writeln!(output, "tracks_imported: {}", report.tracks_imported)?;
            writeln!(output, "facts_imported: {}", report.facts_imported)?;
            writeln!(output, "manifest_sha256: {}", report.manifest_sha256)?;
            Ok(())
        }
        EnrichmentCommand::ModelStatus { account } => {
            let report = model_inference::status(database, &account).await?;
            writeln!(output, "model_inference: current")?;
            writeln!(output, "eligible_tracks: {}", report.eligible_tracks)?;
            writeln!(output, "inferred_tracks: {}", report.inferred_tracks)?;
            writeln!(output, "embedded_tracks: {}", report.embedded_tracks)?;
            writeln!(output, "facts: {}", report.facts)?;
            writeln!(
                output,
                "models: {}",
                if report.models.is_empty() {
                    "-".to_owned()
                } else {
                    report.models.join(", ")
                }
            )?;
            writeln!(
                output,
                "latest_import_at: {}",
                report
                    .latest_import_at
                    .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
            )?;
            Ok(())
        }
        EnrichmentCommand::Status { account } => {
            let report = enrichment::status(database, &account).await?;
            writeln!(output, "enrichment: current")?;
            writeln!(output, "source: musicbrainz")?;
            writeln!(output, "eligible_tracks: {}", report.eligible_tracks)?;
            writeln!(output, "tracks_with_isrc: {}", report.tracks_with_isrc)?;
            writeln!(output, "matched_tracks: {}", report.matched_tracks)?;
            writeln!(output, "ambiguous_tracks: {}", report.ambiguous_tracks)?;
            writeln!(output, "unmatched_tracks: {}", report.unmatched_tracks)?;
            writeln!(output, "error_tracks: {}", report.error_tracks)?;
            writeln!(output, "facts: {}", report.facts)?;
            writeln!(
                output,
                "tracks_with_artist_area: {}",
                report.tracks_with_artist_area
            )?;
            writeln!(output, "artist_area_facts: {}", report.artist_area_facts)?;
            writeln!(
                output,
                "latest_run_at: {}",
                report
                    .latest_run_at
                    .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
            )?;
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

fn bookmark_selector(
    name: Option<String>,
    spotify_id: Option<String>,
) -> bookmarks::BookmarkSelector {
    match (name, spotify_id) {
        (Some(name), None) => bookmarks::BookmarkSelector::Name(name),
        (None, Some(id)) => bookmarks::BookmarkSelector::ProviderId(id),
        _ => unreachable!("clap enforces exactly one bookmark selector"),
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

fn playlist_signal_class(value: PlaylistSignalClassArg) -> playlists::PlaylistSignalClass {
    match value {
        PlaylistSignalClassArg::SemanticLegacy => playlists::PlaylistSignalClass::SemanticLegacy,
        PlaylistSignalClassArg::ProviderCurated => playlists::PlaylistSignalClass::ProviderCurated,
        PlaylistSignalClassArg::Intake => playlists::PlaylistSignalClass::Intake,
        PlaylistSignalClassArg::Canonical => playlists::PlaylistSignalClass::Canonical,
        PlaylistSignalClassArg::Transport => playlists::PlaylistSignalClass::Transport,
        PlaylistSignalClassArg::Ignored => playlists::PlaylistSignalClass::Ignored,
    }
}

fn behavioral_signal(value: BehavioralSignalArg) -> playlists::BehavioralSignal {
    match value {
        BehavioralSignalArg::Rotation => playlists::BehavioralSignal::Rotation,
        BehavioralSignalArg::Discovery => playlists::BehavioralSignal::Discovery,
        BehavioralSignalArg::Prompted => playlists::BehavioralSignal::Prompted,
        BehavioralSignalArg::Recommendation => playlists::BehavioralSignal::Recommendation,
    }
}

fn clear_policy(value: ClearPolicyArg) -> playlists::ClearPolicy {
    match value {
        ClearPolicyArg::Never => playlists::ClearPolicy::Never,
        ClearPolicyArg::AfterVerifiedAssignment => playlists::ClearPolicy::AfterVerifiedAssignment,
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
    writeln!(output, "external_bookmarks: {}", report.external_bookmarks)?;
    writeln!(
        output,
        "external_bookmarks_reused: {}",
        report.external_bookmarks_reused
    )?;
    writeln!(
        output,
        "external_bookmark_entries: {}",
        report.external_bookmark_entries
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

fn write_sync_plan_report(output: &mut impl Write, report: &sync_plan::PlanReport) -> Result<()> {
    writeln!(
        output,
        "sync_plan: {}",
        if report.reused {
            "already current"
        } else {
            "created"
        }
    )?;
    writeln!(output, "plan_id: {}", report.plan_id)?;
    writeln!(
        output,
        "proposal_generation_id: {}",
        report.proposal_generation_id
    )?;
    writeln!(output, "source_snapshot_id: {}", report.source_snapshot_id)?;
    writeln!(output, "operations: {}", report.operation_count)?;
    writeln!(output, "creates: {}", report.creates)?;
    writeln!(output, "renames: {}", report.renames)?;
    writeln!(output, "additions: {}", report.additions)?;
    writeln!(output, "restorations: {}", report.restorations)?;
    writeln!(output, "exclusions: {}", report.exclusions)?;
    writeln!(output, "removals: {}", report.removals)?;
    writeln!(output, "retirements: {}", report.retirements)?;
    writeln!(output, "external_cleanups: {}", report.external_cleanups)?;
    writeln!(output, "deferred: {}", report.deferred)?;
    writeln!(output, "input_hash: {}", report.input_hash)?;
    writeln!(output, "created_at: {}", report.created_at.to_rfc3339())?;
    writeln!(output, "spotify_writes: disabled")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser;

    use super::{
        BehavioralSignalArg, BookmarkCommand, Cli, ClusterCommand, Command, DbCommand,
        EmbeddingCommand, EnrichmentCommand, HistoryCommand, PlaylistCommand, PlaylistRoleArg,
        PlaylistSignalClassArg, SignalCommand, SpotifyCommand, SyncCommand, write_status,
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
    fn parses_dry_run_sync_plan() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "sync",
            "plan",
            "--proposal",
            "ca81d1b2-e56b-41e6-8846-cdb379cb039b",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::Plan {
                    account,
                    proposal: Some(_)
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_detailed_sync_plan_inspection() {
        let cli = Cli::try_parse_from(["chordrift", "sync", "plan-show", "--details"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::PlanShow {
                    account,
                    plan: None,
                    details: true
                }
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
    fn parses_bookmark_tracks_by_name() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "bookmarks",
            "tracks",
            "--name",
            "alone in the car",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Bookmarks {
                command: BookmarkCommand::Tracks {
                    account,
                    name: Some(name),
                    ..
                }
            } if account == "personal" && name == "alone in the car"
        ));
    }

    #[test]
    fn parses_exact_bookmark_cleanup_approval() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "bookmarks",
            "cleanup-approve",
            "--confirm",
            "016defcd-f46b-4070-991d-73cb4c89f00a",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Bookmarks {
                command: BookmarkCommand::CleanupApprove { account, .. }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_playlist_signal_policy() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "playlists",
            "signals",
            "--name",
            "On Repeat",
            "--class",
            "provider-curated",
            "--behavior",
            "rotation",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Playlists {
                command: PlaylistCommand::Signals {
                    account,
                    name: Some(name),
                    class: PlaylistSignalClassArg::ProviderCurated,
                    behavior: Some(BehavioralSignalArg::Rotation),
                    ..
                }
            } if account == "personal" && name == "On Repeat"
        ));
    }

    #[test]
    fn parses_default_signal_generation() {
        let cli = Cli::try_parse_from(["chordrift", "signals", "generate"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Signals {
                command: SignalCommand::Generate { account }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_bounded_musicbrainz_enrichment() {
        let cli = Cli::try_parse_from(["chordrift", "enrich", "musicbrainz", "--limit", "10"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Enrich {
                command: EnrichmentCommand::Musicbrainz {
                    account,
                    limit: 10,
                    refresh: false
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_bounded_artist_area_enrichment() {
        let cli = Cli::try_parse_from(["chordrift", "enrich", "artists", "--limit", "10"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Enrich {
                command: EnrichmentCommand::Artists { account, limit: 10 }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_model_inference_import() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "enrich",
            "model-import",
            "--file",
            "inference.json",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Enrich {
                command: EnrichmentCommand::ModelImport { account, file }
            } if account == "personal" && file.to_str() == Some("inference.json")
        ));
    }

    #[test]
    fn parses_model_inference_status() {
        let cli =
            Cli::try_parse_from(["chordrift", "enrich", "model-status"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Enrich {
                command: EnrichmentCommand::ModelStatus { account }
            } if account == "personal"
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
    fn parses_default_cluster_generation() {
        let cli =
            Cli::try_parse_from(["chordrift", "clusters", "generate"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Clusters {
                command: ClusterCommand::Generate {
                    account,
                    count: 12,
                    min_similarity,
                    min_cluster_size: 10,
                    seed: None
                }
            } if account == "personal" && (min_similarity - 0.05).abs() < f64::EPSILON
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
