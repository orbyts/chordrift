# Playlist product architecture

The zoomable [product architecture overview](playlist-product-architecture.svg)
shows the first-run journey, the intended database boundaries, and the matching
Rust domain types. It is deliberately conceptual: existing database-v2 names
remain unchanged, while the recipe and collection names describe the next
additive foundation rather than an already-applied migration.

Status: proposed foundation, 2026-08-26. This is a design contract, not
authorization to apply a migration or write to a provider.

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
connect several `provider_accounts`. The current personal schema starts at a
provider account, so the product schema needs this small parent boundary before
multi-user onboarding. The first implementation supports Spotify only, but
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
tables remain the foundation. The next schema work should add account-scoped
intent and generation tables rather than replace the clean v2 schema.

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

These are the target names for the proposed boundaries. Migration design must
first reconcile them with equivalent existing tables so it extends rather than
duplicates a concept; the overview must be updated to the exact physical names
before migration approval. Their ownership and relationships should not
change. Recipe documents may use versioned JSON for evolvable composition
details, while queryable identity, revision, dependency, order, provenance,
and audit fields stay typed and normalized.

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

This is a target dependency direction, not a requirement for a disruptive
directory rewrite. Existing modules should move behind these seams
incrementally with compiling tests at every step.

## Invariants for the next foundation

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

## Recommended implementation sequence

1. Freeze provider-neutral IDs, `ProviderCapabilities`, playlist-surface axes,
   collection membership strength, and recipe-v1 types in Rust with unit tests.
2. Add the account/provider isolation test harness and a fake provider adapter;
   prove two accounts and two provider namespaces cannot cross.
3. Design and rehearse one additive migration for collections, surfaces,
   recipes, Spins, and publication links. Do not publish provider changes.
4. Implement a read-only onboarding audit and provider-free starter-plan/Spin
   preview against the existing personal account.
5. Implement the initial **Discovery + Rediscovery** recipe with allocation,
   cadence, and simple ordering sections.
6. Connect approved Spins to the existing immutable sync plan/apply/verify
   boundary.
7. Build the thin native UI over stable query/command DTOs only after those
   contracts pass the account-isolation and deterministic-preview tests.
