# Hosted private beta — V021-06

Status: in progress. Nothing in this document marks the hosted service ready
for provider writes or unrestricted public registration.

## Selected topology

```text
browser / remote CLI
        |
        | HTTPS · chordrift.suhail.ink
        v
Nexus · existing Nginx + wildcard TLS + Tailscale subnet router
        |
        | private LAN · 10.214.90.10 -> 10.214.90.35:8787
        v
Vortex · non-root read-only Chordrift API container
        |
        +---- Neon PostgreSQL
        +---- Auth0 / Google OIDC
        `---- Spotify through a short-lived encrypted-vault credential lease
```

Nexus is ingress. Vortex is compute. Neon remains the durable application
database. Vortex does not receive a second public listener and does not need a
second Tailscale installation because Nexus already routes the private subnet.

## Identity and existing-account adoption

Google proves an external identity through Auth0. Chordrift owns the durable
product subject, account membership, revocable product session, provider
connection, and data. Spotify is never a login method.

The first private-beta login may match one deployment-configured, Google-
verified email address. That match is only a bootstrap gate: Chordrift binds
the verified OIDC issuer/subject to the already-existing Chordrift account
through `provision_account_owner`. Later sessions resolve the stable
issuer/subject binding. The bootstrap email setting can then be removed.

The adoption operation must not create a new music account, rewrite the
existing `chordrift_account_id`, re-import Spotify, or modify Spotify. It adds
identity/session rows around the existing ownership boundary.

## Browser boundary

The Rust service serves a deliberately small HTML/CSS/JavaScript contract
workbench. JavaScript submits only the same typed compatibility, command, and
query DTOs used by other clients. It cannot submit a CLI string, shell command,
SQL, provider URL, database credential, or provider credential.

The OIDC authorization-code exchange and Chordrift session stay server-side.
The browser receives an `HttpOnly`, `Secure`, `SameSite=Lax` Chordrift cookie.
Cookie-authenticated DTO calls additionally require the exact public Origin and
an application-specific request header. CLI authentication uses a separate
public Auth0 Native application and the OAuth 2.0 Device Authorization Flow.
Auth0 hosts verification and consent; Chordrift exchanges the resulting
verified issuer credential for the same `Authorization: Bearer` product
session contract. CLI clients never rely on browser cookies, expose a client
secret, or run a browser callback listener.

## Container boundary

The repository is checked out on the build host and copied into a disposable
multi-stage builder. The runtime image contains the release server binary and
CA roots only; static assets are compiled into the binary. It contains no Git
history, Rust compiler, source tree, Cargo cache, `.env`, or build credential.

The Vortex container runs as UID/GID 65532, drops every Linux capability, uses
`no-new-privileges`, has a read-only root filesystem and bounded tmpfs, binds
only the Vortex private IPv4 address, has bounded log rotation/resources, and
uses the Rust binary itself for liveness checks.

## Current deployed checkpoint

The authenticated private-beta service is deployed at
`https://chordrift.suhail.ink`. Google login adopts the existing Chordrift
account, browser sessions survive container replacement, and the provider-aware
explorer presents Spotify observation state separately from the Chordrift
model. Playlist order, track detail, personal listening statistics and active
exclusions are available through tenant-scoped typed queries.

Playlist membership rows and active exclusions also carry album, meaningful
play count, and last-heard time. The thin browser may sort playlist rows by
custom order, plays, recency, album, or title and may group exclusions by
album, prior playlist, or last-heard bucket. These are presentation operations
over Rust-issued facts. Restoring an exclusion remains an explicit Rust command:
reuse the prior destination only when its stable surface still exists, otherwise
request a destination or classification assistance, then show an exact provider
review. Permanently forgetting an exclusion is a different destructive intent
change with an explicit confirmation. Provider artwork is deliberately deferred
until post-beta UI design work.

A canonical provider-track identity answers *which recording is this?*; it does
not answer *where should it live?* Intake placement recommendations therefore
come only from retained accepted Chordrift placement evidence. If exactly one
prior destination still exists in the latest model, the Rust contract supplies
it as a recommended resolution with a client-safe reason. The browser may
preselect that exact surface, but the session remains unresolved until the user
records the decision. Multiple or absent destinations remain blank. A future
classification authority may add scored evidence without moving this policy or
consent boundary into a client.

