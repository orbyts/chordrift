# Playlist product architecture

The zoomable [product architecture overview](playlist-product-architecture.svg)
shows the first-run journey, the intended database boundaries, and the matching
Rust domain types. The companion
[portable core and native clients overview](client-core-platform-architecture.svg)
shows how the CLI and future native applications consume the same Rust-owned
behavior. Existing database-v2 names remain unchanged. Migration 0046 now
implements the additive recipe, collection, surface, onboarding, and Spin table
names shown here, but remains isolated from production Neon.

Status: active v0.2 architecture, updated 2026-08-27. V020-01 through V020-10
are implemented; V020-11 is next. This document is a design contract, not
authorization to apply a migration or write to a provider.

## Current implementation status

- **Implemented — V020-01:** the public Rust `contract` module defines semantic
  contract/schema compatibility, capability negotiation, provider-neutral
  command and query envelopes, immutable receipts and generic views,
  request/operation/idempotency/cancellation identities, structured progress,
  complete operation lifecycle states, and fixed-message client-safe errors.
- **Implemented — V020-02:** every existing CLI handler passes through one Rust
  application facade while preserving commands, redirected output, interactive
  presentation, errors, database behavior, and provider behavior exactly.
- **Implemented — V020-03:** the public `domain` module defines account-owned
  and provider-qualified IDs, typed provider/evidence capability reports,
  collection membership strength/provenance/confidence, independent playlist-
  surface axes, recipe-v1 values, and account-bound Spin identities. Validated
  deserialization cannot bypass ownership or value invariants.
- **Implemented — V020-04:** a deterministic test-only fake application/provider
  harness proves two-account and two-provider-namespace isolation, idempotent
  acceptance, cooperative cancellation, bounded retry, stable fake generation,
  and visible unsupported-capability failure without production provider calls.
- **Implemented — V020-05:** migration 0046 reconciles and adds 16 product-
  domain tables with composite account ownership, v0.1.4 provider-account
  compatibility, and links to existing inventory/publication history. Fresh and
  migration-45 upgrade rehearsals pass on isolated PostgreSQL 18 only.
- **Implemented — V020-06:** the public onboarding application boundary reads
  one selected immutable inventory and optional extended evidence through a
  mutation-free provider port, persists content-addressed migration-0046 inputs
  and provenance, returns idempotent retries without another provider read, and
  rejects capability/account violations before provider access.
- **Implemented — V020-07:** the public inventory-audit query reads only the
  captured immutable current inventory and returns deterministic library,
  overlap, capability, uncertainty, and unapproved preserve-first starter-
  organization values without a provider call or database write.
- **Implemented — V020-08:** the enriched query preserves the same inventory
  baseline, binds one selected history import by account, fingerprint, kind, and
  record count, and returns only directly supported listening conclusions with
  exact support counts and remaining inference limits.
- **Implemented — V020-09:** the provider-neutral recipe executor canonicalizes
  immutable inputs, allocates source seats, reserves cadence/section capacity,
  enforces eligibility and repetition/artist budgets, and reports capability
  degradation as a deterministic unordered draft without provider or database
  access.
- **Implemented — V020-10:** the Spin preview boundary verifies the unordered
  draft and capability snapshot, assigns exact deterministic one-based order,
  persists full seeds and structured selection/ordering reasons in migration
  0046, and returns the same account-scoped view on replay without a provider
  action.
- **Next — V020-11:** expose the implemented onboarding, collection, recipe, and
  Spin-preview path through a consistent development CLI rehearsal.
- **Not implemented yet:** approved collection authoring, CLI rehearsal,
  publication integration, hosted transport, and native clients.
  Those remain separate roadmap slices and must not be inferred from the
  existence of contract or domain types.

## Product boundary

Chordrift is a durable personal-library orchestrator, not a replacement for a
provider's catalog, radio, playback, or discovery engine. Providers supply
catalog access, user-library observations, playback observations where
available, and publication surfaces. Chordrift preserves intent and evidence,
turns them into renewable listening experiences, and explains every choice.

