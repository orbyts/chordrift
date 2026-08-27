# Product schema foundation — V020-05

This document records the exact additive schema introduced by migration
`0046_product_domain_foundation.sql`. It is the physical database companion to
the provider-neutral Rust domain model and the
[playlist product architecture](PLAYLIST_PRODUCT_ARCHITECTURE.md).

Status: implemented and rehearsed on isolated PostgreSQL 18 on 2026-08-27.
Migration 0046 is present on the v0.2 development line only. It has not been
applied to production Neon and does not change the released v0.1.4 runtime.

## Reconciliation with the existing schema

The v0.1.4 database already contains mature observation, analysis, proposal,
and publication history. Migration 0046 preserves those meanings instead of
renaming or copying them into superficially newer tables.

| Existing boundary | Decision in V020-05 |
| --- | --- |
| `provider_accounts` | Retained as the provider connection and root of the v0.1.4 runtime. It now has one non-null `chordrift_account_id`. |
| `provider_current_*`, provider revision tables, checkpoints, runtime views | Retained as current provider observations and immutable inventory bodies. Onboarding may reference a checkpoint; it does not duplicate inventory. |
| `tracks`, `artists`, `albums`, provider identities | Retained as the canonical music graph and provider mappings. Collection membership and Spin order reference `tracks`. |
| `playlist_concepts`, `playlist_generations`, `playlists`, name/artwork revisions | Retained as existing provider-account canonical-output lineage and approved proposal history. A library collection is broader provider-neutral intent, so it is not a renamed playlist concept. |
| `provider_account_playlists`, bookmarks, routing surfaces, Re-evaluate events | Retained as provider observations and v0.1.4 surface policy/history. A product playlist surface can link to an observed provider playlist without replacing these records. |
| assignment, exclusion, classification, signal, cluster, and embedding tables | Retained as durable decisions or rebuildable evidence. They may inform later collection/onboarding work but are not rewritten by this migration. |
| `sync_runs`, readiness assessments, apply runs, and managed verifications | Retained as the publication safety ledger. A Spin publication stores typed links into this existing chain. |

This split avoids two common errors: treating every old playlist concept as a
general musical collection, and creating a second plan/apply engine for Spins.

## Exact additive tables

Migration 0046 adds 16 tables in six groups.

| Group | Tables | Durable responsibility |
| --- | --- | --- |
| Ownership and capability | `chordrift_accounts`, `provider_capability_observations` | Provider-neutral ownership and immutable, honest provider/evidence capability snapshots. |
| Library map | `library_collections`, `collection_relationships`, `collection_rule_revisions`, `track_collection_membership_revisions` | Overlapping collections, navigational relationships, approved rules, membership strength, provenance, confidence, and correction history. |
| Playlist surfaces | `playlist_surfaces`, `playlist_surface_provider_links`, `playlist_track_directives` | Independent authority/purpose/refresh axes, stable provider targets, and explicit include/exclude/pin directives. |
| Recipes | `playlist_recipes`, `playlist_recipe_revisions`, `playlist_recipe_dependencies` | Stable recipe identity, immutable recipe-v1 documents, and queryable collection/evidence/provider-capability dependencies. |
| Onboarding and Spins | `onboarding_sessions`, `playlist_spins`, `playlist_spin_tracks` | Reproducible session inputs/provenance and provider-free Spin identity, seed, capability snapshot, exact order, and explanations. Runtime behavior begins in later slices. |
| Publication link | `playlist_spin_publications` | Account-safe connection from an approved Spin/surface to the existing plan, readiness, apply, and verification records. |

## Ownership enforcement

`chordrift_accounts` is the product boundary. `provider_accounts` remains the
provider connection and now belongs to exactly one product account. Every new
account-owned parent has a composite unique key `(chordrift_account_id, id)`;
child tables use composite foreign keys so an ID from another account cannot be
attached even if the individual UUID exists.

The same pattern applies across:

- collection relationships, rules, and memberships;
- surfaces, provider links, and directives;
- recipes, revisions, and dependencies;
- onboarding capability/checkpoint inputs;
- Spins, recipe revisions, and onboarding sessions;
- Spin publications, surfaces, provider accounts, and the existing publication
  ledger.

Canonical `tracks` remain globally shareable music identities. The account-
owned membership, directive, or Spin row supplies the personal context.

Provider namespaces are also checked structurally. A surface link must use the
namespace declared by its selected `provider_account`, and a new account-scoped
`sync_run` must use that same namespace. Equal opaque IDs from different
providers therefore never become equal database targets.

## v0.1.4 compatibility bridge

The released import path inserts or upserts `provider_accounts` without a
`chordrift_account_id`. Migration 0046 backfills existing rows and installs a
small `BEFORE INSERT` compatibility trigger:

1. If the caller supplies an owner, it is preserved.
2. If a matching provider/account-label connection already exists, its owner is
   reused before the legacy `ON CONFLICT` path runs.
3. Otherwise, one stable compatibility owner is derived from the provider and
   local account label and inserted once.

This keeps the existing SQL valid, prevents retry/upsert from creating duplicate
orphan owners, and leaves future product code free to create an explicit
Chordrift account before connecting one or more providers.

## Typed and evolvable storage

Queryable identity, ownership, revision, order, lifecycle, capability
dependency, strength, provenance, and surface axes are normalized columns with
checks and foreign keys. Evolvable rule, recipe, reason, capability, input, and
provenance documents use JSON objects with an explicit schema-version or typed
parent row.

Important details include:

- recipe schema version 1 and the Rust lane/axis vocabularies are checked in SQL;
- individual allocation weight zero is valid;
- membership confidence is bounded from 0 through 10,000 basis points;
- Spin seeds support the complete unsigned 64-bit range;
- active memberships and directives are unique while superseded revisions stay
  durable;
- onboarding defaults to ignoring existing Chordrift intent and authorizes no
  provider write;
- a Spin publication reuses, rather than bypasses, plan/readiness/apply/verify.

## Isolated PostgreSQL 18 rehearsal

The integration suite proves two paths on PostgreSQL 18:

1. A fresh database applies all 46 migrations, exposes every new table, accepts
   an unchanged v0.1.4 provider-account upsert, and reports zero pending or
   failed migrations on replay.
2. An isolated schema applies migrations 1–45, inserts a preexisting provider
   account and playlist concept, records the existing table count, then applies
   migration 46. The old row remains, exactly 16 tables are added, every
   provider account receives a non-null owner, and the whole rehearsal rolls
   back.

The fresh-path assertions also create a valid capability observation,
onboarding session, collection dependency, recipe revision, Spin, playlist
surface, provider link, sync plan, and Spin publication. Deliberate cross-account
collection, capability, and Spin inserts are rejected by PostgreSQL.

No production Neon request, provider request, provider write, credential change,
or CLI/runtime behavior is part of V020-05. V020-06 subsequently added the
provider-read-only onboarding application boundary against these tables without
changing migration 0046; see [its focused design record](ONBOARDING_SESSION_V020_06.md).