The service and local CLI now target the same canonical `chordrift_cutover`
database on the single Neon `main` branch. It is at migration 51/51. The
temporary pre-cutover and rehearsal branches, plus the stale duplicate
`chordrift` database, were deleted after external logical backups and client
health checks passed. Rehearsal fixture identities, revoked fixture
credentials, and fixture operations were not promoted. First-owner login and
encrypted import of the existing Spotify refresh authorization must be
reverified on this canonical database before beta.1. Provider-backed
observation, durable worker composition and the maintenance task are now wired
through the real Rust adapter. Provider writes are limited to server-rederived,
enumerated maintenance effects behind an immutable exact review. A newly
selected placement must be added, freshly observed, and verified before a
separate review may consume Liked Songs or another intake surface. The service
must fail closed rather than disguising a test backend or legacy shell workflow
as hosted authority.

## Storage and rehearsal policy

The durable topology is one Neon project, one `main` branch, and one canonical
application database. At the consolidation gate the PostgreSQL database was
approximately 195 MB. Its largest durable components are normalized listening
history (about 81 MB), revisioned playlist membership (about 35 MB), verified
playlist baselines (about 15 MB), and synchronization receipts (about 9 MB).
Those are product evidence, not branch duplication.

For the private beta the lossless policy is intentionally conservative:
normalized listening evidence, accepted intent, exclusions, current anchors,
exact reviews, write receipts, and named recovery checkpoints remain durable.
Routine import staging is transaction-local and must be empty after every
successful pull. Superseded full inventory generations are eligible only after
their current anchor, named-release dependency, pending review, apply receipt,
and restore proof have all been preserved; derived caches may be regenerated.
The read-only compaction plan rehearses this classification without deleting
production rows. No age-only deletion is enabled for beta.1.

Prefer disposable local PostgreSQL for migration and restore rehearsal. When
Neon branch semantics are specifically being tested, set an expiry at branch
creation, record the proof, and delete the branch immediately afterward. A
temporary Neon branch must never become an undocumented service dependency.
Logical backups live outside Neon and are not a reason to retain a permanent
backup branch on the free plan.

The complete product/browser acceptance surface is tracked in
[Web workflow capability matrix](WEB_WORKFLOW_CAPABILITY_MATRIX.md).

## Deployment gates

1. Build and test from the exact Git commit; record the image digest.
2. Back up Neon and prove the backup restores into an isolated target.
3. **Complete:** project `royal-snow-31539822` is the only Chordrift Neon
   project. Its canonical database is at schema 0050 and is shared by local and
   hosted transports. The former legacy project, temporary rehearsal branches,
   and stale duplicate database are deleted.
4. Configure Auth0 as a Regular Web Application with Google login and callback
   `https://chordrift.suhail.ink/auth/callback`. Configure a second Native
   Application with Token Endpoint Authentication Method `None`, OIDC
   conformance enabled, Google connection enabled, and the Device Code grant
   type. Store its public client ID as `device_client_id` on the existing
   1Password Auth0 item; no Native application client secret exists.
5. Start the Vortex API with provider writes limited to server-rederived,
   enumerated effects behind an immutable exact review. Readiness must report
   `exact_review_only`; no general publication or client-supplied write exists.
6. Validate the Nexus configuration before reload, then verify HTTPS headers,
   liveness, readiness, OIDC state/PKCE, first-owner adoption, logout, session
   revocation, and cross-account denial.
7. Import the existing Spotify refresh credential into the encrypted vault only
   after database restore proof and identity adoption succeed. **Pending
   canonical re-verification:** the prior rehearsal proof is retained, but its
   fixture rows were deliberately not copied into the canonical database.
8. Exercise provider reads and ordinary maintenance against fake fixtures and
   then the personal account in read-only mode.
9. Provider writes require a new explicit gate after Suhail reviews the exact
   beta behavior.