First-run setup should therefore be useful with only OAuth and a current
library inventory. Optional listening-history archives improve lifetime
rotation, rediscovery, completion, and skip evidence, but never gate basic use.
The product must state which evidence is live, accumulating, imported, or
unavailable rather than pretending that all providers expose the same facts.

One `chordrift_account` is the product ownership boundary and may eventually
connect several `provider_accounts`. Migration 0046 adds this parent while a
compatibility trigger preserves unchanged v0.1.4 provider-account upserts. The
first implementation supports Spotify only, but
Spotify payloads stop at the adapter boundary. Provider-qualified identities
map to provider-neutral music identities using explicit evidence; IDs from
different providers are never matched by string coincidence.

## First-run journey

1. Create or open a Chordrift account.
2. Choose a provider and authorize it through provider-owned OAuth.
3. Capture the complete current library inventory and provider capabilities.
4. Show a read-only library audit: surfaces, tracks, overlap, available
   evidence, and uncertainty.
5. Offer optional archive import as an enrichment step, not a prerequisite.
6. Let the user either preserve existing playlists and begin learning, or
   preview a default starter organization. The default is always preserve.
7. Save approved collection boundaries and playlist recipes as durable intent.
8. Generate a provider-free **Spin** preview with selection and ordering
   explanations.
9. Publish only an approved immutable plan through the provider adapter, then
   verify the observed provider state.
10. Accumulate listening evidence and explicit corrections so later Spins
    improve without silently changing hard user rules.

Generated names and artwork are proposals attached to a playlist surface.
They are optional, revisioned, and approved independently from the recipe.
An LLM may propose language and art direction, but it never owns collection
membership, eligibility, ordering, or publication authority.

## Portable core and native clients

Chordrift adopts the useful layers from Photara's architecture without its
node packages, proxy graph, or exact third-party runtime registry. Chordrift
has one product domain and a small set of controlled infrastructure adapters,
so a plugin runtime would add complexity without solving a current problem.

The architectural rule is **one portable Rust product, several thin clients**:

- the CLI is the first client, not the product core;
- the macOS client uses native SwiftUI and the current native Apple design
  language, including Liquid Glass where the running OS supports it;
- the Windows client uses its own native Windows presentation and integration;
- a future Linux client may use a separate native shell without changing
  recipes, persistence rules, or provider behavior;
- native clients own presentation, accessibility, navigation, platform window
  behavior, OAuth handoff, notifications, and secure storage of their Chordrift
  session credential;
- Rust owns accounts, identity, inventory, evidence, collections, recipes,
  Spins, publication safety, migrations, background work, and diagnostics.

The shippable authority is a hosted Rust service. It owns the Neon connection
and encrypted provider authorization; neither is distributed in a desktop
binary. Native applications and the installed CLI authenticate to that service.
During development, the CLI may invoke the same application service through an
in-process transport, but it receives no separate business path.

### Client contract

Clients consume a versioned command/query/event contract rather than SQL,
provider payloads, terminal output, or internal domain structs:

```text
Commands   request work: connect, observe, create session, preview Spin,
           approve publication, cancel work

Queries    return immutable views: onboarding audit, collections, recipe,
           Spin preview, operation history, diagnostics

Events     report lifecycle: queued, running, progress, waiting for consent,
           completed, failed, cancelled, recoverable
```

Every connection negotiates API version, database schema version, provider
capabilities, evidence capabilities, and feature availability. This is the
small useful analogue of Photara's exact-runtime declaration: it prevents a
client from displaying or invoking a feature its service, provider, or evidence
cannot support. It is not a general plugin system.

The implemented contract foundation is transport-neutral. The Rust CLI can
eventually call it in process;
native applications can use an authenticated service protocol. A generated
Swift or Windows binding may wrap that protocol, but UniFFI or any one binding
tool must not become the domain boundary. The contract—not the transport—is
authoritative.

### Rust crate direction

