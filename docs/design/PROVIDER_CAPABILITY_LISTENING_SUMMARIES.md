# Capability-aware listening summaries

Status: planned, non-critical follow-up after the current `v0.2.1` daily-driver
hardening. This document defines the product and Rust boundary; it does not
claim that the query or UI is implemented.

## Product outcome

After a provider refresh, Chordrift should show a compact listening summary:

- the tracks most often observed as played in that refresh window, sorted by
  observation count and then most-recent observation;
- the strongest available skip and completion evidence, only when the selected
  provider connection has actually supplied it;
- a small default head (for example, five tracks per section); and
- **Show more**, backed by a paginated query over the same immutable refresh or
  evidence batch rather than a browser-side reconstruction.

The summary must state its window, source, freshness, and completeness. “Since
the last provider refresh,” “through the latest imported archive,” and “not
available from this provider” are materially different claims and must never be
collapsed into one unlabeled count.

## Provider-neutral contract

The domain core consumes normalized listening evidence. It must not consume a
Spotify Recently Played payload, an Apple Music response, or provider-specific
field names. Each provider/evidence adapter maps its available source data into
the shared concepts below:

| Normalized concept | Meaning |
| --- | --- |
| `playback_observation` | The provider reported that a recording was played at a time. |
| `played_duration` | A duration explicitly supplied by the source. Absence is unknown, not zero. |
| `skip_evidence` | An explicit provider/source assertion that playback was skipped. |
| `completion_evidence` | An explicit provider/source assertion, or a documented deterministic derivation from supplied duration fields. |
| `playback_context` | A provider-normalized context such as playlist, album, radio, or unknown. |
| `evidence_batch` | The immutable provider refresh, archive import, or other bounded acquisition that supplied the facts. |

Every normalized fact carries the Chordrift account, provider connection,
provider-qualified recording identity, event time, evidence batch, provenance,
and source capability. Optional values remain optional all the way through the
Rust DTO. Adapters may retain provider extensions for diagnostics, but recipes,
queries, and clients cannot depend on those extensions.

## Capability and availability model

The existing `ProviderCapability` and `EvidenceCapability` vocabulary remains
the gate. A summary reports availability per signal, not one all-or-nothing
provider flag:

- `recent_playback`: available, degraded, unavailable, or unknown;
- `played_duration` / extended playback history;
- `skips`;
- `completion`; and
- playback context when a later typed capability is warranted.

Each reported signal also includes:

- `source_kind` (provider API, provider export, user import, or another explicit
  adapter source);
- `observed_at` and the covered event interval;
- whether the interval is complete, bounded, sampled, or provider-defined;
- the last successful acquisition/batch identity; and
- a human-safe limitation suitable for every client.

A provider adapter can therefore enrich Chordrift differently without changing
the meaning of the core. Adding Apple Music means implementing and proving its
capability mapping from Apple-authorized data. It does not mean translating
Apple fields into fictitious Spotify fields or claiming parity that the API
does not provide.

## Current Spotify mapping

Spotify's current Recently Played integration supplies bounded playback
observations, timestamps, track identity, and context. It does **not** supply
played duration, skip, or completion for those observations. Consequently:

- the latest-refresh panel may count and rank recent observations;
- those observations update event count and last-heard time;
- they must not be presented as duration-qualified “meaningful plays”; and
- they must not create skip or completion facts.

Spotify Extended Streaming History is a separate optional evidence adapter. It
can supply durable duration, skip, completion, and lifetime evidence, but its
summary must be labeled with the archive's coverage end—not “since the last
refresh.” A later archive may supersede overlapping provisional API
observations according to the existing deduplication rules.

No Apple Music mapping is specified yet. Its eventual adapter must be based on
the data and terms available to the authorized integration at implementation
time, with unsupported signals reported honestly.

## Required Rust and storage work

1. Introduce an adapter-facing normalized listening-evidence port whose input
   is the shared evidence vocabulary plus an explicit capability report.
2. Give every newly inserted observation an exact immutable evidence-batch
   association. The existing Spotify sync ledger records refresh boundaries,
   but does not yet provide a first-class client query over exact per-sync
   membership.
3. Add a provider-neutral query such as `ListeningSummary` scoped by the
   authenticated account and an owned provider connection. The server selects
   the latest successful batch by default; clients never supply SQL, provider
   URLs, or source table names.
4. Return a versioned DTO containing the window/provenance, per-signal
   availability, head rows, total distinct-track counts, and opaque pagination
   cursors for **Show more**.
5. Keep “recent observations” separate from lifetime/archive aggregates.
   Missing duration or skip evidence stays `None`; zero means an observed zero.
6. Render the same DTO in Web and CLI. Future native clients reuse it without
   reimplementing aggregation or capability interpretation.

## Acceptance and fake-provider coverage

The deterministic provider harness must cover at least:

- recent observations available while skips and duration are unavailable;
- explicit skip/completion evidence available from a different batch source;
- a provider with neither capability;
- repeated plays of one track within one batch and deterministic ranking;
- pagination that is stable for an immutable batch;
- refresh retry/idempotency without duplicate observations;
- capability degradation and stale/partial coverage labels;
- two provider connections whose evidence cannot cross account boundaries; and
- identical Web/CLI DTO semantics.

The UI must never substitute “not skipped” for “skip data unavailable,” combine
windows without labeling them, or infer dislike from a skip. These are evidence
facts for explanation and later recipes, not automatic classification truth.
