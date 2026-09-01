use std::{
    future::Future,
    io::{self, Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand, ValueEnum};
use sqlx::Row as _;
use zeroize::Zeroizing;

use crate::{
    ChordriftError, Result, albums, analysis,
    application::{ApplicationFacade, ApplicationInvocation},
    apply, apply_readiness, artwork, bookmarks, classifications,
    client_transport::{ClientTransport, RemoteHttpClient},
    clusters, config,
    contract::{
        CAPABILITY_DURABLE_OPERATIONS, CAPABILITY_MAINTENANCE_TASK_SESSION,
        CAPABILITY_PRODUCT_IDENTITY, CAPABILITY_REMOTE_CLI, CONTRACT_VERSION, ClientCompatibility,
        Command as ContractCommand, CommandRequest, ContractVersionRange, IdempotencyKey,
        MaintenanceDecision, MaintenanceReviewId, MaintenanceSessionId, Query, QueryRequest,
        RequestId, ResourceId, SchemaVersionRange,
    },
    credentials::{CredentialStore, SecretId, SystemCredentialStore},
    db, db_cleanup, db_reports, db_v2_migration, embeddings, enrichment, history, intake,
    model_inference, onboarding, onboarding_audit, playlists, presentation, product_rehearsal,
    proposals,
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderVaultKeyring,
    },
    providers::spotify,
    recipe_execution, routes,
    service::AuthenticatedSubject,
    signals, spin_preview, sync_plan, terminal, tracks,
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
    /// Print and optionally require stable installed-binary capabilities as JSON.
    Capabilities {
        /// Capability that must be available; may be repeated.
        #[arg(long = "require", value_name = "CAPABILITY")]
        required: Vec<String>,
    },
    /// Call the authenticated hosted application contract as a thin client.
    Service {
        /// Remote service client operation.
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Rehearse the provider-neutral v0.2 product through the local CLI.
    Product {
        /// Product operation to perform.
        #[command(subcommand)]
        command: ProductCommand,
    },
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
    /// Audit current provider intake against Chordrift intent and history.
    Intake {
        /// Intake operation to perform.
        #[command(subcommand)]
        command: IntakeCommand,
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
    /// Inventory saved albums separately from playlists and saved songs.
    Albums {
        /// Saved-album operation to perform.
        #[command(subcommand)]
        command: AlbumCommand,
    },
    /// Historical Re-evaluate migration and retirement commands.
    #[command(hide = true)]
    Reevaluate {
        /// Re-evaluation operation to perform.
        #[command(subcommand)]
        command: ReevaluateCommand,
    },
    /// Legacy multi-route commands retained only for migration.
    #[command(hide = true)]
    Routes {
        /// Routing operation to perform.
        #[command(subcommand)]
        command: RouteCommand,
    },
    /// Find one song and explain its placement, provenance, signals, and clustering.
    Tracks {
        /// Track operation to perform.
        #[command(subcommand)]
        command: TrackCommand,
    },
    /// Author private revisioned track dimensions directly or through reviewed CSV batches.
    Classify {
        /// Classification operation to perform.
        #[command(subcommand)]
        command: ClassificationCommand,
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
    /// Validate, inspect, and explicitly approve local canonical playlist covers.
    Artwork {
        /// Artwork operation to perform.
        #[command(subcommand)]
        command: ArtworkCommand,
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

/// Authenticated service-client commands. Hosting and login UX arrive in V021-06.
#[derive(Clone, Debug, Subcommand)]
pub enum ServiceCommand {
    /// Adopt the existing local provider authorization into encrypted hosted storage.
    #[command(hide = true)]
    AdoptLocalProviderCredential {
        /// Local provider account label whose OS credential is adopted.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Store, inspect, or remove the opaque Chordrift session in the OS credential store.
    Session {
        /// Session operation.
        #[command(subcommand)]
        command: ServiceSessionCommand,
    },
    /// Start, inspect, refresh, or resolve one durable ordinary-maintenance session.
    Maintenance {
        /// Maintenance-session operation.
        #[command(subcommand)]
        command: ServiceMaintenanceCommand,
    },
    /// Inspect hosted library state through typed read-only queries.
    Library {
        /// Library query.
        #[command(subcommand)]
        command: ServiceLibraryCommand,
    },
    /// Negotiate contract, schema, and capabilities with one authenticated service.
    Compatibility {
        /// HTTPS service base URL; loopback HTTP is allowed for development tests.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Submit one exact typed `CommandRequest` JSON document.
    Command {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Typed command envelope JSON file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Submit one exact typed `QueryRequest` JSON document.
    Query {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Typed query envelope JSON file.
        #[arg(long)]
        file: PathBuf,
    },
}

/// Thin remote client queries for provider and Chordrift library state.
#[derive(Clone, Debug, Subcommand)]
pub enum ServiceLibraryCommand {
    /// Explain provider/model playlist count, membership, and order differences.
    Compare {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Provider connection shown by the hosted provider selector.
        #[arg(long)]
        provider_connection_id: uuid::Uuid,
    },
}

/// Thin remote client operations for durable ordinary maintenance.
#[derive(Clone, Debug, Subcommand)]
pub enum ServiceMaintenanceCommand {
    /// Observe the provider and start a cumulative record-only interpretation.
    Start {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Provider connection shown by the hosted library view.
        #[arg(long)]
        provider_connection_id: uuid::Uuid,
        /// Stable session identity; generated when omitted.
        #[arg(long)]
        session_id: Option<uuid::Uuid>,
    },
    /// Read the exact durable maintenance revision.
    Show {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Maintenance session to inspect.
        #[arg(long)]
        session_id: uuid::Uuid,
    },
    /// Observe the provider again and cumulatively rebase a session.
    Refresh {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Maintenance session to refresh.
        #[arg(long)]
        session_id: uuid::Uuid,
        /// Exact revision currently displayed by the client.
        #[arg(long)]
        expected_revision: u64,
    },
    /// Persist ambiguity decisions from one exact displayed revision.
    Resolve {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Maintenance session receiving the decisions.
        #[arg(long)]
        session_id: uuid::Uuid,
        /// Exact revision currently displayed by the client.
        #[arg(long)]
        expected_revision: u64,
        /// JSON file containing an array of typed `MaintenanceDecision` values.
        #[arg(long)]
        decisions: PathBuf,
    },
    /// Authorize exactly the immutable provider effects shown by `show`.
    Authorize {
        /// HTTPS service base URL.
        #[arg(long)]
        url: String,
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Maintenance session containing the review.
        #[arg(long)]
        session_id: uuid::Uuid,
        /// Exact revision currently displayed by the client.
        #[arg(long)]
        expected_revision: u64,
        /// Immutable review identity displayed by the client.
        #[arg(long)]
        review_id: uuid::Uuid,
    },
}

/// OS credential-store operations for one remote Chordrift session.
#[derive(Clone, Debug, Subcommand)]
pub enum ServiceSessionCommand {
    /// Read an opaque Chordrift session from standard input and store it securely.
    Save {
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Report whether the profile has a stored session without printing it.
    Status {
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
    },
    /// Delete the profile's local Chordrift session.
    Remove {
        /// Local credential-store profile.
        #[arg(long, default_value = "default")]
        profile: String,
    },
}

/// Read-only current-provider intake commands.
#[derive(Clone, Debug, Subcommand)]
pub enum IntakeCommand {
    /// Join the exact current intake inventory with coverage, exclusions, and history.
    Audit {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Record whether one currently liked track remains saved after verified placement.
    LikedDisposition {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track identity shown by the intake audit.
        #[arg(long)]
        spotify_id: String,
        /// Explicit handling after canonical placement is verified.
        #[arg(long, value_enum)]
        disposition: LikedDispositionArg,
        /// Human-readable reason retained with the revision.
        #[arg(long)]
        reason: String,
    },
}

/// CLI spelling for an explicit saved/liked intake disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum LikedDispositionArg {
    /// Retain the track in Liked Songs as well as its managed destination.
    Preserve,
    /// Remove it from Liked Songs only after verified canonical placement.
    ClearAfterVerifiedAssignment,
}

impl From<LikedDispositionArg> for intake::SavedTrackDisposition {
    fn from(value: LikedDispositionArg) -> Self {
        match value {
            LikedDispositionArg::Preserve => Self::Preserve,
            LikedDispositionArg::ClearAfterVerifiedAssignment => Self::ClearAfterVerifiedAssignment,
        }
    }
}

/// Provider-write-free v0.2 product rehearsal commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProductCommand {
    /// Capture or audit fixture-backed onboarding inputs.
    Onboarding {
        /// Onboarding operation to perform.
        #[command(subcommand)]
        command: ProductOnboardingCommand,
    },
    /// Review account-owned overlapping collections.
    Collections {
        /// Collection operation to perform.
        #[command(subcommand)]
        command: ProductCollectionCommand,
    },
    /// Review or execute an immutable recipe revision.
    Recipes {
        /// Recipe operation to perform.
        #[command(subcommand)]
        command: ProductRecipeCommand,
    },
    /// Create or display an exact deterministic Spin preview.
    Spins {
        /// Spin operation to perform.
        #[command(subcommand)]
        command: ProductSpinCommand,
    },
}

/// Fixture-backed onboarding commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProductOnboardingCommand {
    /// Persist one provider-read-only fixture capture.
    Capture {
        /// JSON fixture containing account context plus baseline and enriched inputs.
        #[arg(long)]
        fixture: PathBuf,
        /// Evidence path to capture.
        #[arg(long, value_enum, default_value_t = ProductAuditMode::InventoryOnly)]
        mode: ProductAuditMode,
        /// Optional stable retry identity; generated when omitted.
        #[arg(long)]
        idempotency_key: Option<uuid::Uuid>,
    },
    /// Read one persisted inventory-only or enriched onboarding audit.
    Audit {
        /// JSON fixture supplying the validated account context.
        #[arg(long)]
        fixture: PathBuf,
        /// Captured onboarding session UUID.
        #[arg(long)]
        session: uuid::Uuid,
        /// Evidence path represented by the session.
        #[arg(long, value_enum, default_value_t = ProductAuditMode::InventoryOnly)]
        mode: ProductAuditMode,
    },
}

/// Evidence path used by onboarding capture and audit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ProductAuditMode {
    /// Current provider inventory only.
    InventoryOnly,
    /// Current inventory plus explicitly selected extended history.
    Enriched,
}

/// Collection review commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProductCollectionCommand {
    /// List account-owned collections and current membership counts.
    List {
        /// Chordrift account UUID.
        #[arg(long)]
        account: uuid::Uuid,
    },
}

/// Immutable recipe review and execution commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProductRecipeCommand {
    /// Show one persisted immutable recipe revision.
    Show {
        /// Chordrift account UUID.
        #[arg(long)]
        account: uuid::Uuid,
        /// Recipe revision UUID.
        #[arg(long)]
        revision: uuid::Uuid,
    },
    /// Execute fixture candidates into the exact unordered V020-09 draft.
    Execute {
        /// JSON fixture containing validated recipe candidates and Spin seed.
        #[arg(long)]
        fixture: PathBuf,
    },
}

/// Deterministic Spin preview commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ProductSpinCommand {
    /// Execute a fixture recipe and persist its exact ordered preview.
    Preview {
        /// JSON fixture containing validated recipe candidates and Spin seed.
        #[arg(long)]
        fixture: PathBuf,
    },
    /// Display one persisted account-owned Spin preview.
    Show {
        /// Chordrift account UUID.
        #[arg(long)]
        account: uuid::Uuid,
        /// Spin UUID.
        #[arg(long)]
        spin: uuid::Uuid,
    },
}

/// Provider-native Re-evaluate queue commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ReevaluateCommand {
    /// Create or update the single Neon-backed Re-evaluate playlist.
    Create {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Instructions shown in the Spotify playlist description.
        #[arg(
            long,
            default_value = "Move a misplaced track here and remove it from its current destination. Chordrift will preserve it for later reassignment."
        )]
        description: String,
        /// Label-free PNG master retained for future providers.
        #[arg(long)]
        background: PathBuf,
        /// Deterministically labeled PNG approved for Spotify.
        #[arg(long)]
        artwork: PathBuf,
    },
    /// Show current queue configuration and observed provider membership.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Export the current Spotify queue to the standard classification worksheet.
    Export {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Destination CSV file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Retire the obsolete multi-route review surfaces after coverage is complete.
    RetireLegacy {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact destructive confirmation phrase.
        #[arg(long)]
        confirm: String,
    },
    /// Retire the empty Re-evaluate surface and preserve its Neon history.
    Retire {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact destructive confirmation phrase.
        #[arg(long)]
        confirm: String,
    },
}

/// Canonical database commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DbCommand {
    /// Check connectivity and report migration state without changing it.
    Status,
    /// Apply pending Chordrift schema migrations.
    Migrate,
    /// Report logical invariants needed before and after database-v2 migration.
    InvariantReport {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Report heap, table, index, and total bytes for every database table.
    StorageReport,
    /// Describe database cleanup effects without changing any rows.
    Compact {
        /// Compaction operation to perform.
        #[command(subcommand)]
        command: DbCompactCommand,
    },
    /// Inspect additive database-v2 materialization and cutover gates.
    V2 {
        /// Database-v2 operation to perform.
        #[command(subcommand)]
        command: DbV2Command,
    },
}

/// Provider-free database compaction commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DbCompactCommand {
    /// Plan retention and normalization inside a read-only transaction.
    Plan {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Exact-plan, apply, or verify removal of superseded database-v1 storage.
    Cleanup {
        /// Cleanup phase.
        #[command(subcommand)]
        command: DbCleanupCommand,
    },
}

/// Destructive database cleanup phases, each provider-free.
#[derive(Clone, Debug, Subcommand)]
pub enum DbCleanupCommand {
    /// Emit the exact cleanup confirmation hash without writing.
    Plan {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Apply only the exact plan hash supplied by the operator.
    Apply {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact SHA-256 emitted by `cleanup plan`.
        #[arg(long)]
        confirm: String,
    },
    /// Verify the latest cleanup receipt and durable invariants.
    Verify {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Additive database-v2 inspection commands.
#[derive(Clone, Debug, Subcommand)]
pub enum DbV2Command {
    /// Compare v2 materialization with legacy state without changing either.
    Status {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Plan, exact-confirm, or verify normalized evidence and checkpoints.
    Migration {
        /// Migration phase.
        #[command(subcommand)]
        command: DbV2MigrationCommand,
    },
    /// Print an exact production cutover plan without applying it.
    CutoverPlan {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Provider-free database-v2 migration phases.
#[derive(Clone, Debug, Subcommand)]
pub enum DbV2MigrationCommand {
    /// Describe exact migration inputs and print the required confirmation hash.
    Plan {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Apply a plan to the configured database after exact confirmation.
    Apply {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact SHA-256 emitted by `migration plan`.
        #[arg(long)]
        confirm: String,
    },
    /// Verify migrated evidence and checkpoint parity without changing rows.
    Verify {
        /// Local label for the provider account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
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
    /// Configure native Spotify library surfaces without changing them immediately.
    LibraryPolicy {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Handling for tracks captured through Liked Songs.
        #[arg(long, value_enum)]
        liked_songs: LikedSongsPolicyArg,
    },
    /// Remove the local refresh token without revoking Spotify access.
    Logout {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
}

/// Handling for tracks captured through Spotify Liked Songs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum LikedSongsPolicyArg {
    /// Keep tracks in Liked Songs after Chordrift placement.
    Preserve,
    /// Clear only after canonical placement or exclusion is durably verified.
    ClearAfterVerifiedAssignment,
}

impl LikedSongsPolicyArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::ClearAfterVerifiedAssignment => "after_verified_assignment",
        }
    }
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
    /// Checkpoint an exactly converged provider observation without provider writes.
    AcceptCurrent {
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
    /// Prove future apply safety against an immutable plan without provider writes.
    Readiness {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact plan ID; defaults to the newest plan.
        #[arg(long)]
        plan: Option<uuid::Uuid>,
        /// Perform one authenticated read-only identity and OAuth-scope probe.
        #[arg(long)]
        probe: bool,
    },
    /// Show the latest or selected immutable apply-readiness assessment.
    ReadinessShow {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact assessment ID; defaults to the newest assessment.
        #[arg(long)]
        assessment: Option<uuid::Uuid>,
    },
    /// Execute exactly one gated phase from a ready immutable Spotify plan.
    Apply {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact ready assessment to execute.
        #[arg(long)]
        assessment: uuid::Uuid,
        /// One independently gated execution phase.
        #[arg(long, value_enum)]
        phase: ApplyPhaseArg,
        /// Repeat the exact assessment ID as an explicit confirmation.
        #[arg(long)]
        confirm: uuid::Uuid,
        /// Required in addition to confirmation for cleanup or retirement.
        #[arg(long)]
        allow_destructive: bool,
    },
    /// Validate local publish artifacts and estimate requests without contacting Spotify.
    ApplyPreflight {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact plan ID; defaults to the newest plan.
        #[arg(long)]
        plan: Option<uuid::Uuid>,
    },
    /// Approve all legacy retirement operations in one exact inspected plan.
    RetirementApprove {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact current plan containing the reviewed retirements.
        #[arg(long)]
        plan: uuid::Uuid,
        /// Repeat the exact plan ID as confirmation.
        #[arg(long)]
        confirm: uuid::Uuid,
    },
    /// Show the latest or selected durable apply execution.
    ApplyShow {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact apply run; defaults to the latest.
        #[arg(long)]
        run: Option<uuid::Uuid>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
/// CLI representation of independently gated apply phases.
pub enum ApplyPhaseArg {
    /// Publish canonical playlists and approved artwork.
    Publish,
    /// Reconcile managed state without deferred cleanup.
    Reconcile,
    /// Clear verified intake and approved external relationships.
    Cleanup,
    /// Remove separately approved legacy playlist relationships.
    Retirement,
}

impl From<ApplyPhaseArg> for apply::ApplyPhase {
    fn from(value: ApplyPhaseArg) -> Self {
        match value {
            ApplyPhaseArg::Publish => Self::Publish,
            ApplyPhaseArg::Reconcile => Self::Reconcile,
            ApplyPhaseArg::Cleanup => Self::Cleanup,
            ApplyPhaseArg::Retirement => Self::Retirement,
        }
    }
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
    /// Select user playlists eligible for a separately approved future retirement.
    Retirement {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Mark this exact playlist name for retirement; repeat for multiple names.
        #[arg(long)]
        include: Vec<String>,
        /// Mark all eligible user playlists for retirement.
        #[arg(long)]
        all: bool,
        /// Protect this exact name when using --all; repeat for multiple names.
        #[arg(long, requires = "all")]
        except: Vec<String>,
        /// Protect every eligible user playlist and retire none.
        #[arg(long)]
        none: bool,
    },
}

/// Saved-album inventory and cleanup-policy commands.
#[derive(Clone, Debug, Subcommand)]
pub enum AlbumCommand {
    /// List current saved albums and preservation coverage.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List every saved album ever inventoried, including retired containers.
    History {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Summarize whether every album track is preserved, excluded, or awaiting review.
    Audit {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List ordered tracks and disposition for one saved album.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Case-insensitive exact album title; must be unambiguous.
        #[arg(
            long,
            required_unless_present = "spotify_id",
            conflicts_with = "spotify_id"
        )]
        name: Option<String>,
        /// Stable Spotify album ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
    },
    /// Set an account-specific policy; this never changes Spotify by itself.
    Policy {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Saved-album handling mode.
        #[arg(long, value_enum)]
        mode: SavedAlbumPolicyArg,
    },
}

/// Account-specific saved-album behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SavedAlbumPolicyArg {
    /// Preserve saved albums and only inventory them.
    Preserve,
    /// Inventory changes without proposing album cleanup.
    InventoryOnly,
    /// Require every album track to be reviewed before a future unsave operation.
    ReviewThenUnsave,
    /// Retire album containers while retaining their immutable Neon inventory.
    ArchiveOnly,
}

impl SavedAlbumPolicyArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::InventoryOnly => "inventory_only",
            Self::ReviewThenUnsave => "review_then_unsave",
            Self::ArchiveOnly => "archive_only",
        }
    }
}

/// Durable routing-playlist commands.
#[derive(Clone, Debug, Subcommand)]
pub enum RouteCommand {
    /// Create or update a Neon-backed route and its approved artwork.
    Create {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Route label; Chordrift adds `Route —` when omitted.
        #[arg(long)]
        name: String,
        /// Instructions explaining when this route should be used.
        #[arg(long)]
        description: String,
        /// Label-free PNG master retained for future providers.
        #[arg(long)]
        background: PathBuf,
        /// Deterministically labeled PNG approved for Spotify.
        #[arg(long)]
        artwork: PathBuf,
    },
    /// Add known Spotify tracks to one route without contacting Spotify.
    Add {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Route name, with or without the `Route —` prefix.
        #[arg(long)]
        route: String,
        /// Stable Spotify track ID; repeat for multiple tracks.
        #[arg(long = "spotify-id", required = true)]
        spotify_ids: Vec<String>,
        /// Optional durable note explaining the routing decision.
        #[arg(long)]
        reason: Option<String>,
    },
    /// List every configured route and its publication state.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List desired Neon membership for one route.
    Tracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Route name, with or without the `Route —` prefix.
        #[arg(long)]
        route: String,
    },
}

/// Canonical track lookup commands.
#[derive(Clone, Debug, Subcommand)]
pub enum TrackCommand {
    /// Explain where one track is, where it came from, and why Chordrift placed it.
    Inspect {
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
        /// Optional artist substring used to disambiguate an exact title.
        #[arg(long, requires = "name")]
        artist: Option<String>,
        /// Stable Spotify track ID.
        #[arg(long, required_unless_present = "name", conflicts_with = "name")]
        spotify_id: Option<String>,
        /// Include internal generation IDs, stable keys, and raw placement provenance.
        #[arg(long)]
        technical: bool,
    },
    /// List active reversible exclusions retained by Chordrift.
    Exclusions {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Empty the exclusion archive without deleting audit history or writing Spotify.
    EmptyExclusions {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Must exactly repeat the account label.
        #[arg(long)]
        confirm: String,
    },
    /// Reversibly exclude one exact track from active Chordrift destinations.
    Exclude {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track ID.
        #[arg(long = "spotify-id")]
        spotify_id: String,
        /// Required durable explanation.
        #[arg(long)]
        reason: String,
        /// Must exactly repeat the Spotify track ID.
        #[arg(long)]
        confirm: String,
    },
    /// Restore one excluded track to unresolved review without guessing a destination.
    Restore {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track ID.
        #[arg(long = "spotify-id")]
        spotify_id: String,
        /// Required durable explanation.
        #[arg(long)]
        reason: String,
        /// Must exactly repeat the Spotify track ID.
        #[arg(long)]
        confirm: String,
    },
}

/// Private user-authored track classification commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ClassificationCommand {
    /// Immediately set explicit dimensions for one known Spotify track.
    Set {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track ID; repeat for a small atomic batch.
        #[arg(long = "spotify-id", required = true)]
        spotify_ids: Vec<String>,
        /// Broad collection such as `south-asian`.
        #[arg(long)]
        collection: Option<String>,
        /// Region or cultural region; repeat for multiple values.
        #[arg(long = "region")]
        regions: Vec<String>,
        /// Musical tradition; repeat for multiple values.
        #[arg(long = "tradition")]
        traditions: Vec<String>,
        /// Personal cohort; repeat for multiple values.
        #[arg(long = "cohort")]
        cohorts: Vec<String>,
        /// Language tag or `instrumental`; repeat for multiple values.
        #[arg(long = "language")]
        languages: Vec<String>,
        /// Optional private context excluded from embedding features.
        #[arg(long)]
        notes: Option<String>,
        /// Required explanation retained in revision history.
        #[arg(long)]
        reason: String,
    },
    /// Clear one active classification while retaining its history.
    Clear {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track ID; repeat for a small atomic batch.
        #[arg(long = "spotify-id", required = true)]
        spotify_ids: Vec<String>,
        /// Required explanation retained in history.
        #[arg(long)]
        reason: String,
    },
    /// Show active and superseded user revisions for one track.
    History {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable Spotify track ID.
        #[arg(long = "spotify-id")]
        spotify_id: String,
    },
    /// Export deduplicated playlist tracks to an inert CSV review worksheet.
    Export {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Current Spotify playlist name; repeat to combine playlists.
        #[arg(long = "playlist", required = true)]
        playlists: Vec<String>,
        /// Destination CSV file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Stage CSV rows explicitly marked `set` or `clear` as a draft batch.
    Import {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Reviewed CSV file.
        #[arg(long)]
        file: PathBuf,
    },
    /// Activate one exact staged CSV batch.
    Approve {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Draft batch identity.
        #[arg(long)]
        batch: uuid::Uuid,
        /// Must exactly repeat the batch identity.
        #[arg(long)]
        confirm: uuid::Uuid,
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
    /// Explicitly refresh one bookmark's metadata and readable contents.
    Refresh {
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
    /// Audit the complete preserved-library inventory by source class.
    Inventory {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List preserved tracks that have neither a placement nor an explicit exclusion.
    Unresolved {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Maximum tracks to report.
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    /// Score unresolved inventory against the approved playlist destinations.
    PlacementAudit {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Clone the approved structure and append credible existing-destination fits.
    Extend {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Minimum cosine similarity to an approved playlist centroid.
        #[arg(long, default_value_t = 0.05, allow_hyphen_values = true)]
        min_similarity: f64,
    },
    /// List currently unresolved tracks in one analytical cluster.
    GroupTracks {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Machine cluster label reported by `proposals placement-audit`.
        #[arg(long)]
        cluster: String,
        /// Maximum tracks to report.
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Assign unresolved embedded tracks using dominant analytical-group evidence.
    ConsensusAssign {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Minimum dominant share among already placed group members.
        #[arg(long, default_value_t = 0.55)]
        min_dominance: f64,
        /// Minimum already placed members required as evidence.
        #[arg(long, default_value_t = 10)]
        min_evidence: u32,
    },
    /// Assign unresolved embedded tracks with credible direct centroid fit.
    CentroidAssign {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Minimum cosine similarity to the current destination centroid.
        #[arg(long, default_value_t = 0.05, allow_hyphen_values = true)]
        min_similarity: f64,
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
    /// Assign or move one or more tracks to a stable proposed playlist.
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
    /// Accept current provider order after proving exact membership equality.
    AlignProviderOrder {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable destination key reported by `proposals list`.
        #[arg(long)]
        playlist: String,
    },
    /// Remove one empty destination from the editable proposal.
    RetireEmpty {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Stable playlist key reported by `proposals list`.
        #[arg(long)]
        playlist: String,
        /// Must exactly repeat the stable playlist key.
        #[arg(long)]
        confirm: String,
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

/// Local canonical playlist artwork commands.
#[derive(Clone, Debug, Subcommand)]
pub enum ArtworkCommand {
    /// Overlay a Spotify-style title on one pristine 1254px background.
    Render {
        /// Label-free PNG background to preserve and use as input.
        #[arg(long)]
        background: PathBuf,
        /// Playlist title to render.
        #[arg(long)]
        title: String,
        /// New labeled PNG output; must differ from the background.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate a strict manifest and register an immutable local review set.
    Import {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Strict artwork manifest and its sibling PNG files.
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Show the latest artwork review state and contact sheet.
    Status {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// List the verified cover files and content hashes.
    List {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
    },
    /// Approve one exact immutable artwork batch without provider writes.
    Approve {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact batch ID printed by `artwork import` or `artwork status`.
        #[arg(long)]
        confirm: uuid::Uuid,
    },
    /// Build an immutable one-cover update plan for a playlist or stable key.
    Update {
        /// Local label for this Spotify account.
        #[arg(long, default_value = "personal")]
        account: String,
        /// Exact playlist name or stable artwork key.
        #[arg(long)]
        playlist: String,
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
    /// Protected user-created playlist that Chordrift never retires implicitly.
    UserManaged,
    /// User-curated legacy vibe evidence.
    SemanticLegacy,
    /// Spotify-owned behavioral evidence.
    ProviderCurated,
    /// User-owned temporary intake.
    Intake,
    /// Zero-signal corrective routing inbox.
    Routing,
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
    if terminal::stdout_is_terminal() {
        let mut command_output = Vec::new();
        run_with_writer(cli, &mut command_output).await?;
        let command_output = String::from_utf8(command_output).map_err(|_| {
            ChordriftError::Configuration("command output was not valid UTF-8".to_owned())
        })?;
        output.write_all(presentation::render_interactive(&command_output).as_bytes())?;
        Ok(())
    } else {
        run_with_writer(cli, &mut output).await
    }
}

async fn run_with_writer(cli: Cli, output: &mut impl Write) -> Result<()> {
    ApplicationFacade::new()
        .invoke(CliInvocation { cli, output })
        .await
}

struct CliInvocation<'a, W> {
    cli: Cli,
    output: &'a mut W,
}

impl<W> ApplicationInvocation for CliInvocation<'_, W>
where
    W: Write,
{
    type Output = ();

    fn execute(self) -> impl Future<Output = Result<Self::Output>> {
        execute_cli_handlers(self.cli, self.output)
    }
}

async fn execute_cli_handlers(cli: Cli, output: &mut impl Write) -> Result<()> {
    match cli.command {
        Command::Capabilities { required } => {
            let manifest = binary_capability_manifest();
            for capability in required {
                if !manifest.supports(&capability) {
                    return Err(ChordriftError::Configuration(format!(
                        "required binary capability is unavailable: {capability}"
                    )));
                }
            }
            writeln!(output, "{}", serde_json::to_string(&manifest)?)?;
        }
        Command::Service { command } => run_service_command(command, output).await?,
        Command::Product { command } => run_product_command(command, output).await?,
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
                let result: Result<()> = async {
                    db::require_schema_through(&database, 47).await?;
                    let report = spotify::import(&account, &database).await?;
                    write_import_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SpotifyCommand::LibraryPolicy {
                account,
                liked_songs,
            } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    albums::set_saved_track_policy(&database, &account, liked_songs.as_str())
                        .await?;
                    writeln!(output, "liked songs policy: {}", liked_songs.as_str())?;
                    writeln!(output, "account: {account}")?;
                    writeln!(output, "spotify_writes: disabled")?;
                    Ok(())
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
        Command::Intake { command } => {
            let database = connect_current_database().await?;
            let result: Result<()> = async {
                match command {
                    IntakeCommand::Audit { account } => {
                        let report = intake::audit(&database, &account).await?;
                        write_intake_audit(output, &report)
                    }
                    IntakeCommand::LikedDisposition {
                        account,
                        spotify_id,
                        disposition,
                        reason,
                    } => {
                        let disposition = intake::SavedTrackDisposition::from(disposition);
                        intake::set_saved_track_disposition(
                            &database,
                            &account,
                            &spotify_id,
                            disposition,
                            &reason,
                        )
                        .await?;
                        writeln!(output, "saved_track_disposition: {}", disposition.as_str())?;
                        writeln!(output, "spotify_id: {spotify_id}")?;
                        writeln!(output, "account: {account}")?;
                        writeln!(output, "spotify_writes: disabled")?;
                        Ok(())
                    }
                }
            }
            .await;
            database.close().await;
            result?;
        }
        Command::Sync { command } => match command {
            SyncCommand::Pull { account } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let total_started = Instant::now();
                    let progress = terminal::WorkflowProgress::new("Provider state", 4);
                    terminal::event("Sync", "reading Spotify and updating Neon");
                    let phase_started = Instant::now();
                    let import = spotify::import(&account, &database).await?;
                    let import_elapsed = phase_started.elapsed();
                    progress.complete("Provider state", &format_elapsed(import_elapsed));

                    progress.phase("Library analysis");
                    let phase_started = Instant::now();
                    let summary = if import.library_unchanged {
                        match analysis::reuse_current(&database, &account).await? {
                            Some(summary) => summary,
                            None => analysis::refresh(&database, &account).await?,
                        }
                    } else {
                        analysis::refresh(&database, &account).await?
                    };
                    let analysis_elapsed = phase_started.elapsed();
                    progress.complete("Library analysis", &format_elapsed(analysis_elapsed));

                    progress.phase("Listening history");
                    let phase_started = Instant::now();
                    let history = history::refresh_after_recent_import(&database, &account).await?;
                    let history_elapsed = phase_started.elapsed();
                    progress.complete("Listening history", &format_elapsed(history_elapsed));

                    progress.phase("Publication checks");
                    let phase_started = Instant::now();
                    let verified = apply::verify_pending_publications(
                        &database,
                        &account,
                        !import.library_unchanged,
                    )
                    .await?;
                    let verification_elapsed = phase_started.elapsed();
                    progress.complete("Publication checks", &format_elapsed(verification_elapsed));
                    progress.finish();
                    let timings = SyncPullTimings {
                        import: import_elapsed,
                        analysis: analysis_elapsed,
                        history: history_elapsed,
                        verification: verification_elapsed,
                        total: total_started.elapsed(),
                    };
                    let history = (history.archives != 0).then_some(&history);
                    if terminal::stdout_is_terminal() {
                        write_sync_pull_report(
                            output, &import, &summary, history, verified, &timings,
                        )?;
                    } else {
                        write_sync_pull_plain_report(
                            output, &import, &summary, history, verified, &timings,
                        )?;
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::AcceptCurrent { account } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let report = apply::accept_current_provider_state(&database, &account).await?;
                    writeln!(output, "provider_state: accepted")?;
                    writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
                    writeln!(
                        output,
                        "proposal_generation_id: {}",
                        report.proposal_generation_id
                    )?;
                    writeln!(output, "playlists: {}", report.playlist_count)?;
                    writeln!(output, "spotify_writes: disabled")?;
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
                    if !snapshot_current {
                        writeln!(
                            output,
                            "next: run `chordrift sync plan --account {account}` before readiness"
                        )?;
                    }
                    if details {
                        let annotations = sync_plan::maintenance_annotations(
                            &database,
                            &account,
                            &operations,
                        )
                        .await?;
                        writeln!(
                            output,
                            "sequence\tphase\toperation\tplaylist\tspotify_playlist_id\tspotify_track_id\tpayload\tsafety\ttrack\tartists\tmaintenance_interpretation\told_destination\tdestination"
                        )?;
                        for operation in operations {
                            let annotation = annotations.get(&operation.sequence).ok_or_else(|| {
                                ChordriftError::Configuration(format!(
                                    "plan operation {} has no maintenance annotation",
                                    operation.sequence
                                ))
                            })?;
                            writeln!(
                                output,
                                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                                operation.sequence,
                                operation.phase,
                                operation.operation_type,
                                clean_cell(&operation.playlist_name),
                                operation.spotify_playlist_id.as_deref().unwrap_or("-"),
                                operation.spotify_track_id.as_deref().unwrap_or("-"),
                                clean_cell(&operation.payload.to_string()),
                                clean_cell(&operation.safety.to_string()),
                                clean_cell(annotation.title.as_deref().unwrap_or("-")),
                                clean_cell(annotation.artists.as_deref().unwrap_or("-")),
                                annotation.interpretation,
                                clean_cell(annotation.old_destination.as_deref().unwrap_or("-")),
                                clean_cell(annotation.destination.as_deref().unwrap_or("-"))
                            )?;
                        }
                    }
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::Readiness {
                account,
                plan,
                probe,
            } => {
                let provider_status = if probe {
                    Some(spotify::status(&account).await?)
                } else {
                    None
                };
                let database = connect_current_database().await?;
                let result = async {
                    let report = apply_readiness::assess(
                        &database,
                        &account,
                        plan,
                        provider_status.as_ref(),
                    )
                    .await?;
                    write_readiness_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::ReadinessShow {
                account,
                assessment,
            } => {
                let database = connect_current_database().await?;
                let result = async {
                    let report = apply_readiness::show(&database, &account, assessment).await?;
                    write_readiness_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::Apply {
                account,
                assessment,
                phase,
                confirm,
                allow_destructive,
            } => {
                let database = connect_current_database().await?;
                let result = async {
                    let report = apply::execute(
                        &database,
                        &account,
                        assessment,
                        phase.into(),
                        confirm,
                        allow_destructive,
                    )
                    .await?;
                    write_apply_report(output, &report)
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::ApplyPreflight { account, plan } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let report = apply::preflight_publish(&database, &account, plan).await?;
                    writeln!(output, "publish preflight: passed")?;
                    writeln!(output, "plan_id: {}", report.plan_id)?;
                    writeln!(output, "playlist_creates: {}", report.playlist_creates)?;
                    writeln!(
                        output,
                        "populated_playlists: {}",
                        report.populated_playlists
                    )?;
                    writeln!(output, "playlist_entries: {}", report.playlist_entries)?;
                    writeln!(
                        output,
                        "playlist_item_writes: {}",
                        report.playlist_item_writes
                    )?;
                    writeln!(output, "artwork_uploads: {}", report.artwork_uploads)?;
                    writeln!(
                        output,
                        "largest_artwork_bytes: {}",
                        report.largest_artwork_bytes
                    )?;
                    writeln!(
                        output,
                        "estimated_spotify_reads: {}",
                        report.estimated_spotify_reads
                    )?;
                    writeln!(
                        output,
                        "estimated_spotify_writes: {}",
                        report.estimated_spotify_writes
                    )?;
                    writeln!(output, "spotify_requests_made: 0")?;
                    writeln!(output, "spotify_writes: disabled")?;
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::RetirementApprove {
                account,
                plan,
                confirm,
            } => {
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let approval =
                        apply::approve_retirement(&database, &account, plan, confirm).await?;
                    writeln!(output, "retirement approval: recorded")?;
                    writeln!(output, "plan_id: {}", approval.plan_id)?;
                    writeln!(output, "operations: {}", approval.operation_count)?;
                    writeln!(output, "approved_at: {}", approval.approved_at.to_rfc3339())?;
                    writeln!(output, "spotify_writes: disabled")?;
                    Ok(())
                }
                .await;
                database.close().await;
                result?;
            }
            SyncCommand::ApplyShow { account, run } => {
                let database = connect_current_database().await?;
                let result = async {
                    let report = apply::show(&database, &account, run).await?;
                    write_apply_report(output, &report)
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
        Command::Albums { command } => {
            let database = connect_current_database().await?;
            let result = run_album_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Reevaluate { command } => {
            let database = connect_current_database().await?;
            let result = run_reevaluate_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Routes { command } => {
            let database = connect_current_database().await?;
            let result = run_route_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Tracks { command } => {
            let database = connect_current_database().await?;
            let result = run_track_command(command, output, &database).await;
            database.close().await;
            result?;
        }
        Command::Classify { command } => {
            let database = connect_current_database().await?;
            let result = run_classification_command(command, output, &database).await;
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
        Command::Artwork {
            command:
                ArtworkCommand::Render {
                    background,
                    title,
                    output: path,
                },
        } => {
            let report = artwork::render_label(&background, &path, &title)?;
            writeln!(output, "artwork: rendered")?;
            writeln!(output, "path: {}", report.path.display())?;
            writeln!(output, "dimensions: {}x{}", report.width, report.height)?;
            writeln!(output, "bytes: {}", report.byte_size)?;
            writeln!(output, "sha256: {}", report.sha256)?;
            writeln!(output, "spotify_writes: disabled")?;
        }
        Command::Artwork { command } => {
            let database = connect_current_database().await?;
            let result = run_artwork_command(command, output, &database).await;
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

fn service_session_id(profile: &str) -> Result<SecretId> {
    SecretId::new("chordrift-service", profile, "product-session")
}

fn load_service_client(url: &str, profile: &str) -> Result<RemoteHttpClient> {
    let token = SystemCredentialStore
        .load(&service_session_id(profile)?)?
        .ok_or_else(|| {
            ChordriftError::Configuration(format!(
                "no Chordrift service session is stored for profile '{profile}'"
            ))
        })?;
    let token = String::from_utf8(token).map_err(|_| {
        ChordriftError::Configuration("stored Chordrift service session is invalid".to_owned())
    })?;
    RemoteHttpClient::new(url, token)
        .map_err(|error| ChordriftError::Configuration(error.to_string()))
}

fn service_compatibility_offer() -> ClientCompatibility {
    ClientCompatibility {
        contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
        schema_versions: SchemaVersionRange::new(0, 51).expect("CLI schema range is valid"),
        requested_features: vec![
            CAPABILITY_MAINTENANCE_TASK_SESSION.to_owned(),
            CAPABILITY_PRODUCT_IDENTITY.to_owned(),
            CAPABILITY_DURABLE_OPERATIONS.to_owned(),
            CAPABILITY_REMOTE_CLI.to_owned(),
        ],
    }
}

async fn negotiated_service_client(url: &str, profile: &str) -> Result<RemoteHttpClient> {
    let client = load_service_client(url, profile)?;
    client
        .negotiate(service_compatibility_offer())
        .await
        .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
    Ok(client)
}

async fn run_service_command(command: ServiceCommand, output: &mut impl Write) -> Result<()> {
    match command {
        ServiceCommand::AdoptLocalProviderCredential { account } => {
            let (provider_identity, refresh) = spotify::local_refresh_credential(&account)?;
            let database = connect_current_database().await?;
            let result: Result<()> = async {
                db::require_schema_through(&database, 50).await?;
                let row = sqlx::query(
                    "SELECT provider.id AS provider_account_id,
                            provider.chordrift_account_id,
                            membership.product_subject_id
                       FROM provider_accounts provider
                       JOIN chordrift_account_memberships membership
                         ON membership.chordrift_account_id = provider.chordrift_account_id
                        AND membership.role = 'owner' AND membership.status = 'active'
                       JOIN product_subjects subject
                         ON subject.id = membership.product_subject_id AND subject.status = 'active'
                      WHERE provider.provider = 'spotify'
                        AND provider.account_label = $1
                        AND provider.provider_account_id = $2",
                )
                .bind(&account)
                .bind(&provider_identity)
                .fetch_optional(database.pool())
                .await?
                .ok_or_else(|| {
                    ChordriftError::Configuration(
                        "local Spotify authorization does not match the adopted Chordrift account"
                            .to_owned(),
                    )
                })?;
                let provider_account_id: uuid::Uuid = row.try_get("provider_account_id")?;
                let chordrift_account_id: uuid::Uuid = row.try_get("chordrift_account_id")?;
                let product_subject_id: uuid::Uuid = row.try_get("product_subject_id")?;
                let subject = AuthenticatedSubject {
                    subject_id: ResourceId::from_uuid(product_subject_id),
                    account_id: ResourceId::from_uuid(chordrift_account_id),
                };
                let identity = ProviderCredentialIdentity::new(
                    subject.account_id,
                    ResourceId::from_uuid(provider_account_id),
                    "spotify",
                )
                .map_err(|_| {
                    ChordriftError::Configuration(
                        "provider credential identity is invalid".to_owned(),
                    )
                })?;
                let keyring = ProviderVaultKeyring::from_environment().map_err(|_| {
                    ChordriftError::Configuration(
                        "provider vault key configuration is unavailable".to_owned(),
                    )
                })?;
                let vault = ProviderCredentialVault::new(
                    PostgresProviderCredentialStore::new(database.pool().clone()),
                    keyring,
                );
                let revision = vault
                    .rotate(subject, identity, &refresh, chrono::Utc::now())
                    .await
                    .map_err(|_| {
                        ChordriftError::Configuration(
                            "provider credential adoption failed closed".to_owned(),
                        )
                    })?;
                writeln!(output, "provider: spotify")?;
                writeln!(output, "account: {account}")?;
                writeln!(output, "vault_generation: {}", revision.generation)?;
                writeln!(output, "credential_encrypted: true")?;
                writeln!(output, "provider_contacted: false")?;
                Ok(())
            }
            .await;
            database.close().await;
            result?;
        }
        ServiceCommand::Session { command } => match command {
            ServiceSessionCommand::Save { profile } => {
                let mut token = Zeroizing::new(String::new());
                io::stdin().take(4097).read_to_string(&mut token)?;
                if token.len() > 4096 {
                    return Err(ChordriftError::Configuration(
                        "Chordrift service session exceeds the accepted size".to_owned(),
                    ));
                }
                let token = token.trim();
                if !token.starts_with("chd_session_") || token.len() <= "chd_session_".len() {
                    return Err(ChordriftError::Configuration(
                        "standard input is not an opaque Chordrift session".to_owned(),
                    ));
                }
                SystemCredentialStore.save(&service_session_id(&profile)?, token.as_bytes())?;
                writeln!(output, "stored: true\nprofile: {profile}")?;
            }
            ServiceSessionCommand::Status { profile } => {
                let stored = SystemCredentialStore
                    .load(&service_session_id(&profile)?)?
                    .is_some();
                writeln!(output, "stored: {stored}\nprofile: {profile}")?;
            }
            ServiceSessionCommand::Remove { profile } => {
                let removed = SystemCredentialStore.delete(&service_session_id(&profile)?)?;
                writeln!(output, "removed: {removed}\nprofile: {profile}")?;
            }
        },
        ServiceCommand::Maintenance { command } => match command {
            ServiceMaintenanceCommand::Start {
                url,
                profile,
                provider_connection_id,
                session_id,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let session_id =
                    MaintenanceSessionId::from_uuid(session_id.unwrap_or_else(uuid::Uuid::new_v4));
                let receipt = client
                    .command(CommandRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        idempotency_key: IdempotencyKey::new(),
                        command: ContractCommand::StartMaintenance {
                            session_id,
                            provider_connection_id: ResourceId::from_uuid(provider_connection_id),
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&receipt)?)?;
            }
            ServiceMaintenanceCommand::Show {
                url,
                profile,
                session_id,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let response = client
                    .query(QueryRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        query: Query::MaintenanceSession {
                            session_id: MaintenanceSessionId::from_uuid(session_id),
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&response)?)?;
            }
            ServiceMaintenanceCommand::Refresh {
                url,
                profile,
                session_id,
                expected_revision,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let receipt = client
                    .command(CommandRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        idempotency_key: IdempotencyKey::new(),
                        command: ContractCommand::RefreshMaintenance {
                            session_id: MaintenanceSessionId::from_uuid(session_id),
                            expected_revision,
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&receipt)?)?;
            }
            ServiceMaintenanceCommand::Resolve {
                url,
                profile,
                session_id,
                expected_revision,
                decisions,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let decisions: Vec<MaintenanceDecision> =
                    serde_json::from_slice(&std::fs::read(decisions)?)?;
                let receipt = client
                    .command(CommandRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        idempotency_key: IdempotencyKey::new(),
                        command: ContractCommand::ResolveMaintenance {
                            session_id: MaintenanceSessionId::from_uuid(session_id),
                            expected_revision,
                            decisions,
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&receipt)?)?;
            }
            ServiceMaintenanceCommand::Authorize {
                url,
                profile,
                session_id,
                expected_revision,
                review_id,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let receipt = client
                    .command(CommandRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        idempotency_key: IdempotencyKey::new(),
                        command: ContractCommand::AuthorizeMaintenance {
                            session_id: MaintenanceSessionId::from_uuid(session_id),
                            expected_revision,
                            review_id: MaintenanceReviewId::from_uuid(review_id),
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&receipt)?)?;
            }
        },
        ServiceCommand::Library { command } => match command {
            ServiceLibraryCommand::Compare {
                url,
                profile,
                provider_connection_id,
            } => {
                let client = negotiated_service_client(&url, &profile).await?;
                let response = client
                    .query(QueryRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        query: Query::LibraryComparison {
                            provider_connection_id: ResourceId::from_uuid(provider_connection_id),
                        },
                    })
                    .await
                    .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
                writeln!(output, "{}", serde_json::to_string(&response)?)?;
            }
        },
        ServiceCommand::Compatibility { url, profile } => {
            let client = load_service_client(&url, &profile)?;
            let negotiated = client
                .negotiate(service_compatibility_offer())
                .await
                .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
            writeln!(output, "{}", serde_json::to_string(&negotiated)?)?;
        }
        ServiceCommand::Command { url, profile, file } => {
            let client = load_service_client(&url, &profile)?;
            client
                .negotiate(service_compatibility_offer())
                .await
                .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
            let request: CommandRequest = serde_json::from_slice(&std::fs::read(file)?)?;
            let receipt = client
                .command(request)
                .await
                .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
            writeln!(output, "{}", serde_json::to_string(&receipt)?)?;
        }
        ServiceCommand::Query { url, profile, file } => {
            let client = load_service_client(&url, &profile)?;
            client
                .negotiate(service_compatibility_offer())
                .await
                .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
            let request: QueryRequest = serde_json::from_slice(&std::fs::read(file)?)?;
            let response = client
                .query(request)
                .await
                .map_err(|error| ChordriftError::Configuration(error.to_string()))?;
            writeln!(output, "{}", serde_json::to_string(&response)?)?;
        }
    }
    Ok(())
}

async fn run_reevaluate_command(
    command: ReevaluateCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        ReevaluateCommand::Create {
            account,
            description,
            background,
            artwork,
        } => {
            let queue =
                routes::create_reevaluate(database, &account, &description, &background, &artwork)
                    .await?;
            writeln!(output, "queue: {}", queue.name)?;
            writeln!(output, "stable_key: {}", queue.stable_key)?;
            writeln!(output, "playlist_id: {}", queue.playlist_id)?;
            writeln!(output, "artwork_sha256: {}", queue.artwork_sha256)?;
            writeln!(
                output,
                "spotify_playlist_id: {}",
                queue.spotify_playlist_id.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ReevaluateCommand::Status { account } => {
            let queue = routes::reevaluate(database, &account).await?;
            let observed_tracks = if queue.spotify_playlist_id.is_some() {
                playlists::tracks(
                    database,
                    &account,
                    &playlists::PlaylistSelector::Name(queue.name.clone()),
                )
                .await?
                .tracks
                .len()
            } else {
                usize::try_from(queue.track_count).map_err(|_| {
                    ChordriftError::Configuration(
                        "Re-evaluate desired membership count is invalid".to_owned(),
                    )
                })?
            };
            writeln!(output, "queue: {}", queue.name)?;
            writeln!(output, "active: {}", queue.active)?;
            writeln!(output, "tracks: {observed_tracks}")?;
            writeln!(
                output,
                "spotify_playlist_id: {}",
                queue.spotify_playlist_id.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "description: {}", clean_cell(&queue.description))?;
            Ok(())
        }
        ReevaluateCommand::Export { account, file } => {
            let queue = routes::reevaluate(database, &account).await?;
            let report = classifications::export(database, &account, &[queue.name], &file).await?;
            writeln!(output, "file: {}", report.path)?;
            writeln!(output, "tracks: {}", report.tracks)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ReevaluateCommand::RetireLegacy { account, confirm } => {
            let report = routes::retire_legacy(database, &account, &confirm).await?;
            writeln!(output, "legacy_routes_retired: {}", report.routes)?;
            writeln!(output, "covered_tracks: {}", report.tracks)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ReevaluateCommand::Retire { account, confirm } => {
            let queue = routes::retire_reevaluate(database, &account, &confirm).await?;
            writeln!(output, "queue: {}", queue.name)?;
            writeln!(output, "active: {}", queue.active)?;
            writeln!(output, "tracks: {}", queue.track_count)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
    }
}

fn write_readiness_report(
    output: &mut impl Write,
    report: &apply_readiness::ReadinessReport,
) -> Result<()> {
    writeln!(
        output,
        "apply_readiness: {}{}",
        report.status,
        if report.reused {
            " (already current)"
        } else {
            ""
        }
    )?;
    writeln!(output, "assessment_id: {}", report.assessment_id)?;
    writeln!(output, "plan_id: {}", report.plan_id)?;
    writeln!(output, "operations: {}", report.operation_count)?;
    writeln!(
        output,
        "checks: {}/{} passed",
        report.passed_checks, report.check_count
    )?;
    writeln!(
        output,
        "restart_checkpoints: {}",
        report.restart_checkpoints
    )?;
    writeln!(output, "replay_changes: {}", report.replay_changes)?;
    writeln!(
        output,
        "provider_probe_performed: {}",
        report.provider_probe_performed
    )?;
    writeln!(output, "input_hash: {}", report.input_hash)?;
    writeln!(output, "created_at: {}", report.created_at.to_rfc3339())?;
    writeln!(output, "status\tcheck\tevidence")?;
    for check in &report.checks {
        writeln!(
            output,
            "{}\t{}\t{}",
            if check.passed { "passed" } else { "blocked" },
            check.name,
            clean_cell(&check.evidence.to_string())
        )?;
    }
    writeln!(output, "spotify_writes: disabled")?;
    Ok(())
}

async fn run_artwork_command(
    command: ArtworkCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        ArtworkCommand::Render {
            background,
            title,
            output: path,
        } => {
            let report = artwork::render_label(&background, &path, &title)?;
            writeln!(output, "artwork: rendered")?;
            writeln!(output, "path: {}", report.path.display())?;
            writeln!(output, "dimensions: {}x{}", report.width, report.height)?;
            writeln!(output, "bytes: {}", report.byte_size)?;
            writeln!(output, "sha256: {}", report.sha256)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ArtworkCommand::Import { account, manifest } => {
            let report = artwork::import(database, &account, &manifest).await?;
            writeln!(
                output,
                "artwork: {}",
                if report.reused {
                    "already current"
                } else {
                    "verified"
                }
            )?;
            writeln!(output, "batch_id: {}", report.batch_id)?;
            writeln!(
                output,
                "proposal_generation_id: {}",
                report.proposal_generation_id
            )?;
            writeln!(output, "state: {}", report.state)?;
            writeln!(output, "artifacts: {}", report.artifact_count)?;
            writeln!(output, "input_hash: {}", report.input_hash)?;
            writeln!(output, "contact_sheet: {}", report.contact_sheet_path)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ArtworkCommand::Status { account } => {
            let status = artwork::status(database, &account).await?;
            write_artwork_status(output, &status)
        }
        ArtworkCommand::List { account } => {
            let rows = artwork::list(database, &account).await?;
            writeln!(
                output,
                "dimensions\tbytes\ttarget\tname\tstable_key\tsha256\tpath"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}x{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.width,
                    row.height,
                    row.byte_size,
                    row.target_kind,
                    clean_cell(&row.name),
                    row.stable_key,
                    row.sha256,
                    clean_cell(&row.path)
                )?;
            }
            Ok(())
        }
        ArtworkCommand::Approve { account, confirm } => {
            let status = artwork::approve(database, &account, confirm).await?;
            write_artwork_status(output, &status)
        }
        ArtworkCommand::Update { account, playlist } => {
            let report = sync_plan::create_artwork_update(database, &account, &playlist).await?;
            write_sync_plan_report(output, &report)
        }
    }
}

fn write_artwork_status(output: &mut impl Write, status: &artwork::Status) -> Result<()> {
    writeln!(output, "artwork: {}", status.state)?;
    writeln!(output, "batch_id: {}", status.batch_id)?;
    writeln!(
        output,
        "proposal_generation_id: {}",
        status.proposal_generation_id
    )?;
    writeln!(output, "visual_system: {}", status.visual_system)?;
    writeln!(output, "generator: {}", status.generator)?;
    writeln!(output, "artifacts: {}", status.artifact_count)?;
    writeln!(output, "input_hash: {}", status.input_hash)?;
    writeln!(output, "contact_sheet: {}", status.contact_sheet_path)?;
    writeln!(
        output,
        "approved_at: {}",
        status
            .approved_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(output, "created_at: {}", status.created_at.to_rfc3339())?;
    writeln!(output, "spotify_writes: disabled")?;
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
    if let Err(error) = db::require_schema_through(&database, 47).await {
        database.close().await;
        return Err(error);
    }
    Ok(database)
}

async fn run_product_command(command: ProductCommand, output: &mut impl Write) -> Result<()> {
    use crate::contract::{
        CONTRACT_VERSION, Command as ContractCommand, CommandRequest, IdempotencyKey, Query,
        QueryRequest, RequestId, ResourceId,
    };
    use crate::domain::ChordriftAccountId;

    match command {
        ProductCommand::Onboarding { command } => {
            require_product_rehearsal_opt_in()?;
            let fixture_path = match &command {
                ProductOnboardingCommand::Capture { fixture, .. }
                | ProductOnboardingCommand::Audit { fixture, .. } => fixture,
            };
            let fixture: product_rehearsal::OnboardingRehearsalFixture =
                read_product_fixture(fixture_path)?;
            let database = connect_current_database().await?;
            let result: Result<()> = async {
                match command {
                    ProductOnboardingCommand::Capture {
                        mode,
                        idempotency_key,
                        ..
                    } => {
                        let include_extended_history = mode == ProductAuditMode::Enriched;
                        let request = CommandRequest {
                            contract_version: CONTRACT_VERSION,
                            request_id: RequestId::new(),
                            idempotency_key: idempotency_key
                                .map_or_else(IdempotencyKey::new, IdempotencyKey::from_uuid),
                            command: ContractCommand::CreateOnboardingSession {
                                account_id: ResourceId::from_uuid(
                                    fixture.context.account_id().as_uuid(),
                                ),
                                include_extended_history,
                            },
                        };
                        let boundary = onboarding::OnboardingSessionBoundary::new(&database);
                        let session = ApplicationFacade::new()
                            .invoke(boundary.invocation(&fixture.context, &request, &fixture))
                            .await?
                            .map_err(product_boundary_error)?;
                        write_product_header(output, "onboarding_session")?;
                        writeln!(output, "session_id: {}", session.id)?;
                        writeln!(output, "mode: {}", audit_mode_name(mode))?;
                        writeln!(output, "input_fingerprint: {}", session.input_fingerprint)?;
                        write_product_json(output, &session)
                    }
                    ProductOnboardingCommand::Audit { session, mode, .. } => {
                        let request = QueryRequest {
                            contract_version: CONTRACT_VERSION,
                            request_id: RequestId::new(),
                            query: Query::OnboardingAudit {
                                session_id: ResourceId::from_uuid(session),
                            },
                        };
                        match mode {
                            ProductAuditMode::InventoryOnly => {
                                let boundary =
                                    onboarding_audit::InventoryOnlyAuditBoundary::new(&database);
                                let view = ApplicationFacade::new()
                                    .invoke(boundary.invocation(&fixture.context, &request))
                                    .await?
                                    .map_err(product_boundary_error)?;
                                write_product_header(output, "onboarding_audit")?;
                                writeln!(output, "session_id: {session}")?;
                                writeln!(output, "mode: inventory_only")?;
                                writeln!(
                                    output,
                                    "audit_fingerprint: {}",
                                    view.value.audit_fingerprint
                                )?;
                                writeln!(
                                    output,
                                    "inventory_findings_fingerprint: {}",
                                    onboarding_audit::inventory_findings_fingerprint(&view.value)
                                        .map_err(product_boundary_error)?
                                )?;
                                writeln!(output, "strengthened_conclusions: 0")?;
                                write_product_json(output, &view)
                            }
                            ProductAuditMode::Enriched => {
                                let boundary =
                                    onboarding_audit::EnrichedAuditBoundary::new(&database);
                                let view = ApplicationFacade::new()
                                    .invoke(boundary.invocation(&fixture.context, &request))
                                    .await?
                                    .map_err(product_boundary_error)?;
                                write_product_header(output, "onboarding_audit")?;
                                writeln!(output, "session_id: {session}")?;
                                writeln!(output, "mode: enriched")?;
                                writeln!(
                                    output,
                                    "audit_fingerprint: {}",
                                    view.value.audit_fingerprint
                                )?;
                                writeln!(
                                    output,
                                    "inventory_findings_fingerprint: {}",
                                    onboarding_audit::inventory_findings_fingerprint(
                                        &view.value.inventory_baseline,
                                    )
                                    .map_err(product_boundary_error)?
                                )?;
                                writeln!(
                                    output,
                                    "strengthened_conclusions: {}",
                                    view.value.strengthened_conclusions.len()
                                )?;
                                write_product_json(output, &view)
                            }
                        }
                    }
                }
            }
            .await;
            database.close().await;
            result?;
        }
        ProductCommand::Collections {
            command: ProductCollectionCommand::List { account },
        } => {
            require_product_rehearsal_opt_in()?;
            let database = connect_current_database().await?;
            let result: Result<()> = async {
                let account_id = ChordriftAccountId::from_uuid(account);
                let request = QueryRequest {
                    contract_version: CONTRACT_VERSION,
                    request_id: RequestId::new(),
                    query: Query::Collections {
                        account_id: ResourceId::from_uuid(account),
                    },
                };
                let boundary = product_rehearsal::CollectionReviewBoundary::new(&database);
                let view = ApplicationFacade::new()
                    .invoke(boundary.invocation(account_id, &request))
                    .await??;
                write_product_header(output, "collections")?;
                writeln!(output, "account_id: {account}")?;
                writeln!(output, "collections: {}", view.value.collections.len())?;
                write_product_json(output, &view)
            }
            .await;
            database.close().await;
            result?;
        }
        ProductCommand::Recipes { command } => match command {
            ProductRecipeCommand::Show { account, revision } => {
                require_product_rehearsal_opt_in()?;
                let database = connect_current_database().await?;
                let result: Result<()> = async {
                    let request = QueryRequest {
                        contract_version: CONTRACT_VERSION,
                        request_id: RequestId::new(),
                        query: Query::Recipe {
                            recipe_revision_id: ResourceId::from_uuid(revision),
                        },
                    };
                    let boundary = product_rehearsal::RecipeReviewBoundary::new(&database);
                    let view = ApplicationFacade::new()
                        .invoke(
                            boundary.invocation(ChordriftAccountId::from_uuid(account), &request),
                        )
                        .await??;
                    write_product_header(output, "recipe_revision")?;
                    writeln!(output, "account_id: {account}")?;
                    writeln!(output, "recipe_revision_id: {revision}")?;
                    write_product_json(output, &view)
                }
                .await;
                database.close().await;
                result?;
            }
            ProductRecipeCommand::Execute { fixture } => {
                let fixture: product_rehearsal::SpinRehearsalFixture =
                    read_product_fixture(&fixture)?;
                let executor = recipe_execution::RecipeExecutor::new();
                let draft = ApplicationFacade::new()
                    .invoke(executor.invocation(&fixture.recipe_execution))
                    .await?
                    .map_err(product_boundary_error)?;
                if draft.recipe_revision.recipe_id.account_id() != fixture.account_id {
                    return Err(product_boundary_error(
                        "Spin fixture account does not own its recipe execution",
                    ));
                }
                write_product_header(output, "recipe_execution")?;
                writeln!(output, "account_id: {}", fixture.account_id)?;
                writeln!(
                    output,
                    "draft_fingerprint: {}",
                    draft.draft_fingerprint.as_str()
                )?;
                writeln!(output, "selected_tracks: {}", draft.selections.len())?;
                writeln!(output, "unfilled_seats: {}", draft.unfilled_seats)?;
                write_product_json(output, &draft)?;
            }
        },
        ProductCommand::Spins { command } => {
            require_product_rehearsal_opt_in()?;
            let database = connect_current_database().await?;
            let result: Result<()> = async {
                match command {
                    ProductSpinCommand::Preview { fixture } => {
                        let fixture: product_rehearsal::SpinRehearsalFixture =
                            read_product_fixture(&fixture)?;
                        let executor = recipe_execution::RecipeExecutor::new();
                        let draft = ApplicationFacade::new()
                            .invoke(executor.invocation(&fixture.recipe_execution))
                            .await?
                            .map_err(product_boundary_error)?;
                        let recipe_revision = draft.recipe_revision.revision_id.as_uuid();
                        let request = CommandRequest {
                            contract_version: CONTRACT_VERSION,
                            request_id: RequestId::new(),
                            idempotency_key: IdempotencyKey::new(),
                            command: ContractCommand::PreviewSpin {
                                recipe_revision_id: ResourceId::from_uuid(recipe_revision),
                            },
                        };
                        let input = spin_preview::SpinPreviewInput {
                            draft,
                            capability_snapshot: fixture.capability_snapshot,
                            seed: fixture.seed,
                        };
                        let boundary = spin_preview::SpinPreviewBoundary::new(&database);
                        let preview = ApplicationFacade::new()
                            .invoke(boundary.create_invocation(
                                fixture.account_id,
                                &request,
                                &input,
                            ))
                            .await?
                            .map_err(product_boundary_error)?;
                        write_product_header(output, "spin_preview")?;
                        writeln!(
                            output,
                            "spin_id: {}",
                            preview.identity.spin_id().into_resource_id()
                        )?;
                        writeln!(
                            output,
                            "preview_fingerprint: {}",
                            preview.preview_fingerprint
                        )?;
                        writeln!(output, "tracks: {}", preview.tracks.len())?;
                        write_product_json(output, &preview)
                    }
                    ProductSpinCommand::Show { account, spin } => {
                        let request = QueryRequest {
                            contract_version: CONTRACT_VERSION,
                            request_id: RequestId::new(),
                            query: Query::SpinPreview {
                                spin_id: ResourceId::from_uuid(spin),
                            },
                        };
                        let boundary = spin_preview::SpinPreviewBoundary::new(&database);
                        let view =
                            ApplicationFacade::new()
                                .invoke(boundary.read_invocation(
                                    ChordriftAccountId::from_uuid(account),
                                    &request,
                                ))
                                .await?
                                .map_err(product_boundary_error)?;
                        write_product_header(output, "spin_preview")?;
                        writeln!(output, "spin_id: {spin}")?;
                        writeln!(
                            output,
                            "preview_fingerprint: {}",
                            view.value.preview_fingerprint
                        )?;
                        writeln!(output, "tracks: {}", view.value.tracks.len())?;
                        write_product_json(output, &view)
                    }
                }
            }
            .await;
            database.close().await;
            result?;
        }
    }
    Ok(())
}

fn require_product_rehearsal_opt_in() -> Result<()> {
    if std::env::var("CHORDRIFT_PRODUCT_REHEARSAL").as_deref() == Ok("1") {
        Ok(())
    } else {
        Err(ChordriftError::Configuration(
            "v0.2 product commands require CHORDRIFT_PRODUCT_REHEARSAL=1 and an isolated migration-0046 database"
                .to_owned(),
        ))
    }
}

fn read_product_fixture<T>(path: &std::path::Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(ChordriftError::from)
}

fn write_product_header(output: &mut impl Write, product: &str) -> Result<()> {
    writeln!(output, "product_view: {product}")?;
    writeln!(
        output,
        "contract_version: {}",
        crate::contract::CONTRACT_VERSION
    )?;
    writeln!(output, "provider_writes: disabled")?;
    Ok(())
}

fn write_product_json(output: &mut impl Write, value: &impl serde::Serialize) -> Result<()> {
    writeln!(output, "value_json: {}", serde_json::to_string(value)?)?;
    Ok(())
}

fn audit_mode_name(mode: ProductAuditMode) -> &'static str {
    match mode {
        ProductAuditMode::InventoryOnly => "inventory_only",
        ProductAuditMode::Enriched => "enriched",
    }
}

fn product_boundary_error(error: impl std::fmt::Display) -> ChordriftError {
    ChordriftError::Configuration(error.to_string())
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

async fn run_album_command(
    command: AlbumCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        AlbumCommand::List { account } => {
            let rows = albums::list(database, &account).await?;
            if terminal::stdout_is_terminal() {
                let rendered = terminal::pretty_table(
                    &["Review", "Tracks", "Album", "Spotify ID"],
                    rows.into_iter()
                        .map(|row| {
                            vec![
                                if row.pending == 0 {
                                    "complete".to_owned()
                                } else {
                                    format!("{} pending", row.pending)
                                },
                                format!(
                                    "{} total\n{} preserved · {} excluded",
                                    row.tracks, row.preserved, row.excluded
                                ),
                                format!(
                                    "{}\n{}",
                                    row.title,
                                    row.artist.unwrap_or_else(|| "—".to_owned())
                                ),
                                row.spotify_id,
                            ]
                        })
                        .collect(),
                );
                writeln!(output, "\x1b[1;36mSaved albums · {account}\x1b[0m")?;
                writeln!(output, "{rendered}")?;
                return Ok(());
            }
            writeln!(
                output,
                "tracks\tpreserved\texcluded\tpending\tsaved_at\tartist\talbum\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.tracks,
                    row.preserved,
                    row.excluded,
                    row.pending,
                    row.saved_at
                        .map_or_else(|| "-".to_owned(), |v| v.to_rfc3339()),
                    clean_cell(row.artist.as_deref().unwrap_or("-")),
                    clean_cell(&row.title),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        AlbumCommand::Audit { account } => {
            let audit = albums::audit(database, &account).await?;
            writeln!(output, "saved album audit: current")?;
            writeln!(output, "snapshot_id: {}", audit.snapshot_id)?;
            writeln!(output, "policy: {}", audit.policy)?;
            writeln!(output, "albums: {}", audit.albums)?;
            writeln!(output, "unique_tracks: {}", audit.unique_tracks)?;
            writeln!(output, "preserved: {}", audit.preserved)?;
            writeln!(output, "excluded: {}", audit.excluded)?;
            writeln!(output, "pending_review: {}", audit.pending)?;
            writeln!(
                output,
                "review_complete_albums: {}",
                audit.review_complete_albums
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        AlbumCommand::History { account } => {
            let rows = albums::history(database, &account).await?;
            if terminal::stdout_is_terminal() {
                let rendered = terminal::pretty_table(
                    &["State", "Tracks", "Album", "Last saved"],
                    rows.into_iter()
                        .map(|row| {
                            vec![
                                row.state,
                                row.tracks.to_string(),
                                format!(
                                    "{}\n{}",
                                    row.title,
                                    row.artist.unwrap_or_else(|| "—".to_owned())
                                ),
                                row.last_saved_at
                                    .map_or_else(|| "—".to_owned(), |v| v.to_rfc3339()),
                            ]
                        })
                        .collect(),
                );
                writeln!(output, "\x1b[1;36mSaved-album history · {account}\x1b[0m")?;
                writeln!(output, "{rendered}")?;
                return Ok(());
            }
            writeln!(
                output,
                "state\ttracks\tfirst_saved_at\tlast_saved_at\tartist\talbum\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.state,
                    row.tracks,
                    row.first_saved_at
                        .map_or_else(|| "-".to_owned(), |v| v.to_rfc3339()),
                    row.last_saved_at
                        .map_or_else(|| "-".to_owned(), |v| v.to_rfc3339()),
                    clean_cell(row.artist.as_deref().unwrap_or("-")),
                    clean_cell(&row.title),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        AlbumCommand::Tracks {
            account,
            name,
            spotify_id,
        } => {
            let rows =
                albums::tracks(database, &account, name.as_deref(), spotify_id.as_deref()).await?;
            writeln!(output, "position\tdisposition\ttrack\tartists\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}",
                    row.position + 1,
                    row.disposition,
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        AlbumCommand::Policy { account, mode } => {
            albums::set_policy(database, &account, mode.as_str()).await?;
            writeln!(output, "saved album policy: {}", mode.as_str())?;
            writeln!(output, "account: {account}")?;
            writeln!(output, "spotify_writes: disabled")?;
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
            if terminal::stdout_is_terminal() {
                let rendered = terminal::pretty_table(
                    &["State", "Semantics", "Tracks", "Playlist"],
                    rows.into_iter()
                        .map(|row| {
                            let authority = match row.drift_policy.as_str() {
                                "neon_wins" => "Neon owns",
                                "provider_wins" => "Spotify owns",
                                "manual" => "manual review",
                                other => other,
                            };
                            let mut semantics = format!(
                                "{} · {}",
                                row.signal_class,
                                row.behavioral_signal.unwrap_or_else(|| "—".to_owned())
                            );
                            if row.clear_policy == "after_verified_assignment" {
                                semantics.push_str("\nclears after assignment");
                            }
                            if row.semantic_weight != 0.0 {
                                semantics
                                    .push_str(&format!("\nweight: {:.2}", row.semantic_weight));
                            }
                            vec![
                                format!(
                                    "{} · {}\n{}",
                                    row.role,
                                    if row.present { "live" } else { "absent" },
                                    authority
                                ),
                                semantics,
                                row.total_items
                                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
                                format!("{}\n{}", row.name, row.provider_playlist_id),
                            ]
                        })
                        .collect(),
                );
                writeln!(output, "\x1b[1;36mPlaylists · {account}\x1b[0m")?;
                writeln!(output, "{rendered}")?;
                return Ok(());
            }
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
            if terminal::stdout_is_terminal() {
                writeln!(
                    output,
                    "\x1b[1;36m{}\x1b[0m  \x1b[2m{} tracks · {}\x1b[0m",
                    report.playlist.name,
                    report.tracks.len(),
                    report.playlist.provider_playlist_id
                )?;
                let rendered = terminal::pretty_table(
                    &["#", "Track", "Spotify ID"],
                    report
                        .tracks
                        .into_iter()
                        .map(|row| {
                            vec![
                                (row.position + 1).to_string(),
                                format!(
                                    "{}\n{}\n{}",
                                    row.title,
                                    row.artists,
                                    row.album.unwrap_or_else(|| "—".to_owned())
                                ),
                                row.provider_track_id,
                            ]
                        })
                        .collect(),
                );
                writeln!(output, "{rendered}")?;
                return Ok(());
            }
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
        PlaylistCommand::Retirement {
            account,
            include,
            all,
            except,
            none,
        } => {
            let report =
                playlists::configure_retirement(database, &account, &include, all, &except, none)
                    .await?;
            writeln!(output, "retirement_policy: updated")?;
            writeln!(output, "changed: {}", report.changed)?;
            writeln!(
                output,
                "retirement_candidates: {}",
                report.retirement_candidates
            )?;
            writeln!(
                output,
                "protected_playlists: {}",
                report.protected_playlists
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
    }
}

async fn run_track_command(
    command: TrackCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        TrackCommand::Inspect {
            account,
            name,
            artist,
            spotify_id,
            technical,
        } => {
            let selector = match (name, spotify_id) {
                (Some(title), None) => tracks::TrackSelector::Name { title, artist },
                (None, Some(id)) => tracks::TrackSelector::SpotifyId(id),
                _ => unreachable!("clap enforces one track selector"),
            };
            let report = tracks::inspect(database, &account, &selector).await?;
            if terminal::stdout_is_terminal() {
                return write_pretty_track_inspection(output, &report, technical);
            }
            writeln!(
                output,
                "track: {} — {}",
                clean_cell(&report.title),
                clean_cell(&report.artists)
            )?;
            writeln!(output, "spotify_id: {}", report.spotify_id)?;
            writeln!(output, "canonical_id: {}", report.track_id)?;
            writeln!(
                output,
                "album: {}",
                clean_cell(report.album.as_deref().unwrap_or("-"))
            )?;
            writeln!(output, "isrc: {}", report.isrc.as_deref().unwrap_or("-"))?;
            writeln!(
                output,
                "duration_ms: {}",
                report
                    .duration_ms
                    .map_or_else(|| "-".to_owned(), |value| value.to_string())
            )?;
            writeln!(
                output,
                "current_playlists: {}",
                report.current_playlists.len()
            )?;
            for playlist in &report.current_playlists {
                writeln!(
                    output,
                    "  - {} (position {}, role {}, signal {})",
                    clean_cell(&playlist.name),
                    playlist.position,
                    playlist.role,
                    playlist.signal_class
                )?;
            }
            writeln!(
                output,
                "canonical_placements: {}",
                report.canonical_placements.len()
            )?;
            for placement in &report.canonical_placements {
                writeln!(
                    output,
                    "  - {} (position {}, key {}, source {})",
                    clean_cell(&placement.name),
                    placement.position,
                    placement.stable_key,
                    placement.source
                )?;
                writeln!(
                    output,
                    "    provenance: {}",
                    clean_cell(&placement.provenance.to_string())
                )?;
                if let Some(reason) = &placement.manual_reason {
                    writeln!(output, "    manual_reason: {}", clean_cell(reason))?;
                }
            }
            writeln!(output, "signals:")?;
            writeln!(
                output,
                "  listening: plays={} events={} skips={} hours={:.2} last_played={}",
                report.signals.play_count,
                report.signals.event_count,
                report.signals.skip_count,
                hours(report.signals.total_ms_played),
                report
                    .signals
                    .last_played_at
                    .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
            )?;
            writeln!(
                output,
                "  lifecycle: saved={} rotation={} discovery={} prompted={} intake={} recommendation={}",
                report.signals.saved,
                report.signals.rotation,
                report.signals.discovery,
                report.signals.prompted,
                report.signals.intake,
                report.signals.recommendation
            )?;
            if let Some(vector) = &report.vector {
                writeln!(output, "embedding:")?;
                writeln!(
                    output,
                    "  generation={} model={}@{} dimensions={}",
                    vector.embedding_generation_id,
                    vector.embedding_model,
                    vector.embedding_version,
                    vector.dimensions
                )?;
                writeln!(
                    output,
                    "  cluster={} similarity={} rank={}",
                    vector.cluster_label.as_deref().unwrap_or("unassigned"),
                    vector
                        .membership_score
                        .map_or_else(|| "-".to_owned(), |value| format!("{value:.4}")),
                    vector
                        .representative_rank
                        .map_or_else(|| "-".to_owned(), |value| value.to_string())
                )?;
            } else {
                writeln!(output, "embedding: unavailable")?;
            }
            writeln!(output, "semantic_facts: {}", report.semantic_facts.len())?;
            for fact in &report.semantic_facts {
                writeln!(
                    output,
                    "  - {}={} ({:.3}, {})",
                    fact.kind,
                    clean_cell(&fact.value),
                    fact.confidence,
                    fact.model
                )?;
            }
            if let Some(classification) = &report.user_classification {
                writeln!(output, "user_classification:")?;
                writeln!(
                    output,
                    "  collection: {}",
                    classification.values.collection.as_deref().unwrap_or("-")
                )?;
                writeln!(
                    output,
                    "  regions: {}",
                    list_or_dash(&classification.values.regions)
                )?;
                writeln!(
                    output,
                    "  traditions: {}",
                    list_or_dash(&classification.values.traditions)
                )?;
                writeln!(
                    output,
                    "  cohorts: {}",
                    list_or_dash(&classification.values.cohorts)
                )?;
                writeln!(
                    output,
                    "  languages: {}",
                    list_or_dash(&classification.values.languages)
                )?;
                writeln!(output, "  reason: {}", clean_cell(&classification.reason))?;
                writeln!(output, "  revision_id: {}", classification.id)?;
            } else {
                writeln!(output, "user_classification: none")?;
            }
            writeln!(
                output,
                "historical_source_playlists: {}",
                report.historical_playlists.len()
            )?;
            for playlist in &report.historical_playlists {
                writeln!(
                    output,
                    "  - {} (signal {}{}, present {}, first {}, last {}, spotify_id {})",
                    clean_cell(&playlist.names),
                    playlist.signal_class,
                    playlist
                        .behavioral_signal
                        .as_deref()
                        .map_or_else(String::new, |value| format!("/{value}")),
                    playlist.present,
                    playlist.first_seen_at.to_rfc3339(),
                    playlist.last_seen_at.to_rfc3339(),
                    playlist.spotify_id
                )?;
            }
            writeln!(
                output,
                "excluded: {}",
                report
                    .exclusion_reason
                    .as_deref()
                    .map(clean_cell)
                    .unwrap_or_else(|| "false".to_owned())
            )?;
            Ok(())
        }
        TrackCommand::Exclusions { account } => {
            let rows = tracks::active_exclusions(database, &account).await?;
            writeln!(output, "track\tartists\texcluded_at\treason\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}",
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    row.excluded_at.to_rfc3339(),
                    clean_cell(&row.reason),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        TrackCommand::EmptyExclusions { account, confirm } => {
            let report = tracks::empty_exclusions(database, &account, &confirm).await?;
            writeln!(output, "exclusions: emptied")?;
            writeln!(output, "cleared: {}", report.cleared)?;
            writeln!(output, "history_retained: true")?;
            writeln!(output, "replay_blocking_tombstones: retained")?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        TrackCommand::Exclude {
            account,
            spotify_id,
            reason,
            confirm,
        } => {
            let report =
                tracks::exclude(database, &account, &spotify_id, &reason, &confirm).await?;
            writeln!(output, "spotify_id: {}", report.spotify_id)?;
            writeln!(output, "state: {}", report.state)?;
            writeln!(
                output,
                "proposal_generation_id: {}",
                report
                    .proposal_generation_id
                    .map_or_else(|| "-".to_owned(), |value| value.to_string())
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        TrackCommand::Restore {
            account,
            spotify_id,
            reason,
            confirm,
        } => {
            let report =
                tracks::restore(database, &account, &spotify_id, &reason, &confirm).await?;
            writeln!(output, "spotify_id: {}", report.spotify_id)?;
            writeln!(output, "state: {}", report.state)?;
            writeln!(
                output,
                "proposal_generation_id: {}",
                report
                    .proposal_generation_id
                    .map_or_else(|| "-".to_owned(), |value| value.to_string())
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
    }
}

async fn run_classification_command(
    command: ClassificationCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        ClassificationCommand::Set {
            account,
            spotify_ids,
            collection,
            regions,
            traditions,
            cohorts,
            languages,
            notes,
            reason,
        } => {
            let revisions = classifications::set(
                database,
                &account,
                &spotify_ids,
                classifications::ClassificationValues {
                    collection,
                    regions,
                    traditions,
                    cohorts,
                    languages,
                    notes,
                },
                &reason,
            )
            .await?;
            writeln!(output, "classifications: active")?;
            writeln!(output, "tracks: {}", revisions.len())?;
            for revision in &revisions {
                write_classification_revision(output, revision)?;
            }
            writeln!(
                output,
                "next: run `chordrift embeddings generate --account {account}`"
            )?;
        }
        ClassificationCommand::Clear {
            account,
            spotify_ids,
            reason,
        } => {
            let changed = classifications::clear(database, &account, &spotify_ids, &reason).await?;
            writeln!(output, "classifications: cleared")?;
            writeln!(output, "tracks: {}", spotify_ids.len())?;
            writeln!(output, "previously_active: {changed}")?;
        }
        ClassificationCommand::History {
            account,
            spotify_id,
        } => {
            let revisions = classifications::history(database, &account, &spotify_id).await?;
            writeln!(output, "classification_history: {}", revisions.len())?;
            for revision in &revisions {
                write_classification_revision(output, revision)?;
            }
        }
        ClassificationCommand::Export {
            account,
            playlists,
            file,
        } => {
            let report = classifications::export(database, &account, &playlists, &file).await?;
            writeln!(output, "classification_export: created")?;
            writeln!(output, "file: {}", report.path)?;
            writeln!(output, "tracks: {}", report.tracks)?;
            writeln!(output, "active_changes: 0")?;
            writeln!(
                output,
                "next: edit only user_* columns, set action, and add a reason"
            )?;
        }
        ClassificationCommand::Import { account, file } => {
            let report = classifications::import(database, &account, &file).await?;
            write_classification_batch(output, &report)?;
            writeln!(output, "active_changes: 0")?;
            writeln!(
                output,
                "next: chordrift classify approve --batch {} --confirm {}",
                report.batch_id, report.batch_id
            )?;
        }
        ClassificationCommand::Approve {
            account,
            batch,
            confirm,
        } => {
            let report = classifications::approve(database, &account, batch, confirm).await?;
            write_classification_batch(output, &report)?;
            writeln!(
                output,
                "next: run `chordrift embeddings generate --account {account}`"
            )?;
        }
    }
    Ok(())
}

fn write_classification_revision(
    output: &mut impl Write,
    revision: &classifications::ClassificationRevision,
) -> Result<()> {
    writeln!(output, "revision_id: {}", revision.id)?;
    writeln!(
        output,
        "track: {} — {}",
        clean_cell(&revision.title),
        clean_cell(&revision.artists)
    )?;
    writeln!(output, "spotify_id: {}", revision.spotify_id)?;
    writeln!(output, "decision: {}", revision.decision)?;
    writeln!(
        output,
        "collection: {}",
        revision.values.collection.as_deref().unwrap_or("-")
    )?;
    writeln!(
        output,
        "regions: {}",
        list_or_dash(&revision.values.regions)
    )?;
    writeln!(
        output,
        "traditions: {}",
        list_or_dash(&revision.values.traditions)
    )?;
    writeln!(
        output,
        "cohorts: {}",
        list_or_dash(&revision.values.cohorts)
    )?;
    writeln!(
        output,
        "languages: {}",
        list_or_dash(&revision.values.languages)
    )?;
    writeln!(
        output,
        "notes: {}",
        revision
            .values
            .notes
            .as_deref()
            .map(clean_cell)
            .unwrap_or_else(|| "-".to_owned())
    )?;
    writeln!(output, "reason: {}", clean_cell(&revision.reason))?;
    writeln!(output, "source: {}", revision.source)?;
    writeln!(output, "created_at: {}", revision.created_at.to_rfc3339())?;
    writeln!(
        output,
        "status: {}",
        revision
            .superseded_at
            .map_or("active".to_owned(), |at| format!(
                "superseded@{}",
                at.to_rfc3339()
            ))
    )?;
    Ok(())
}

fn write_classification_batch(
    output: &mut impl Write,
    report: &classifications::BatchReport,
) -> Result<()> {
    writeln!(output, "classification_batch: {}", report.status)?;
    writeln!(output, "batch_id: {}", report.batch_id)?;
    writeln!(output, "entries: {}", report.entries)?;
    Ok(())
}

fn list_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

async fn run_route_command(
    command: RouteCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        RouteCommand::Create {
            account,
            name,
            description,
            background,
            artwork,
        } => {
            let route = routes::create(
                database,
                &account,
                &name,
                &description,
                &background,
                &artwork,
            )
            .await?;
            writeln!(output, "route: {}", route.name)?;
            writeln!(output, "stable_key: {}", route.stable_key)?;
            writeln!(output, "playlist_id: {}", route.playlist_id)?;
            writeln!(output, "artwork_sha256: {}", route.artwork_sha256)?;
            writeln!(
                output,
                "spotify_playlist_id: {}",
                route.spotify_playlist_id.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        RouteCommand::Add {
            account,
            route,
            spotify_ids,
            reason,
        } => {
            let report =
                routes::add(database, &account, &route, &spotify_ids, reason.as_deref()).await?;
            writeln!(output, "route: {}", report.route.name)?;
            writeln!(output, "added: {}", report.added)?;
            writeln!(output, "reused: {}", report.reused)?;
            writeln!(output, "desired_tracks: {}", report.route.track_count)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        RouteCommand::List { account } => {
            let rows = routes::list(database, &account).await?;
            writeln!(
                output,
                "active\ttracks\tspotify_id\tstable_key\tname\tdescription"
            )?;
            for route in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    route.active,
                    route.track_count,
                    route.spotify_playlist_id.as_deref().unwrap_or("-"),
                    route.stable_key,
                    clean_cell(&route.name),
                    clean_cell(&route.description)
                )?;
            }
            Ok(())
        }
        RouteCommand::Tracks { account, route } => {
            let (route, tracks) = routes::tracks(database, &account, &route).await?;
            writeln!(output, "route: {}", route.name)?;
            writeln!(
                output,
                "spotify_id: {}",
                route.spotify_playlist_id.as_deref().unwrap_or("-")
            )?;
            writeln!(output, "tracks: {}", tracks.len())?;
            writeln!(output, "position\ttrack\tartists\tspotify_track_id")?;
            for track in tracks {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    track.position,
                    clean_cell(&track.title),
                    clean_cell(&track.artists),
                    track.spotify_track_id
                )?;
            }
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
        BookmarkCommand::Refresh {
            account,
            name,
            spotify_id,
        } => {
            let selector = bookmark_selector(name, spotify_id);
            let bookmark = bookmarks::resolve(database, &account, &selector).await?;
            let fetched = spotify::fetch_bookmark(&account, &bookmark.provider_playlist_id).await?;
            let report = bookmarks::record_refresh(
                database,
                &account,
                &bookmark.provider_playlist_id,
                fetched,
            )
            .await?;
            writeln!(output, "bookmark_refresh: {}", report.status)?;
            writeln!(output, "refresh_id: {}", report.refresh_id)?;
            writeln!(output, "bookmark: {}", clean_cell(&report.name))?;
            writeln!(output, "spotify_id: {}", report.provider_playlist_id)?;
            writeln!(output, "provider_items: {}", report.item_count)?;
            writeln!(output, "captured_tracks: {}", report.captured_items)?;
            writeln!(output, "unavailable_items: {}", report.unavailable_items)?;
            writeln!(output, "unsupported_items: {}", report.unsupported_items)?;
            writeln!(output, "refreshed_at: {}", report.refreshed_at.to_rfc3339())?;
            writeln!(output, "normal_sync_request_budget: unchanged")?;
            writeln!(output, "spotify_writes: disabled")?;
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
                "session_related_tracks: {}",
                report.session_related_tracks
            )?;
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
        ProposalCommand::Inventory { account } => {
            let rows = proposals::historical_coverage(database, &account).await?;
            writeln!(
                output,
                "inventory\tplaylists\tplaced\texcluded\tunresolved\tconflicting\tsource_class"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.unique_tracks,
                    row.playlist_count,
                    row.represented_tracks,
                    row.excluded_tracks,
                    row.missing_tracks,
                    row.conflicting_tracks,
                    row.signal_class
                )?;
            }
            Ok(())
        }
        ProposalCommand::Unresolved { account, limit } => {
            let rows = proposals::historical_missing(database, &account, limit).await?;
            writeln!(
                output,
                "track\tartists\tsource_classes\tsource_playlists\tspotify_id"
            )?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}",
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    clean_cell(&row.signal_classes),
                    clean_cell(&row.source_playlists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        ProposalCommand::PlacementAudit { account } => {
            let report = proposals::placement_audit(database, &account).await?;
            writeln!(output, "placement_audit: complete")?;
            writeln!(
                output,
                "proposal_generation_id: {}",
                report.proposal_generation_id
            )?;
            writeln!(
                output,
                "embedding_generation_id: {}",
                report.embedding_generation_id
            )?;
            writeln!(output, "inventory_tracks: {}", report.inventory_tracks)?;
            writeln!(
                output,
                "already_placed_tracks: {}",
                report.already_placed_tracks
            )?;
            writeln!(
                output,
                "embedded_unresolved_tracks: {}",
                report.embedded_unresolved_tracks
            )?;
            writeln!(
                output,
                "unembedded_unresolved_tracks: {}",
                report.unembedded_unresolved_tracks
            )?;
            writeln!(output, "strong_existing_fit: {}", report.strong_fit_tracks)?;
            writeln!(output, "usable_existing_fit: {}", report.usable_fit_tracks)?;
            writeln!(output, "weak_fit_review: {}", report.weak_fit_tracks)?;
            writeln!(output, "\nstrong\tusable\tdestination\tstable_key")?;
            for destination in report.destinations {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    destination.strong_fit_tracks,
                    destination.usable_fit_tracks,
                    clean_cell(&destination.name),
                    destination.stable_key
                )?;
            }
            writeln!(
                output,
                "\nweak\tplaced\tdominant\tdominant_tracks\tcluster_tracks\trepresentative\tcluster"
            )?;
            for group in report.new_group_candidates {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    group.weak_fit_tracks,
                    group.placed_tracks,
                    clean_cell(group.dominant_destination.as_deref().unwrap_or("-")),
                    group.dominant_tracks,
                    group.cluster_tracks,
                    clean_cell(&group.representative),
                    group.machine_label
                )?;
            }
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::Extend {
            account,
            min_similarity,
        } => {
            let report = proposals::extend_approved(database, &account, min_similarity).await?;
            writeln!(
                output,
                "proposal_extension: {}",
                if report.reused {
                    "already current"
                } else {
                    "created"
                }
            )?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
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
        ProposalCommand::GroupTracks {
            account,
            cluster,
            limit,
        } => {
            let rows =
                proposals::unresolved_group_tracks(database, &account, &cluster, limit).await?;
            writeln!(output, "rank\tscore\ttrack\tartists\tspotify_id")?;
            for row in rows {
                writeln!(
                    output,
                    "{}\t{:.4}\t{}\t{}\t{}",
                    row.position,
                    row.score,
                    clean_cell(&row.title),
                    clean_cell(&row.artists),
                    row.spotify_id
                )?;
            }
            Ok(())
        }
        ProposalCommand::ConsensusAssign {
            account,
            min_dominance,
            min_evidence,
        } => {
            let report = proposals::assign_by_group_consensus(
                database,
                &account,
                min_dominance,
                min_evidence,
            )
            .await?;
            writeln!(output, "consensus_assignment: succeeded")?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "assigned_tracks: {}", report.assigned_tracks)?;
            writeln!(output, "required_tracks: {}", report.required_tracks)?;
            writeln!(output, "represented_tracks: {}", report.represented_tracks)?;
            writeln!(output, "unresolved_tracks: {}", report.unresolved_tracks)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::CentroidAssign {
            account,
            min_similarity,
        } => {
            let report =
                proposals::assign_by_existing_centroid(database, &account, min_similarity).await?;
            writeln!(output, "centroid_assignment: succeeded")?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "assigned_tracks: {}", report.assigned_tracks)?;
            writeln!(output, "required_tracks: {}", report.required_tracks)?;
            writeln!(output, "represented_tracks: {}", report.represented_tracks)?;
            writeln!(output, "unresolved_tracks: {}", report.unresolved_tracks)?;
            writeln!(output, "spotify_writes: disabled")?;
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
            for report in
                proposals::assign_many(database, &account, &spotify_id, &playlist, &reason).await?
            {
                write_assignment_report(output, &report)?;
            }
            Ok(())
        }
        ProposalCommand::AlignProviderOrder { account, playlist } => {
            let report = proposals::align_provider_order(database, &account, &playlist).await?;
            writeln!(output, "proposal_order: aligned")?;
            writeln!(output, "generation_id: {}", report.generation_id)?;
            writeln!(output, "stable_key: {}", report.stable_key)?;
            writeln!(output, "playlist: {}", clean_cell(&report.name))?;
            writeln!(output, "tracks: {}", report.track_count)?;
            writeln!(output, "spotify_writes: disabled")?;
            Ok(())
        }
        ProposalCommand::RetireEmpty {
            account,
            playlist,
            confirm,
        } => {
            let retired = proposals::retire_empty(database, &account, &playlist, &confirm).await?;
            writeln!(output, "proposal_generation_id: {}", retired.generation_id)?;
            writeln!(output, "retired: {}", clean_cell(&retired.name))?;
            writeln!(output, "stable_key: {}", retired.stable_key)?;
            writeln!(output, "spotify_writes: disabled")?;
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
        PlaylistSignalClassArg::UserManaged => playlists::PlaylistSignalClass::UserManaged,
        PlaylistSignalClassArg::SemanticLegacy => playlists::PlaylistSignalClass::SemanticLegacy,
        PlaylistSignalClassArg::ProviderCurated => playlists::PlaylistSignalClass::ProviderCurated,
        PlaylistSignalClassArg::Intake => playlists::PlaylistSignalClass::Intake,
        PlaylistSignalClassArg::Routing => playlists::PlaylistSignalClass::Routing,
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

fn write_pretty_track_inspection(
    output: &mut impl Write,
    report: &tracks::Inspection,
    technical: bool,
) -> Result<()> {
    writeln!(
        output,
        "\x1b[1;36m{}\x1b[0m  \x1b[2m— {}\x1b[0m",
        clean_cell(&report.title),
        clean_cell(&report.artists)
    )?;
    writeln!(
        output,
        "{}",
        terminal::pretty_table(
            &["Track details", "Value"],
            vec![
                vec![
                    "Album".to_owned(),
                    clean_cell(report.album.as_deref().unwrap_or("—")),
                ],
                vec![
                    "Duration".to_owned(),
                    report
                        .duration_ms
                        .map_or_else(|| "—".to_owned(), human_duration),
                ],
                vec!["Spotify ID".to_owned(), report.spotify_id.clone()],
                vec![
                    "ISRC".to_owned(),
                    report.isrc.clone().unwrap_or_else(|| "—".to_owned()),
                ],
                vec![
                    "Liked now".to_owned(),
                    yes_no(report.signals.saved).to_owned(),
                ],
                vec![
                    "Excluded".to_owned(),
                    report
                        .exclusion_reason
                        .as_deref()
                        .map(clean_cell)
                        .unwrap_or_else(|| "No".to_owned()),
                ],
            ],
        )
    )?;

    writeln!(output, "\n\x1b[1;36mCurrent location\x1b[0m")?;
    let location_rows = if report.current_playlists.is_empty() {
        vec![vec![
            "—".to_owned(),
            "—".to_owned(),
            "Not in a current playlist".to_owned(),
        ]]
    } else {
        report
            .current_playlists
            .iter()
            .map(|playlist| {
                vec![
                    clean_cell(&playlist.name),
                    playlist.position.to_string(),
                    format!("{} · {}", playlist.role, playlist.signal_class),
                ]
            })
            .collect()
    };
    writeln!(
        output,
        "{}",
        terminal::pretty_table(&["Spotify playlist", "Position", "Meaning"], location_rows)
    )?;

    writeln!(output, "\n\x1b[1;36mChordrift placement\x1b[0m")?;
    let placement_rows = if report.canonical_placements.is_empty() {
        vec![vec![
            "—".to_owned(),
            "—".to_owned(),
            "No canonical placement".to_owned(),
            "—".to_owned(),
        ]]
    } else {
        report
            .canonical_placements
            .iter()
            .map(|placement| {
                let provenance = &placement.provenance;
                let decision = placement
                    .manual_reason
                    .as_deref()
                    .map(clean_cell)
                    .unwrap_or_else(|| {
                        let method = provenance
                            .get("method")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(&placement.source)
                            .replace('-', " ");
                        let dominance = provenance
                            .get("dominance")
                            .and_then(serde_json::Value::as_f64);
                        let known = provenance
                            .get("dominant_known_tracks")
                            .and_then(serde_json::Value::as_u64);
                        let total = provenance
                            .get("known_placed_tracks")
                            .and_then(serde_json::Value::as_u64);
                        match (dominance, known, total) {
                            (Some(dominance), Some(known), Some(total)) => format!(
                                "{method} · {:.1}% consensus ({known}/{total})",
                                dominance * 100.0
                            ),
                            _ => method,
                        }
                    });
                let score = provenance
                    .get("cluster_membership_score")
                    .and_then(serde_json::Value::as_f64)
                    .map_or_else(|| "—".to_owned(), |value| format!("{value:.3}"));
                vec![
                    clean_cell(&placement.name),
                    placement.position.to_string(),
                    decision,
                    score,
                ]
            })
            .collect()
    };
    writeln!(
        output,
        "{}",
        terminal::pretty_table(
            &["Destination", "Desired position", "Why", "Model score"],
            placement_rows
        )
    )?;

    writeln!(output, "\n\x1b[1;36mListening and preference\x1b[0m")?;
    let last_played = report
        .signals
        .last_played_at
        .map_or_else(|| "—".to_owned(), |at| at.format("%Y-%m-%d").to_string());
    writeln!(
        output,
        "{}",
        terminal::pretty_table(
            &[
                "Meaningful plays",
                "Listening time",
                "Last played",
                "Current signals"
            ],
            vec![vec![
                report.signals.play_count.to_string(),
                format!("{:.1} hours", hours(report.signals.total_ms_played)),
                last_played,
                active_signals(&report.signals),
            ]],
        )
    )?;

    writeln!(output, "\n\x1b[1;36mClassification\x1b[0m")?;
    let classification_rows = if let Some(classification) = &report.user_classification {
        vec![
            vec![
                "Collection".to_owned(),
                classification
                    .values
                    .collection
                    .clone()
                    .unwrap_or_else(|| "—".to_owned()),
            ],
            vec![
                "Regions".to_owned(),
                list_or_dash(&classification.values.regions),
            ],
            vec![
                "Traditions".to_owned(),
                list_or_dash(&classification.values.traditions),
            ],
            vec![
                "Cohorts".to_owned(),
                list_or_dash(&classification.values.cohorts),
            ],
            vec![
                "Languages".to_owned(),
                list_or_dash(&classification.values.languages),
            ],
            vec!["Reason".to_owned(), clean_cell(&classification.reason)],
        ]
    } else {
        vec![vec!["User classification".to_owned(), "None".to_owned()]]
    };
    writeln!(
        output,
        "{}",
        terminal::pretty_table(&["Dimension", "Value"], classification_rows)
    )?;

    writeln!(output, "\n\x1b[1;36mRetained history\x1b[0m")?;
    let history_rows: Vec<Vec<String>> = report
        .historical_playlists
        .iter()
        .map(|playlist| {
            vec![
                clean_cell(&playlist.names),
                if playlist.present {
                    "Current".to_owned()
                } else {
                    "Retired".to_owned()
                },
                format!(
                    "{}{}",
                    playlist.signal_class,
                    playlist
                        .behavioral_signal
                        .as_deref()
                        .map_or_else(String::new, |value| format!(" · {value}"))
                ),
                playlist.first_seen_at.format("%Y-%m-%d").to_string(),
                playlist.last_seen_at.format("%Y-%m-%d").to_string(),
            ]
        })
        .collect();
    writeln!(
        output,
        "{}",
        terminal::pretty_table(
            &[
                "Source playlist",
                "State",
                "Meaning",
                "First seen",
                "Last seen"
            ],
            history_rows
        )
    )?;

    if technical {
        writeln!(output, "\n\x1b[1;36mTechnical details\x1b[0m")?;
        let mut rows = vec![vec![
            "Canonical track ID".to_owned(),
            report.track_id.to_string(),
        ]];
        if let Some(vector) = &report.vector {
            rows.push(vec![
                "Embedding".to_owned(),
                format!(
                    "{}@{} · {} dimensions · {}",
                    vector.embedding_model,
                    vector.embedding_version,
                    vector.dimensions,
                    vector.embedding_generation_id
                ),
            ]);
            rows.push(vec![
                "Cluster".to_owned(),
                format!(
                    "{} · similarity {} · rank {}",
                    vector.cluster_label.as_deref().unwrap_or("unassigned"),
                    vector
                        .membership_score
                        .map_or_else(|| "—".to_owned(), |value| format!("{value:.4}")),
                    vector
                        .representative_rank
                        .map_or_else(|| "—".to_owned(), |value| value.to_string())
                ),
            ]);
        }
        for placement in &report.canonical_placements {
            rows.push(vec![
                format!("Placement · {}", placement.stable_key),
                clean_cell(&placement.provenance.to_string()),
            ]);
        }
        writeln!(
            output,
            "{}",
            terminal::pretty_table(&["Reference", "Value"], rows)
        )?;
    } else {
        writeln!(
            output,
            "\n\x1b[2mUse --technical to show generation IDs and raw placement provenance.\x1b[0m"
        )?;
    }
    Ok(())
}

fn human_duration(milliseconds: i32) -> String {
    let seconds = milliseconds.max(0) / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn active_signals(signals: &tracks::ListeningSignals) -> String {
    let mut values = Vec::new();
    if signals.rotation {
        values.push("high rotation");
    }
    if signals.discovery {
        values.push("discovery");
    }
    if signals.prompted {
        values.push("prompted");
    }
    if signals.intake {
        values.push("intake");
    }
    if signals.recommendation {
        values.push("friend recommendation");
    }
    if values.is_empty() {
        "—".to_owned()
    } else {
        values.join(" · ")
    }
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

struct SyncPullTimings {
    import: Duration,
    analysis: Duration,
    history: Duration,
    verification: Duration,
    total: Duration,
}

fn write_sync_pull_plain_report(
    output: &mut impl Write,
    import: &spotify::ImportReport,
    analysis: &analysis::AnalysisSummary,
    history: Option<&history::HistorySummary>,
    verified_apply_runs: usize,
    timings: &SyncPullTimings,
) -> Result<()> {
    write_import_report(output, import)?;
    write_analysis_summary(output, analysis)?;
    if let Some(history) = history {
        write_history_summary(output, history)?;
    }
    writeln!(output, "verified_apply_runs: {verified_apply_runs}")?;
    writeln!(
        output,
        "provider_elapsed_ms: {}",
        timings.import.as_millis()
    )?;
    writeln!(
        output,
        "analysis_elapsed_ms: {}",
        timings.analysis.as_millis()
    )?;
    writeln!(
        output,
        "history_elapsed_ms: {}",
        timings.history.as_millis()
    )?;
    writeln!(
        output,
        "verification_elapsed_ms: {}",
        timings.verification.as_millis()
    )?;
    writeln!(output, "total_elapsed_ms: {}", timings.total.as_millis())?;
    Ok(())
}

fn write_sync_pull_report(
    output: &mut impl Write,
    import: &spotify::ImportReport,
    analysis: &analysis::AnalysisSummary,
    history: Option<&history::HistorySummary>,
    verified_apply_runs: usize,
    timings: &SyncPullTimings,
) -> Result<()> {
    writeln!(
        output,
        "\x1b[1;36mSync complete\x1b[0m  \x1b[2m— {} · {}\x1b[0m",
        import.account_label,
        format_elapsed(timings.total)
    )?;
    writeln!(
        output,
        "{}",
        terminal::pretty_table(
            &["Phase", "Result", "Elapsed"],
            vec![
                vec![
                    "Provider state".to_owned(),
                    if import.library_unchanged {
                        format!(
                            "current · {} playlists reused · {} API requests",
                            import.playlists_reused, import.spotify_api_requests
                        )
                    } else {
                        format!(
                            "updated · {} playlists imported · {} API requests",
                            import.playlists_imported, import.spotify_api_requests
                        )
                    },
                    format_elapsed(timings.import),
                ],
                vec![
                    "Library analysis".to_owned(),
                    if import.library_unchanged {
                        "reused".to_owned()
                    } else {
                        "refreshed".to_owned()
                    },
                    format_elapsed(timings.analysis),
                ],
                vec![
                    "Listening history".to_owned(),
                    format!("{} new observations", import.recent_plays_inserted),
                    format_elapsed(timings.history),
                ],
                vec![
                    "Publication checks".to_owned(),
                    if verified_apply_runs == 0 {
                        "nothing pending".to_owned()
                    } else {
                        format!("{verified_apply_runs} verified")
                    },
                    format_elapsed(timings.verification),
                ],
            ]
        )
    )?;

    writeln!(output, "\n\x1b[1mCurrent library\x1b[0m")?;
    let mut library_rows = vec![
        vec!["Playlists".to_owned(), format_count(analysis.playlists)],
        vec![
            "Playlist tracks".to_owned(),
            format!(
                "{} entries · {} unique · {} overlaps",
                format_count(analysis.playlist_entries),
                format_count(analysis.unique_playlist_tracks),
                format_count(analysis.overlapping_tracks)
            ),
        ],
        vec![
            "Saved tracks".to_owned(),
            format!(
                "{}{}",
                import.saved_tracks,
                if import.saved_tracks_reused {
                    " · unchanged"
                } else {
                    ""
                }
            ),
        ],
        vec![
            "Saved albums".to_owned(),
            format!(
                "{} albums · {} tracks{}",
                import.saved_albums,
                import.saved_album_tracks,
                if import.saved_albums_reused {
                    " · unchanged"
                } else {
                    ""
                }
            ),
        ],
        vec![
            "Recent plays".to_owned(),
            format!(
                "{} seen · {} inserted{}",
                import.recent_plays_seen,
                import.recent_plays_inserted,
                import
                    .recent_plays_through
                    .map_or_else(String::new, |value| format!(
                        " · through {}",
                        value.to_rfc3339()
                    ))
            ),
        ],
    ];
    let warnings = import.unavailable_items
        + import.unsupported_items
        + import.inaccessible_collaborative_playlists;
    if warnings != 0 || import.followed_playlists_skipped != 0 {
        library_rows.push(vec![
            "Skipped".to_owned(),
            format!(
                "{} unavailable/unsupported/inaccessible · {} followed playlists",
                warnings, import.followed_playlists_skipped
            ),
        ]);
    }
    writeln!(
        output,
        "{}",
        terminal::pretty_table(&["Surface", "State"], library_rows)
    )?;

    if let Some(history) = history {
        writeln!(output, "\n\x1b[1mListening evidence\x1b[0m")?;
        writeln!(
            output,
            "{}",
            terminal::pretty_table(
                &["Evidence", "State"],
                vec![
                    vec![
                        "Events".to_owned(),
                        format!(
                            "{} · {} matched · {} unmatched",
                            format_count(history.events),
                            format_count(history.matched_events),
                            format_count(history.unmatched_events)
                        ),
                    ],
                    vec![
                        "Track identities".to_owned(),
                        format!(
                            "{} · {} matched · {} unmatched",
                            format_count(history.unique_tracks),
                            format_count(history.matched_unique_tracks),
                            format_count(history.unmatched_unique_tracks)
                        ),
                    ],
                    vec![
                        "Listening time".to_owned(),
                        format!("{:.2} hours", hours(history.total_ms_played)),
                    ],
                    vec![
                        "Range".to_owned(),
                        format!(
                            "{} → {}",
                            history
                                .first_event_at
                                .map_or_else(|| "—".to_owned(), |value| value.to_rfc3339()),
                            history
                                .last_event_at
                                .map_or_else(|| "—".to_owned(), |value| value.to_rfc3339())
                        ),
                    ],
                ]
            )
        )?;
    }
    writeln!(
        output,
        "\x1b[2mObservation {} · Spotify writes disabled\x1b[0m",
        import.snapshot_id
    )?;
    Ok(())
}

fn format_elapsed(duration: Duration) -> String {
    if duration.as_secs() == 0 {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{:.1} s", duration.as_secs_f64())
    }
}

fn format_count(value: i64) -> String {
    let negative = value.is_negative();
    let digits = value.unsigned_abs().to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    if negative {
        formatted.insert(0, '-');
    }
    formatted
}

fn write_import_report(output: &mut impl Write, report: &spotify::ImportReport) -> Result<()> {
    writeln!(output, "spotify import: succeeded")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(
        output,
        "spotify_api_requests: {}",
        report.spotify_api_requests
    )?;
    writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
    writeln!(
        output,
        "playlists: {}/{} imported",
        report.playlists_imported, report.playlists_seen
    )?;
    writeln!(output, "playlists_reused: {}", report.playlists_reused)?;
    writeln!(output, "library_unchanged: {}", report.library_unchanged)?;
    writeln!(output, "playlist_entries: {}", report.playlist_entries)?;
    writeln!(output, "saved_tracks: {}", report.saved_tracks)?;
    writeln!(
        output,
        "saved_tracks_reused: {}",
        report.saved_tracks_reused
    )?;
    writeln!(output, "saved_albums: {}", report.saved_albums)?;
    writeln!(output, "saved_album_tracks: {}", report.saved_album_tracks)?;
    writeln!(
        output,
        "saved_albums_reused: {}",
        report.saved_albums_reused
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
    writeln!(output, "recent_plays_seen: {}", report.recent_plays_seen)?;
    writeln!(
        output,
        "recent_plays_inserted: {}",
        report.recent_plays_inserted
    )?;
    writeln!(
        output,
        "recent_plays_through: {}",
        report
            .recent_plays_through
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    Ok(())
}

async fn run_db_command(
    command: DbCommand,
    output: &mut impl Write,
    database: &storexa::Database,
) -> Result<()> {
    match command {
        DbCommand::Migrate => {
            let report = db::migrate(database).await?;
            writeln!(
                output,
                "migrations: {} available migration(s) current in {} ms",
                report.available,
                report.elapsed.as_millis()
            )?;
            let status = db::status(database).await?;
            write_status(output, &status)
        }
        DbCommand::Status => {
            let status = db::status(database).await?;
            write_status(output, &status)
        }
        DbCommand::InvariantReport { account } => {
            let report = db_reports::invariant_report(database, &account).await?;
            write_invariant_report(output, &report)
        }
        DbCommand::StorageReport => {
            let report = db_reports::storage_report(database).await?;
            write_storage_report(output, &report)
        }
        DbCommand::Compact {
            command: DbCompactCommand::Plan { account },
        } => {
            let report = db_reports::compaction_plan(database, &account).await?;
            write_compaction_plan(output, &report)
        }
        DbCommand::Compact {
            command: DbCompactCommand::Cleanup { command },
        } => match command {
            DbCleanupCommand::Plan { account } => {
                let report = db_cleanup::plan(database, &account).await?;
                write_database_cleanup_plan(output, &report)
            }
            DbCleanupCommand::Apply { account, confirm } => {
                let report = db_cleanup::apply(database, &account, &confirm).await?;
                write_database_cleanup_verification(output, &report)
            }
            DbCleanupCommand::Verify { account } => {
                let report = db_cleanup::verify(database, &account).await?;
                write_database_cleanup_verification(output, &report)
            }
        },
        DbCommand::V2 {
            command: DbV2Command::Status { account },
        } => {
            let report = db_reports::database_v2_status(database, &account).await?;
            write_database_v2_status(output, &report)
        }
        DbCommand::V2 {
            command:
                DbV2Command::Migration {
                    command: DbV2MigrationCommand::Plan { account },
                },
        } => {
            let report = db_v2_migration::plan(database, &account).await?;
            write_database_v2_migration_plan(output, &report)
        }
        DbCommand::V2 {
            command:
                DbV2Command::Migration {
                    command: DbV2MigrationCommand::Apply { account, confirm },
                },
        } => {
            let report = db_v2_migration::apply(database, &account, &confirm).await?;
            writeln!(
                output,
                "migration_apply: normalized-evidence-checkpoints-v1"
            )?;
            writeln!(output, "plan_sha256: {}", report.plan_sha256)?;
            write_database_v2_migration_verification(output, &report.verification)
        }
        DbCommand::V2 {
            command:
                DbV2Command::Migration {
                    command: DbV2MigrationCommand::Verify { account },
                },
        } => {
            let report = db_v2_migration::verify(database, &account).await?;
            write_database_v2_migration_verification(output, &report)
        }
        DbCommand::V2 {
            command: DbV2Command::CutoverPlan { account },
        } => {
            let (plan_sha256, verification, current_state_verified) =
                db_v2_migration::cutover_plan(database, &account).await?;
            write_database_v2_cutover_plan(
                output,
                &plan_sha256,
                &verification,
                current_state_verified,
            )
        }
    }
}

fn write_database_v2_migration_plan(
    output: &mut impl Write,
    report: &db_v2_migration::MigrationPlan,
) -> Result<()> {
    writeln!(output, "migration_plan: normalized-evidence-checkpoints-v1")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "plan_sha256: {}", report.plan_sha256)?;
    writeln!(
        output,
        "current_snapshot_id: {}",
        report.current_snapshot_id
    )?;
    writeln!(output, "legacy_events: {}", report.legacy_events)?;
    writeln!(
        output,
        "active_legacy_events: {}",
        report.active_legacy_events
    )?;
    writeln!(
        output,
        "historical_identities: {}",
        report.historical_identities
    )?;
    writeln!(output, "archive_imports: {}", report.archive_imports)?;
    writeln!(
        output,
        "known_archive_source_files: {}",
        report.known_archive_source_files
    )?;
    writeln!(
        output,
        "checkpoint_source_snapshots: {}",
        report.checkpoint_source_snapshots
    )?;
    writeln!(
        output,
        "sync_plan_references: {}",
        report.sync_plan_references
    )?;
    writeln!(
        output,
        "verification_references: {}",
        report.verification_references
    )?;
    writeln!(output, "cleanup_references: {}", report.cleanup_references)?;
    writeln!(
        output,
        "reevaluation_references: {}",
        report.reevaluation_references
    )?;
    writeln!(
        output,
        "unsupported_media_events: {}",
        report.unsupported_media_events
    )?;
    writeln!(
        output,
        "archive_events_missing_import: {}",
        report.archive_events_missing_import
    )?;
    writeln!(
        output,
        "events_missing_provider_identity: {}",
        report.events_missing_provider_identity
    )?;
    writeln!(output, "applicable: {}", report.applicable)?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
}

fn write_database_v2_migration_verification(
    output: &mut impl Write,
    report: &db_v2_migration::MigrationVerification,
) -> Result<()> {
    writeln!(
        output,
        "migration_verify: normalized-evidence-checkpoints-v1"
    )?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "legacy_events: {}", report.legacy_events)?;
    writeln!(output, "normalized_events: {}", report.normalized_events)?;
    writeln!(
        output,
        "active_legacy_events: {}",
        report.active_legacy_events
    )?;
    writeln!(
        output,
        "active_normalized_events: {}",
        report.active_normalized_events
    )?;
    writeln!(output, "legacy_duration_ms: {}", report.legacy_duration_ms)?;
    writeln!(
        output,
        "normalized_duration_ms: {}",
        report.normalized_duration_ms
    )?;
    writeln!(
        output,
        "legacy_first_event_at: {}",
        report
            .legacy_first_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "normalized_first_event_at: {}",
        report
            .normalized_first_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "legacy_last_event_at: {}",
        report
            .legacy_last_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "normalized_last_event_at: {}",
        report
            .normalized_last_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "legacy_matched_events: {}",
        report.legacy_matched_events
    )?;
    writeln!(
        output,
        "normalized_matched_events: {}",
        report.normalized_matched_events
    )?;
    writeln!(
        output,
        "legacy_matched_identities: {}",
        report.legacy_matched_identities
    )?;
    writeln!(
        output,
        "normalized_matched_identities: {}",
        report.normalized_matched_identities
    )?;
    writeln!(
        output,
        "legacy_unmatched_identities: {}",
        report.legacy_unmatched_identities
    )?;
    writeln!(
        output,
        "normalized_unmatched_identities: {}",
        report.normalized_unmatched_identities
    )?;
    writeln!(
        output,
        "archive_manifests_match: {}",
        report.archive_manifests_match
    )?;
    writeln!(
        output,
        "plans_awaiting_checkpoints: {}",
        report.plans_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "verifications_awaiting_checkpoints: {}",
        report.verifications_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "cleanups_awaiting_checkpoints: {}",
        report.cleanups_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "reevaluations_awaiting_checkpoints: {}",
        report.reevaluations_awaiting_checkpoints
    )?;
    writeln!(output, "checkpoints: {}", report.checkpoints)?;
    writeln!(output, "verified: {}", report.verified)?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
}

fn write_database_v2_cutover_plan(
    output: &mut impl Write,
    plan_sha256: &str,
    verification: &db_v2_migration::MigrationVerification,
    current_state_verified: bool,
) -> Result<()> {
    writeln!(output, "production_cutover_plan: database-v2-v1")?;
    writeln!(output, "plan_sha256: {plan_sha256}")?;
    writeln!(output, "schema_migrations: 0040-0043")?;
    writeln!(output, "account: {}", verification.account_label)?;
    writeln!(output, "evidence_verified: {}", verification.verified)?;
    writeln!(output, "current_state_verified: {current_state_verified}")?;
    writeln!(
        output,
        "rehearsal_verified: {}",
        verification.verified && current_state_verified
    )?;
    writeln!(
        output,
        "approval_state: separate_explicit_approval_required"
    )?;
    writeln!(
        output,
        "step_1: verify backup checksum and production invariant report"
    )?;
    writeln!(
        output,
        "step_2: apply additive migrations 0040 through 0043"
    )?;
    writeln!(
        output,
        "step_3: run db v2 migration plan and compare exact counts"
    )?;
    writeln!(
        output,
        "step_4: approve and apply only the emitted migration hash"
    )?;
    writeln!(
        output,
        "step_5: run db v2 migration verify and db v2 status"
    )?;
    writeln!(
        output,
        "step_6: switch reads only after every parity gate is true"
    )?;
    writeln!(
        output,
        "step_7: retain legacy tables through an observation window"
    )?;
    writeln!(
        output,
        "rollback: switch reads back to intact legacy tables"
    )?;
    writeln!(output, "legacy_deletion: excluded_from_this_plan")?;
    writeln!(output, "production_connection_change: disabled")?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    writeln!(output, "spotify_writes: disabled")?;
    Ok(())
}

fn write_database_v2_status(
    output: &mut impl Write,
    report: &db_reports::DatabaseV2Status,
) -> Result<()> {
    writeln!(output, "database_v2_status: additive-foundation-v1")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "legacy_snapshot_id: {}", report.legacy_snapshot_id)?;
    writeln!(
        output,
        "current_source_snapshot_id: {}",
        report
            .current_source_snapshot_id
            .map_or_else(|| "-".to_owned(), |value| value.to_string())
    )?;
    writeln!(output, "current_playlists: {}", report.current_playlists)?;
    writeln!(
        output,
        "current_playlist_tracks: {}",
        report.current_playlist_tracks
    )?;
    writeln!(output, "playlist_revisions: {}", report.playlist_revisions)?;
    writeln!(
        output,
        "current_playlist_headers_match: {}",
        report.current_playlist_headers_match
    )?;
    writeln!(
        output,
        "current_playlist_order_matches: {}",
        report.current_playlist_order_matches
    )?;
    writeln!(
        output,
        "current_saved_tracks_match: {}",
        report.current_saved_tracks_match
    )?;
    writeln!(
        output,
        "current_saved_albums_match: {}",
        report.current_saved_albums_match
    )?;
    writeln!(
        output,
        "legacy_listening_events: {}",
        report.legacy_listening_events
    )?;
    writeln!(
        output,
        "normalized_listening_events: {}",
        report.normalized_listening_events
    )?;
    writeln!(
        output,
        "historical_identities: {}",
        report.historical_identities
    )?;
    writeln!(
        output,
        "legacy_archive_imports: {}",
        report.legacy_archive_imports
    )?;
    writeln!(output, "evidence_imports: {}", report.evidence_imports)?;
    writeln!(output, "checkpoints: {}", report.checkpoints)?;
    writeln!(
        output,
        "plans_awaiting_checkpoints: {}",
        report.plans_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "verifications_awaiting_checkpoints: {}",
        report.verifications_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "cleanups_awaiting_checkpoints: {}",
        report.cleanups_awaiting_checkpoints
    )?;
    writeln!(
        output,
        "reevaluations_awaiting_checkpoints: {}",
        report.reevaluations_awaiting_checkpoints
    )?;
    writeln!(output, "ready_for_cutover: {}", report.ready_for_cutover)?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
}

fn write_invariant_report(
    output: &mut impl Write,
    report: &db_reports::InvariantReport,
) -> Result<()> {
    writeln!(output, "invariant_report: database-v2-v1")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "provider: {}", report.provider)?;
    writeln!(output, "provider_accounts: {}", report.provider_accounts)?;
    writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
    writeln!(
        output,
        "snapshot_captured_at: {}",
        report.snapshot_captured_at.to_rfc3339()
    )?;
    writeln!(output, "playlists: {}", report.playlist_count)?;
    writeln!(
        output,
        "playlist_memberships: {}",
        report.playlist_memberships
    )?;
    writeln!(
        output,
        "playlist_order_fingerprint: {}",
        report.playlist_order_fingerprint
    )?;
    writeln!(
        output,
        "unique_playlist_tracks: {}",
        report.unique_playlist_tracks
    )?;
    writeln!(output, "saved_tracks: {}", report.saved_tracks)?;
    writeln!(output, "saved_albums: {}", report.saved_albums)?;
    writeln!(output, "saved_album_tracks: {}", report.saved_album_tracks)?;
    writeln!(
        output,
        "canonical_generation_id: {}",
        report.canonical_generation_id
    )?;
    writeln!(
        output,
        "canonical_playlists: {}",
        report.canonical_playlists
    )?;
    writeln!(
        output,
        "canonical_assignments: {}",
        report.canonical_assignments
    )?;
    writeln!(
        output,
        "unique_canonical_tracks: {}",
        report.unique_canonical_tracks
    )?;
    writeln!(
        output,
        "canonical_fingerprint: {}",
        report.canonical_fingerprint
    )?;
    writeln!(output, "active_exclusions: {}", report.active_exclusions)?;
    writeln!(
        output,
        "reevaluate_surfaces: {}",
        report.reevaluate_surfaces
    )?;
    writeln!(output, "reevaluate_tracks: {}", report.reevaluate_tracks)?;
    writeln!(
        output,
        "reevaluate_fingerprint: {}",
        report.reevaluate_fingerprint
    )?;
    writeln!(output, "listening_events: {}", report.history.events)?;
    writeln!(
        output,
        "historical_identities: {}",
        report.history.unique_tracks
    )?;
    writeln!(
        output,
        "matched_historical_identities: {}",
        report.history.matched_unique_tracks
    )?;
    writeln!(
        output,
        "unmatched_historical_identities: {}",
        report.history.unmatched_unique_tracks
    )?;
    writeln!(
        output,
        "matched_listening_events: {}",
        report.history.matched_events
    )?;
    writeln!(
        output,
        "unmatched_listening_events: {}",
        report.history.unmatched_events
    )?;
    writeln!(
        output,
        "total_listening_ms: {}",
        report.history.total_ms_played
    )?;
    writeln!(
        output,
        "first_listening_at: {}",
        report
            .history
            .first_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(
        output,
        "last_listening_at: {}",
        report
            .history
            .last_event_at
            .map_or_else(|| "-".to_owned(), |value| value.to_rfc3339())
    )?;
    writeln!(output, "spotify_archive_imports: {}", report.archives.len())?;
    for (index, archive) in report.archives.iter().enumerate() {
        writeln!(output, "archive_{}_sha256: {}", index + 1, archive.sha256)?;
        writeln!(output, "archive_{}_kind: {}", index + 1, archive.kind)?;
        writeln!(
            output,
            "archive_{}_events_imported: {}",
            index + 1,
            archive.events_imported
        )?;
        writeln!(
            output,
            "archive_{}_events_matched: {}",
            index + 1,
            archive.events_matched
        )?;
        writeln!(
            output,
            "archive_{}_imported_at: {}",
            index + 1,
            archive.imported_at.to_rfc3339()
        )?;
    }
    writeln!(
        output,
        "verified_apply_runs: {}",
        report.verified_apply_runs
    )?;
    writeln!(output, "latest_plan_id: {}", report.latest_plan_id)?;
    writeln!(
        output,
        "latest_planner_version: {}",
        report.latest_planner_version
    )?;
    writeln!(
        output,
        "latest_plan_operations: {}",
        report.latest_plan_operations
    )?;
    writeln!(
        output,
        "latest_plan_input_hash: {}",
        report.latest_plan_input_hash
    )?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
}

fn write_storage_report(output: &mut impl Write, report: &db_reports::StorageReport) -> Result<()> {
    writeln!(output, "storage_report: database-v2-v1")?;
    writeln!(output, "database_bytes: {}", report.database_bytes)?;
    writeln!(
        output,
        "table\theap_bytes\ttable_bytes\tindex_bytes\ttotal_bytes"
    )?;
    for table in &report.tables {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            table.table, table.heap_bytes, table.table_bytes, table.index_bytes, table.total_bytes
        )?;
    }
    writeln!(
        output,
        "TOTAL\t{}\t{}\t{}\t{}",
        report.heap_bytes, report.table_bytes, report.index_bytes, report.total_bytes
    )?;
    writeln!(output, "database_writes: disabled")?;
    Ok(())
}

fn write_compaction_plan(
    output: &mut impl Write,
    report: &db_reports::CompactionPlan,
) -> Result<()> {
    writeln!(output, "compaction_plan: database-v2-v1")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "mode: read-only")?;
    writeln!(output, "snapshots_total: {}", report.snapshots_total)?;
    writeln!(
        output,
        "current_snapshots_keep: {}",
        report.current_snapshots
    )?;
    writeln!(
        output,
        "protected_historical_snapshots_keep: {}",
        report.protected_historical_snapshots
    )?;
    writeln!(
        output,
        "redundant_routine_snapshots_normalize: {}",
        report.redundant_routine_snapshots
    )?;
    writeln!(
        output,
        "redundant_playlist_headers_normalize: {}",
        report.redundant_playlist_headers
    )?;
    writeln!(
        output,
        "redundant_playlist_memberships_normalize: {}",
        report.redundant_playlist_memberships
    )?;
    writeln!(
        output,
        "redundant_saved_tracks_normalize: {}",
        report.redundant_saved_tracks
    )?;
    writeln!(
        output,
        "redundant_saved_albums_normalize: {}",
        report.redundant_saved_albums
    )?;
    writeln!(
        output,
        "redundant_saved_album_tracks_normalize: {}",
        report.redundant_saved_album_tracks
    )?;
    writeln!(
        output,
        "plan_protected_snapshots: {}",
        report.plan_protected_snapshots
    )?;
    writeln!(
        output,
        "verification_protected_snapshots: {}",
        report.verification_protected_snapshots
    )?;
    writeln!(
        output,
        "generation_protected_snapshots: {}",
        report.generation_protected_snapshots
    )?;
    writeln!(
        output,
        "bookmark_protected_snapshots: {}",
        report.bookmark_protected_snapshots
    )?;
    writeln!(
        output,
        "intent_audit_protected_snapshots: {}",
        report.intent_audit_protected_snapshots
    )?;
    writeln!(
        output,
        "listening_events_keep_normalized: {}",
        report.listening_events
    )?;
    writeln!(
        output,
        "historical_identities_keep_normalized: {}",
        report.historical_identities
    )?;
    writeln!(
        output,
        "raw_event_json_bytes_recoverable_after_archive_rehearsal: {}",
        report.raw_event_json_bytes
    )?;
    writeln!(
        output,
        "event_fields_keep: provider_account,provider_identity,played_at,ms_played,skipped,completion_evidence,context,source_import,source_kind,superseded_at"
    )?;
    writeln!(
        output,
        "identity_fields_store_once: track_name,artist_name,album_name"
    )?;
    writeln!(
        output,
        "archive_recoverable_fields: platform,country,reason_start,reason_end,shuffle,offline,offline_timestamp,incognito_mode,raw_source_file"
    )?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
}

fn write_database_cleanup_plan(
    output: &mut impl Write,
    report: &db_cleanup::CleanupPlan,
) -> Result<()> {
    writeln!(output, "cleanup_plan: database-v2-clean-runtime-v1")?;
    writeln!(output, "account: {}", report.account_label)?;
    writeln!(output, "plan_sha256: {}", report.plan_sha256)?;
    writeln!(output, "invariant_sha256: {}", report.invariant_sha256)?;
    writeln!(
        output,
        "provider_observations_retained: {}",
        report.observations_retained
    )?;
    writeln!(
        output,
        "legacy_provider_rows_removed: {}",
        report.legacy_provider_rows_removed
    )?;
    writeln!(
        output,
        "legacy_listening_events_removed: {}",
        report.legacy_listening_events_removed
    )?;
    writeln!(
        output,
        "legacy_archive_imports_removed: {}",
        report.legacy_archive_imports_removed
    )?;
    writeln!(
        output,
        "normalized_listening_events_retained: {}",
        report.normalized_listening_events_retained
    )?;
    writeln!(
        output,
        "evidence_imports_retained: {}",
        report.evidence_imports_retained
    )?;
    writeln!(output, "database_writes: disabled")?;
    writeln!(output, "provider_requests: disabled")?;
    writeln!(output, "approval_required: exact_plan_sha256")?;
    Ok(())
}

fn write_database_cleanup_verification(
    output: &mut impl Write,
    report: &db_cleanup::CleanupVerification,
) -> Result<()> {
    writeln!(output, "cleanup_verification: database-v2-clean-runtime-v1")?;
    writeln!(output, "plan_sha256: {}", report.plan_sha256)?;
    writeln!(output, "invariant_sha256: {}", report.invariant_sha256)?;
    writeln!(
        output,
        "live_invariant_matches_cleanup_instant: {}",
        report.invariant_matches_receipt
    )?;
    writeln!(
        output,
        "legacy_tables_absent: {}",
        report.legacy_tables_absent
    )?;
    writeln!(
        output,
        "provider_import_staging_empty: {}",
        report.import_staging_empty
    )?;
    writeln!(
        output,
        "normalized_listening_events: {}",
        report.normalized_listening_events
    )?;
    writeln!(
        output,
        "minimum_normalized_listening_events: {}",
        report.minimum_normalized_listening_events
    )?;
    writeln!(output, "evidence_imports: {}", report.evidence_imports)?;
    writeln!(
        output,
        "minimum_evidence_imports: {}",
        report.minimum_evidence_imports
    )?;
    writeln!(output, "verified_at: {}", report.verified_at.to_rfc3339())?;
    writeln!(output, "verified: {}", report.verified)?;
    writeln!(output, "provider_requests: disabled")?;
    Ok(())
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

fn binary_capability_manifest() -> crate::contract::BinaryCapabilityManifest {
    use crate::contract::{
        BINARY_CAPABILITY_SCHEMA_VERSION, BinaryCapabilityManifest,
        CAPABILITY_ARTWORK_CARRY_FORWARD, CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT,
        CAPABILITY_BULK_MAINTENANCE_PREVIEW, CAPABILITY_DIRECT_MANAGED_INTAKE,
        CAPABILITY_DURABLE_OPERATIONS, CAPABILITY_ENUMERATED_PLAYLIST_ADDITIONS,
        CAPABILITY_MAINTENANCE_INTAKE_AUDIT, CAPABILITY_MAINTENANCE_INTAKE_WORKFLOW,
        CAPABILITY_MAINTENANCE_TASK_SESSION, CAPABILITY_PLAN_ORIGIN, CAPABILITY_PRODUCT_IDENTITY,
        CAPABILITY_PROVIDER_BASELINE, CAPABILITY_PROVIDER_CREDENTIAL_VAULT,
        CAPABILITY_PROVIDER_ORDER_INTENT, CAPABILITY_REMOTE_CLI, CAPABILITY_SPIN_PUBLICATION_PLAN,
        CAPABILITY_UNIFIED_MAINTENANCE_WORKFLOW, CapabilityAvailability, ContractVersionRange,
    };

    BinaryCapabilityManifest {
        schema_version: BINARY_CAPABILITY_SCHEMA_VERSION,
        binary_version: env!("CARGO_PKG_VERSION").to_owned(),
        contract_versions: ContractVersionRange::exact(crate::contract::CONTRACT_VERSION),
        capabilities: std::collections::BTreeMap::from([
            (
                CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PRODUCT_IDENTITY.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PROVIDER_CREDENTIAL_VAULT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_DURABLE_OPERATIONS.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_REMOTE_CLI.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_ARTWORK_CARRY_FORWARD.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_BULK_MAINTENANCE_PREVIEW.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_ENUMERATED_PLAYLIST_ADDITIONS.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_DIRECT_MANAGED_INTAKE.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_MAINTENANCE_INTAKE_AUDIT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_MAINTENANCE_INTAKE_WORKFLOW.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_MAINTENANCE_TASK_SESSION.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PROVIDER_ORDER_INTENT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PROVIDER_BASELINE.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                crate::contract::CAPABILITY_SAVED_INTAKE_DISPOSITION.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_UNIFIED_MAINTENANCE_WORKFLOW.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PLAN_ORIGIN.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_SPIN_PUBLICATION_PLAN.to_owned(),
                CapabilityAvailability::Available,
            ),
        ]),
    }
}

fn write_intake_audit(output: &mut impl Write, report: &intake::IntakeAudit) -> Result<()> {
    use intake::IntakeState;

    let count = |state| {
        report
            .items
            .iter()
            .filter(|item| item.state == state)
            .count()
    };
    writeln!(output, "intake audit: current")?;
    writeln!(output, "snapshot_id: {}", report.snapshot_id)?;
    writeln!(
        output,
        "proposal_generation_id: {}",
        report
            .proposal_generation_id
            .map_or_else(|| "-".to_owned(), |value| value.to_string())
    )?;
    writeln!(
        output,
        "proposal_state: {}",
        report.proposal_state.as_deref().unwrap_or("-")
    )?;
    writeln!(output, "items: {}", report.items.len())?;
    writeln!(
        output,
        "already_covered: {}",
        count(IntakeState::AlreadyCovered)
    )?;
    writeln!(
        output,
        "direct_managed_addition: {}",
        count(IntakeState::DirectManagedAddition)
    )?;
    writeln!(
        output,
        "previously_excluded: {}",
        count(IntakeState::PreviouslyExcluded)
    )?;
    writeln!(
        output,
        "assigned_approved: {}",
        count(IntakeState::AssignedApproved)
    )?;
    writeln!(
        output,
        "suggested_in_draft: {}",
        count(IntakeState::SuggestedInDraft)
    )?;
    writeln!(
        output,
        "known_from_history: {}",
        count(IntakeState::KnownFromHistory)
    )?;
    writeln!(
        output,
        "genuinely_new: {}",
        count(IntakeState::GenuinelyNew)
    )?;
    writeln!(output, "spotify_writes: disabled")?;
    writeln!(
        output,
        "state\ttrack\tartists\tsources\tcurrent_destinations\tproposal_destinations\trecommended_destination\trecommendation_reason\tevents\tplays\texclusion_history\texclusion_reason\tspotify_id\tsaved_track_disposition"
    )?;
    for item in &report.items {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            item.state.as_str(),
            clean_cell(&item.title),
            clean_cell(&item.artists),
            clean_cell(&item.sources.join(" / ")),
            clean_cell(&item.current_destinations.join(" / ")),
            clean_cell(&item.proposal_destinations.join(" / ")),
            clean_cell(item.recommended_destination.as_deref().unwrap_or("-")),
            clean_cell(item.recommendation_reason.as_deref().unwrap_or("-")),
            item.listening_events,
            item.play_count,
            item.exclusion_history,
            clean_cell(item.active_exclusion_reason.as_deref().unwrap_or("-")),
            item.spotify_id,
            item.saved_track_disposition.as_deref().unwrap_or("-")
        )?;
    }
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
    writeln!(output, "plan_origin: {}", report.origin.as_str())?;
    writeln!(
        output,
        "proposal_generation_id: {}",
        report
            .proposal_generation_id
            .map_or_else(|| "-".to_owned(), |value| value.to_string())
    )?;
    writeln!(output, "source_snapshot_id: {}", report.source_snapshot_id)?;
    writeln!(output, "operations: {}", report.operation_count)?;
    writeln!(output, "creates: {}", report.creates)?;
    writeln!(output, "renames: {}", report.renames)?;
    writeln!(output, "reorders: {}", report.reorders)?;
    writeln!(output, "additions: {}", report.additions)?;
    writeln!(output, "restorations: {}", report.restorations)?;
    writeln!(output, "artwork_uploads: {}", report.artwork_uploads)?;
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

fn write_apply_report(output: &mut impl Write, report: &apply::ApplyReport) -> Result<()> {
    writeln!(output, "spotify apply: {}", report.status)?;
    writeln!(output, "apply_run_id: {}", report.apply_run_id)?;
    writeln!(output, "plan_id: {}", report.plan_id)?;
    writeln!(output, "assessment_id: {}", report.assessment_id)?;
    writeln!(output, "phase: {}", report.phase)?;
    writeln!(output, "resumed: {}", report.resumed)?;
    writeln!(output, "operations: {}", report.operation_count)?;
    writeln!(output, "succeeded: {}", report.succeeded_count)?;
    writeln!(output, "failed: {}", report.failed_count)?;
    writeln!(output, "started_at: {}", report.started_at.to_rfc3339())?;
    writeln!(
        output,
        "next: run `chordrift sync pull` before another phase"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser;

    use super::{
        AlbumCommand, ApplyPhaseArg, ArtworkCommand, BehavioralSignalArg, BookmarkCommand,
        ClassificationCommand, Cli, ClusterCommand, Command, DbCleanupCommand, DbCommand,
        DbCompactCommand, DbV2Command, DbV2MigrationCommand, EmbeddingCommand, EnrichmentCommand,
        HistoryCommand, IntakeCommand, LikedDispositionArg, LikedSongsPolicyArg, PlaylistCommand,
        PlaylistRoleArg, PlaylistSignalClassArg, ProductAuditMode, ProductCollectionCommand,
        ProductCommand, ProductOnboardingCommand, ProductRecipeCommand, ProductSpinCommand,
        ReevaluateCommand, RouteCommand, SavedAlbumPolicyArg, ServiceCommand,
        ServiceLibraryCommand, ServiceMaintenanceCommand, SignalCommand, SpotifyCommand,
        SyncCommand, TrackCommand, binary_capability_manifest, format_count, format_elapsed,
        write_status,
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
    fn parses_machine_readable_capability_requirements() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "capabilities",
            "--require",
            "maintenance.intake-workflow.v1",
            "--require",
            "plan-origin.v1",
        ])
        .expect("valid capability handshake");
        assert!(matches!(
            cli.command,
            Command::Capabilities { required }
                if required == ["maintenance.intake-workflow.v1", "plan-origin.v1"]
        ));

        let manifest = binary_capability_manifest();
        assert!(manifest.supports("maintenance.intake-workflow.v1"));
        assert!(manifest.supports("maintenance.enumerated-playlist-additions.v1"));
        assert!(manifest.supports("spin-publication-plan.v1"));
        assert!(manifest.supports("service.remote-cli.v1"));
        assert!(!manifest.supports("spin-publication.v1"));
        serde_json::to_string(&manifest).expect("capability manifest serializes");
    }

    #[test]
    fn parses_authenticated_remote_service_query() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "service",
            "query",
            "--url",
            "https://api.chordrift.example",
            "--file",
            "query.json",
        ])
        .expect("valid remote service query");
        assert!(matches!(
            cli.command,
            Command::Service {
                command: ServiceCommand::Query { profile, .. }
            } if profile == "default"
        ));
    }

    #[test]
    fn parses_typed_remote_library_comparison() {
        let connection_id = uuid::Uuid::new_v4();
        let cli = Cli::try_parse_from([
            "chordrift",
            "service",
            "library",
            "compare",
            "--url",
            "https://api.chordrift.example",
            "--provider-connection-id",
            &connection_id.to_string(),
        ])
        .expect("valid remote library comparison");
        assert!(matches!(
            cli.command,
            Command::Service {
                command: ServiceCommand::Library {
                    command: ServiceLibraryCommand::Compare {
                        provider_connection_id,
                        profile,
                        ..
                    }
                }
            } if provider_connection_id == connection_id && profile == "default"
        ));
    }

    #[test]
    fn parses_typed_remote_maintenance_commands() {
        let connection_id = uuid::Uuid::new_v4();
        let cli = Cli::try_parse_from([
            "chordrift",
            "service",
            "maintenance",
            "start",
            "--url",
            "https://api.chordrift.example",
            "--provider-connection-id",
            &connection_id.to_string(),
        ])
        .expect("valid remote maintenance start");
        assert!(matches!(
            cli.command,
            Command::Service {
                command: ServiceCommand::Maintenance {
                    command: ServiceMaintenanceCommand::Start {
                        provider_connection_id,
                        profile,
                        ..
                    }
                }
            } if provider_connection_id == connection_id && profile == "default"
        ));

        let session_id = uuid::Uuid::new_v4();
        let review_id = uuid::Uuid::new_v4();
        let cli = Cli::try_parse_from([
            "chordrift",
            "service",
            "maintenance",
            "authorize",
            "--url",
            "https://api.chordrift.example",
            "--session-id",
            &session_id.to_string(),
            "--expected-revision",
            "4",
            "--review-id",
            &review_id.to_string(),
        ])
        .expect("valid exact maintenance authorization");
        assert!(matches!(
            cli.command,
            Command::Service {
                command: ServiceCommand::Maintenance {
                    command: ServiceMaintenanceCommand::Authorize {
                        session_id: parsed_session,
                        review_id: parsed_review,
                        expected_revision: 4,
                        ..
                    }
                }
            } if parsed_session == session_id && parsed_review == review_id
        ));
    }

    #[test]
    fn parses_read_only_intake_audit() {
        let cli = Cli::try_parse_from(["chordrift", "intake", "audit", "--account", "personal"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Intake {
                command: IntakeCommand::Audit { account }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_explicit_liked_intake_disposition() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "intake",
            "liked-disposition",
            "--account",
            "personal",
            "--spotify-id",
            "track-1",
            "--disposition",
            "clear-after-verified-assignment",
            "--reason",
            "already placed",
        ])
        .expect("valid saved-intake decision");
        assert!(matches!(
            cli.command,
            Command::Intake {
                command: IntakeCommand::LikedDisposition {
                    account,
                    spotify_id,
                    disposition: LikedDispositionArg::ClearAfterVerifiedAssignment,
                    reason,
                }
            } if account == "personal" && spotify_id == "track-1" && reason == "already placed"
        ));
    }

    #[test]
    fn parses_the_consistent_product_rehearsal_surface() {
        let capture = Cli::try_parse_from([
            "chordrift",
            "product",
            "onboarding",
            "capture",
            "--fixture",
            "onboarding.json",
            "--mode",
            "enriched",
        ])
        .expect("valid capture command");
        assert!(matches!(
            capture.command,
            Command::Product {
                command: ProductCommand::Onboarding {
                    command: ProductOnboardingCommand::Capture {
                        mode: ProductAuditMode::Enriched,
                        ..
                    }
                }
            }
        ));

        let collections = Cli::try_parse_from([
            "chordrift",
            "product",
            "collections",
            "list",
            "--account",
            "00000000-0000-0000-0000-000000000001",
        ])
        .expect("valid collection command");
        assert!(matches!(
            collections.command,
            Command::Product {
                command: ProductCommand::Collections {
                    command: ProductCollectionCommand::List { .. }
                }
            }
        ));

        let recipe = Cli::try_parse_from([
            "chordrift",
            "product",
            "recipes",
            "execute",
            "--fixture",
            "spin.json",
        ])
        .expect("valid recipe command");
        assert!(matches!(
            recipe.command,
            Command::Product {
                command: ProductCommand::Recipes {
                    command: ProductRecipeCommand::Execute { .. }
                }
            }
        ));

        let spin = Cli::try_parse_from([
            "chordrift",
            "product",
            "spins",
            "show",
            "--account",
            "00000000-0000-0000-0000-000000000001",
            "--spin",
            "00000000-0000-0000-0000-000000000002",
        ])
        .expect("valid Spin command");
        assert!(matches!(
            spin.command,
            Command::Product {
                command: ProductCommand::Spins {
                    command: ProductSpinCommand::Show { .. }
                }
            }
        ));
    }

    #[test]
    fn parses_database_v2_reports_and_compaction_plan() {
        let invariant = Cli::try_parse_from(["chordrift", "db", "invariant-report"])
            .expect("valid invariant command");
        assert!(matches!(
            invariant.command,
            Command::Db {
                command: DbCommand::InvariantReport { account }
            } if account == "personal"
        ));

        let storage = Cli::try_parse_from(["chordrift", "db", "storage-report"])
            .expect("valid storage command");
        assert!(matches!(
            storage.command,
            Command::Db {
                command: DbCommand::StorageReport
            }
        ));

        let compact = Cli::try_parse_from(["chordrift", "db", "compact", "plan"])
            .expect("valid compaction command");
        assert!(matches!(
            compact.command,
            Command::Db {
                command: DbCommand::Compact {
                    command: DbCompactCommand::Plan { account }
                }
            } if account == "personal"
        ));

        let cleanup = Cli::try_parse_from([
            "chordrift",
            "db",
            "compact",
            "cleanup",
            "apply",
            "--confirm",
            "abc",
        ])
        .expect("valid cleanup command");
        assert!(matches!(
            cleanup.command,
            Command::Db {
                command: DbCommand::Compact {
                    command: DbCompactCommand::Cleanup {
                        command: DbCleanupCommand::Apply { account, confirm }
                    }
                }
            } if account == "personal" && confirm == "abc"
        ));

        let v2 = Cli::try_parse_from(["chordrift", "db", "v2", "status"])
            .expect("valid v2 status command");
        assert!(matches!(
            v2.command,
            Command::Db {
                command: DbCommand::V2 {
                    command: DbV2Command::Status { account }
                }
            } if account == "personal"
        ));

        let migration = Cli::try_parse_from([
            "chordrift",
            "db",
            "v2",
            "migration",
            "apply",
            "--confirm",
            "abc",
        ])
        .expect("valid v2 migration command");
        assert!(matches!(
            migration.command,
            Command::Db {
                command: DbCommand::V2 {
                    command: DbV2Command::Migration {
                        command: DbV2MigrationCommand::Apply { account, confirm }
                    }
                }
            } if account == "personal" && confirm == "abc"
        ));

        let cutover = Cli::try_parse_from(["chordrift", "db", "v2", "cutover-plan"])
            .expect("valid v2 cutover plan command");
        assert!(matches!(
            cutover.command,
            Command::Db {
                command: DbCommand::V2 {
                    command: DbV2Command::CutoverPlan { account }
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_reevaluate_export() {
        let cli =
            Cli::try_parse_from(["chordrift", "reevaluate", "export", "--file", "review.csv"])
                .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Reevaluate {
                command: ReevaluateCommand::Export { account, file }
            } if account == "personal" && file == std::path::Path::new("review.csv")
        ));
    }

    #[test]
    fn parses_reevaluate_retirement() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "reevaluate",
            "retire",
            "--confirm",
            "RETIRE RE-EVALUATE",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Reevaluate {
                command: ReevaluateCommand::Retire { account, confirm }
            } if account == "personal" && confirm == "RETIRE RE-EVALUATE"
        ));
    }

    #[test]
    fn parses_atomic_cohort_classification() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "classify",
            "set",
            "--spotify-id",
            "first",
            "--spotify-id",
            "second",
            "--cohort",
            "ar-rahman-favorites",
            "--reason",
            "reviewed",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Classify {
                command: ClassificationCommand::Set { spotify_ids, cohorts, .. }
            } if spotify_ids == ["first", "second"] && cohorts == ["ar-rahman-favorites"]
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
    fn parses_temporary_liked_songs_policy() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "spotify",
            "library-policy",
            "--liked-songs",
            "clear-after-verified-assignment",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Spotify {
                command: SpotifyCommand::LibraryPolicy {
                    account,
                    liked_songs: LikedSongsPolicyArg::ClearAfterVerifiedAssignment,
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_saved_album_review_policy() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "albums",
            "policy",
            "--mode",
            "review-then-unsave",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Albums {
                command: AlbumCommand::Policy {
                    account,
                    mode: SavedAlbumPolicyArg::ReviewThenUnsave,
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_saved_album_archive_and_history() {
        let policy =
            Cli::try_parse_from(["chordrift", "albums", "policy", "--mode", "archive-only"])
                .expect("valid command");
        assert!(matches!(
            policy.command,
            Command::Albums {
                command: AlbumCommand::Policy {
                    account,
                    mode: SavedAlbumPolicyArg::ArchiveOnly,
                }
            } if account == "personal"
        ));
        let history =
            Cli::try_parse_from(["chordrift", "albums", "history"]).expect("valid command");
        assert!(matches!(
            history.command,
            Command::Albums {
                command: AlbumCommand::History { account }
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
    fn parses_provider_state_acceptance() {
        let cli =
            Cli::try_parse_from(["chordrift", "sync", "accept-current"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::AcceptCurrent { account }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_neon_only_route_creation() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "routes",
            "create",
            "--name",
            "South Indian",
            "--description",
            "Corrective regional inbox",
            "--background",
            "background.png",
            "--artwork",
            "artwork.png",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Routes {
                command: RouteCommand::Create { account, name, .. }
            } if account == "personal" && name == "South Indian"
        ));
    }

    #[test]
    fn parses_batch_route_addition() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "routes",
            "add",
            "--route",
            "Decide Later",
            "--spotify-id",
            "track-a",
            "--spotify-id",
            "track-b",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Routes {
                command: RouteCommand::Add { spotify_ids, .. }
            } if spotify_ids == ["track-a", "track-b"]
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
    fn parses_apply_readiness_with_read_only_probe() {
        let cli = Cli::try_parse_from(["chordrift", "sync", "readiness", "--probe"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::Readiness {
                    account,
                    plan: None,
                    probe: true
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_apply_readiness_inspection() {
        let cli =
            Cli::try_parse_from(["chordrift", "sync", "readiness-show"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::ReadinessShow {
                    account,
                    assessment: None
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_exact_publish_apply() {
        let id = uuid::Uuid::new_v4();
        let cli = Cli::try_parse_from([
            "chordrift",
            "sync",
            "apply",
            "--assessment",
            &id.to_string(),
            "--phase",
            "publish",
            "--confirm",
            &id.to_string(),
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::Apply {
                    phase: ApplyPhaseArg::Publish,
                    allow_destructive: false,
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_provider_free_publish_preflight() {
        let cli =
            Cli::try_parse_from(["chordrift", "sync", "apply-preflight"]).expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Sync {
                command: SyncCommand::ApplyPreflight {
                    account,
                    plan: None
                }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_exact_artwork_approval() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "artwork",
            "approve",
            "--confirm",
            "ca81d1b2-e56b-41e6-8846-cdb379cb039b",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Artwork {
                command: ArtworkCommand::Approve { account, .. }
            } if account == "personal"
        ));
    }

    #[test]
    fn parses_focused_artwork_update() {
        let cli = Cli::try_parse_from(["chordrift", "artwork", "update", "--playlist", "Inbox"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Artwork {
                command: ArtworkCommand::Update { account, playlist }
            } if account == "personal" && playlist == "Inbox"
        ));
    }

    #[test]
    fn parses_on_demand_artwork_render() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "artwork",
            "render",
            "--background",
            "background.png",
            "--title",
            "Made for Suhail",
            "--output",
            "made-for-suhail.png",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Artwork {
                command: ArtworkCommand::Render { title, .. }
            } if title == "Made for Suhail"
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
    fn parses_track_inspection_with_artist_disambiguation() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "tracks",
            "inspect",
            "--name",
            "Do Your Best",
            "--artist",
            "John Maus",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Tracks {
                command: TrackCommand::Inspect {
                    account,
                    name: Some(name),
                    artist: Some(artist),
                    spotify_id: None,
                    technical: false
                }
            } if account == "personal" && name == "Do Your Best" && artist == "John Maus"
        ));
    }

    #[test]
    fn parses_exclusion_archive_commands() {
        let list =
            Cli::try_parse_from(["chordrift", "tracks", "exclusions"]).expect("valid command");
        assert!(matches!(
            list.command,
            Command::Tracks {
                command: TrackCommand::Exclusions { account }
            } if account == "personal"
        ));
        let empty = Cli::try_parse_from([
            "chordrift",
            "tracks",
            "empty-exclusions",
            "--confirm",
            "personal",
        ])
        .expect("valid command");
        assert!(matches!(
            empty.command,
            Command::Tracks {
                command: TrackCommand::EmptyExclusions { account, confirm }
            } if account == "personal" && confirm == "personal"
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
    fn parses_targeted_bookmark_refresh() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "bookmarks",
            "refresh",
            "--spotify-id",
            "1128mckrHSNSNt3PzyE4Bp",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Bookmarks {
                command: BookmarkCommand::Refresh {
                    account,
                    spotify_id: Some(id),
                    ..
                }
            } if account == "personal" && id == "1128mckrHSNSNt3PzyE4Bp"
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
    fn parses_protected_retirement_selection() {
        let cli = Cli::try_parse_from([
            "chordrift",
            "playlists",
            "retirement",
            "--all",
            "--except",
            "Road Trip Order",
        ])
        .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Playlists {
                command: PlaylistCommand::Retirement {
                    account,
                    all: true,
                    except,
                    ..
                }
            } if account == "personal" && except == ["Road Trip Order"]
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

    #[test]
    fn renders_human_sync_counts_and_timings() {
        assert_eq!(format_count(149_350), "149,350");
        assert_eq!(format_count(-1_790), "-1,790");
        assert_eq!(format_elapsed(Duration::from_millis(842)), "842 ms");
        assert_eq!(format_elapsed(Duration::from_millis(1_250)), "1.2 s");
    }
}