The eventual workspace should separate these responsibilities without a
big-bang rewrite. V020-01 begins as a public module in the existing crate; the
names below are logical dependency boundaries, not claims that these workspace
crates already exist:

```text
chordrift-domain       pure IDs, invariants, collections, recipes, Spins
chordrift-application  commands, queries, transactions, authorization policy
chordrift-contract     versioned client DTOs, events, errors, compatibility
chordrift-storage      repository ports and PostgreSQL/Neon implementation
chordrift-providers    provider ports and Spotify implementation
chordrift-service      auth, API transport, jobs, scheduling, migrations
chordrift-client       reusable Rust client used by CLI and binding generators
chordrift-cli          terminal presentation only
```

Cross-cutting contracts cover account isolation, secret handling, structured
diagnostics, tracing, cancellation, idempotency, deterministic generation,
schema/API compatibility, performance budgets, backup, and recovery.

### Native client boundary

The platform shells do not decide which tracks qualify, calculate weights,
order a Spin, interpret a provider deletion, or generate a provider mutation.
They render Rust-supplied views and issue Rust-defined commands. Platform-only
code is expected for:

- SwiftUI/AppKit presentation and Liquid Glass availability on macOS;
- Windows-native presentation and lifecycle integration;
- OAuth browser launch and callback routing;
- application session storage in Keychain, Windows Credential Manager, or a
  future Linux secret service;
- provider deep links, file pickers, notifications, accessibility, and updater
  integration.

The service stores provider refresh credentials in its encrypted server-side
vault. The client credential store contains only the user's Chordrift session;
clients never receive the Neon owner URL.

## Core concepts

### Collections are the library map

A collection is an unordered, dynamic set answering “what musical world does
this belong to?” Collections may be nested for navigation and overlap in
membership. For example, `A. R. Rahman` may be shown beneath `South Indian`
while a recording also belongs to `Indian`, a film-score collection, and a
personal childhood-favorites fact.

Hierarchy is therefore presentation, not exclusivity. Separation is expressed
by a playlist recipe: one recipe may isolate A. R. Rahman; another may draw a
small allocation from that collection into a broader Indian experience.

Membership has provenance, confidence, and strength:

- explicit user boundary or correction;
- approved reusable user rule;
- reliable provider or external fact;
- learned affinity proposed from repeated evidence;
- unresolved proposal awaiting review.

Hard user boundaries outrank learned evidence. Chordrift can propose a rule but
cannot silently promote a pattern into durable policy.

### Recipes describe renewable experiences

A recipe revision is an immutable specification with:

- collection and evidence sources;
- allocation lanes such as discovery, emerging, familiar, dormant, and
  recovery;
- eligibility and hard exclusions;
- repetition, duration, artist-spacing, and cross-output budgets;
- familiarity cadence, which distributes anchors rather than merely counting
  them;
- an ordering narrative such as shuffle, smooth transitions, intentional
  contrast, or warm-up / focus / landing sections.

Weights allocate seats; cadence distributes them. Setting the familiar lane to
zero produces discovery-only output. Setting a familiar cadence of roughly one
in four places familiar anchors periodically rather than clumping them.

Selection and ordering are separate stages. The selector casts eligible tracks
into lanes; the orderer directs the resulting sequence. A Spin stores both the
selected set and final order, with a concise reason for every track.

### Playlist surfaces describe ownership

Authority, purpose, and refresh behavior are independent fields, not one large
playlist enum:

- authority: provider, user, Chordrift, or collaborative;
- purpose: intake, collection view, renewable experience, utility, or bookmark;
- refresh: untouched, monitored, manual Spin, scheduled later, or provider
  controlled.

This represents user playlists, provider mixes, pure Chordrift outputs, and
collaborative outputs without multiplying special cases. Collaborative
playlists keep explicit user-pinned tracks as directives while Chordrift
regenerates the remaining seats.

## Database organization

The existing database-v2 current-state, evidence, canonical identity, and audit
tables remain the foundation. Migration 0046 adds account-scoped intent and
generation tables beside them rather than replacing the clean v2 schema. The
[physical reconciliation](PRODUCT_SCHEMA_V020_05.md) records every distinction.

| Boundary | Representative tables | Purpose |
| --- | --- | --- |
| Account and provider links | `chordrift_accounts`, `provider_accounts`, `provider_capability_observations` | Product ownership, provider connection identity, and an honest capability snapshot. Secrets remain outside PostgreSQL. |
| Current provider state | `provider_current_inventories`, `provider_current_playlists`, content-addressed revision tables | One verified current inventory per provider account without repeating unchanged membership. |
| Music identity | `tracks`, `artists`, `albums`, provider identities and matches | Provider-neutral recordings plus evidence-backed provider mappings. |
| Permanent evidence | `normalized_listening_events`, `historical_provider_track_identities`, `listening_evidence_imports`, source files | Append-oriented listening ledger and optional archive manifests. |
| Library map and intent | `library_collections`, `collection_relationships`, `collection_rule_revisions`, `track_collection_membership_revisions` | Overlapping collections, navigational hierarchy, rules, provenance, corrections, and review state. |
| Playlist surfaces | `playlist_surfaces`, `playlist_surface_provider_links`, `playlist_track_directives` | Authority, purpose, refresh policy, stable provider targets, and user-pinned inclusions/exclusions. |
| Recipes and Spins | `playlist_recipes`, `playlist_recipe_revisions`, `playlist_recipe_dependencies`, `playlist_spins`, `playlist_spin_tracks` | Immutable composition specifications, capability dependencies, deterministic previews, reasons, and exact order. |
| Publication audit | existing sync plans/apply receipts/verifications plus `playlist_spin_publications` | Connect an approved Spin to the existing plan/apply/verify safety boundary. |
| Rebuildable intelligence | statistics, signals, embeddings, recommendation generations | Versioned caches derived from permanent evidence and durable intent; eligible for retention. |

These are now the physical migration-0046 names. Their ownership and
relationships match the Rust domain. Recipe documents use versioned JSON for
evolvable composition details, while queryable identity, revision, dependency,
order, provenance, and audit fields stay typed and normalized. Existing
provider observation and publication tables retain their v0.1.4 meanings.

## Rust domain boundary

Rust owns policy. SQL rows and provider payloads are infrastructure types and
must not leak into recipes or UI DTOs.

```text
AccountContext
  ├─ ChordriftAccountId
  └─ ProviderConnection + ProviderCapabilities

ProviderAdapter trait
  ├─ authorize / refresh authorization
  ├─ observe inventory and recent evidence
  ├─ publish an approved provider plan
  └─ verify and build provider deep links

LibraryMap
  ├─ Collection
  ├─ CollectionRelationship
  ├─ CollectionRuleRevision
  └─ TrackMembershipRevision

PlaylistSurface
  ├─ Authority
  ├─ Purpose
  ├─ RefreshPolicy
  └─ TrackDirective

RecipeRevision
  ├─ SourceLane + Allocation
  ├─ EligibilityRule + Guardrail
  ├─ FamiliarityCadence
  └─ OrderingNarrative + Section

Spin
  ├─ EvidenceCapabilities
  ├─ SpinTrack + SelectionReason
  ├─ exact final order
  └─ deterministic seed and input fingerprint

PublicationPlan → Approval → ApplyReceipt → Verification
```

The corresponding module direction is:

```text
domain/
  accounts.rs       provider-neutral account context
  music.rs          canonical and provider-qualified identity value types
  collections.rs    library map, hierarchy, membership, and rule revisions
  surfaces.rs       authority, purpose, refresh, and collaboration directives
  recipes.rs        typed recipe specification and validation
  spins.rs          selection, ordering, explanations, and deterministic output
  publication.rs    immutable plan/apply/verify domain contracts

application/
  onboarding.rs     inventory audit and preserve-or-organize starter plan
  spin.rs           compile recipe, select, order, persist, and preview
  publish.rs        turn an approved Spin into the existing safety workflow

providers/
  mod.rs            ProviderAdapter and capability contract
  spotify/          OAuth, inventory, evidence, writes, verification, deep links

storage/
  repositories.rs   domain-facing repository traits
  postgres/         SQLx records and repository implementations

interface/
  dto.rs            stable UI/CLI query and command shapes
```

This earlier module sketch maps into the crate boundaries above. It is a target
dependency direction, not a requirement for a disruptive directory rewrite.
Existing modules should move behind these seams incrementally with compiling
tests at every step.

## v0.2 product invariants

- Basic onboarding works from OAuth plus current inventory alone.
- Optional archives enrich results but never become a hidden prerequisite.
- One account may connect multiple providers without merging their credentials,
  current inventories, or personal actions.
- Every provider account, collection, surface, recipe, Spin, and user decision
  resolves to exactly one Chordrift ownership boundary.
- Collections overlap; their hierarchy never forces exclusive membership.
- A playlist surface references a recipe revision; it does not embed mutable
  recipe state.
- Every Spin is reproducible from an immutable recipe revision, evidence
  capabilities, input fingerprint, and seed.
- Every generated track has a selection reason and an ordering reason.
- A user-pinned inclusion or hard exclusion survives regeneration.
- Provider writes still require preview, immutable plan, approval, apply, and
  verification.
- Generated names and artwork cannot authorize provider publication.
- Listening evidence and explicit user corrections remain permanent;
  statistics, embeddings, and unreferenced candidate generations remain
  rebuildable.
- Unsupported provider capabilities degrade visibly and safely.
- CLI and native clients receive identical decisions through the same versioned
  application contract.
- No shipped client contains a Neon connection or provider refresh credential.
- Client disconnect, retry, or duplicate submission cannot duplicate a Spin or
  provider apply; commands are idempotent and resumable.
- A client can display progress, cancellation, recovery, and capability gaps
  without parsing logs or terminal prose.

## CLI-first new-account rehearsal

The earliest product acceptance goal is intentionally UI-free. An onboarding
session will treat the current personal provider inventory and optional history
as if they had just been supplied by a new account. It may read that evidence
but must ignore existing Chordrift collection, recipe, and publication intent
unless explicitly selected as comparison data.

The session produces a read-only audit, evidence/capability report, proposed
starter collections, default playlist surfaces, and one or more provider-free
Spin previews. It writes no Spotify state. Accepting a starter organization or
publishing a Spin remains a later explicit plan/apply/verify action.

The same acceptance suite runs twice: inventory-only and inventory plus
extended history. Results may improve with history, but both paths must produce
an honest usable experience.

## Recommended implementation sequence

1. **Complete:** establish the versioned command/query/event, compatibility,
   progress, cancellation, and structured-error contract.
2. **Complete:** route the existing CLI through the application facade without
   changing behavior.
3. **Complete:** freeze provider-neutral IDs, `ProviderCapabilities`, playlist-
   surface axes, collection membership strength, and recipe-v1 types in Rust
   with unit tests.
4. **Complete:** add account/provider isolation, idempotency, cancellation, and
   fake-provider tests; prove two accounts and provider namespaces cannot cross.
5. **Complete:** design and rehearse additive ownership, collection, surface,
   recipe, Spin, onboarding-session, and publication-link migration 0046. No
   production migration or provider change occurred.
6. **Complete:** capture immutable onboarding inputs and produce inventory-only
   and enriched provider-free audit results. CLI exposure remains V020-11.
7. **Complete:** implement initial **Discovery + Rediscovery** selection with
   allocation, cadence capacity, eligibility, budgets, and section capacity.
8. **Complete:** persist and display the deterministic exact Spin order,
   fingerprints, capability snapshot, seed, and per-track explanations.
9. Rehearse the complete provider-write-free product path through the CLI.
10. Connect approved Spins to the existing immutable sync plan/apply/verify
   boundary.
11. Introduce the hosted Rust service and authenticated client transport without
   distributing Neon or provider refresh credentials.
12. Build native clients over the stable contract only after isolation,
   compatibility, and deterministic-preview tests pass.
