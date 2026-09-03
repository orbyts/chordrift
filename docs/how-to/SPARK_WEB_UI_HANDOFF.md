# Spark Web UI handoff

Use this document for a narrow Chordrift Web UI polish task. It intentionally
omits the repository's release history, database history, provider internals,
and most Rust implementation detail so a small-context client can spend its
context on the browser experience.

## Assignment

Polish the existing Chordrift Web UI without changing product behavior. The
current release is `v0.2.1-beta.13`, contract `1.6`, schema `52`. Web and remote
CLI are thin clients of the same Rust-owned application contract.

The sole canonical local checkout is `$CRATES/chordrift` (currently
`/Users/suhail/Library/CloudStorage/Dropbox/matrix/crates/chordrift`). Confirm
that path before editing. Do not work in a duplicate checkout under Documents,
ChatGPT, Music, or `/private/tmp`.

This is a presentation task. Do not modify Rust, migrations, Docker/Compose
architecture, authentication, provider integration, persistence, workflow
semantics, or the wire contract. Each accepted Web UI update is nevertheless
rebuilt into the existing production-shaped image and restarted on Vortex so
the user can review the real hosted asset.

## Architecture boundary

```text
Browser skin                         Remote CLI skin
  HTML/CSS + small JavaScript          Rust presentation adapter
             \                         /
              versioned command/query DTOs
                         |
                Rust application authority
          decisions, revisions, allowed actions,
          exact reviews, durable operation lifecycle
                         |
              Rust domain + infrastructure ports
          PostgreSQL, credential vault, worker, Spotify
```

The browser may authenticate, submit an existing typed command or query,
poll an operation, render server-provided state, collect a choice from the
allowed choices, and submit that choice unchanged. It may not:

- interpret Spotify/provider deltas;
- decide where a track belongs;
- assemble or reorder provider effects;
- infer whether authorization is required;
- sequence maintenance safety phases;
- accept shell commands, SQL, credentials, tokens, or provider URLs; or
- replace Rust behavior with JavaScript behavior.

## Files in scope

Primary Web UI files:

- `web/index.html` — semantic page structure and static copy;
- `web/app.css` — layout, typography, responsive behavior, states and polish;
- `web/app.js` — browser adapter and rendering. Change presentation carefully;
  do not change routes, DTO construction, workflow transitions, IDs or action
  semantics;
- `web/maintenance-decisions.js` — pure mapping of selected controls to existing
  Rust DTO variants. Treat its DTO shapes as frozen;
- `web/library-explorer.js` — pure client-side sorting/grouping of views already
  returned by Rust; and
- `tests/web_maintenance_decisions.mjs` and
  `tests/web_library_explorer.mjs` — browser-helper regression harnesses.

Documentation and screenshots directly describing the Web UI are also in
scope.

Files outside editing scope include `src/**/*.rs`, `migrations/**`,
`Cargo.toml`, `Cargo.lock`, container/Compose files, deployment scripts,
secrets, release metadata and tags. Do not edit those files. `Dockerfile` and
`deploy/vortex/compose.yml` may be read and used by the established delivery
loop below; they are not Web UI design surfaces.

## Current browser surfaces

- Chordrift login/logout through Rust-owned Auth0 routes.
- Spotify connection selection plus connect, reconnect and disconnect through
  Rust-owned provider routes.
- Maintain: start observation, show durable progress, render ambiguous changes,
  record decisions, refresh, and authorize one exact displayed review.
- Library: switch between provider observation and Chordrift model, explain
  comparison counts, inspect playlists, ordered tracks and track details.
- Excluded: inspect, sort and group the reversible archive. Restore/forget are
  not yet browser actions.
- Activity: inspect durable hosted operations.
- Developer: a temporary typed-contract lab. It is not a command line, SQL
  console, arbitrary HTTP client, or credential viewer.

## Contract rules Spark must preserve

- The Rust server injects `__CHORDRIFT_CONTRACT_VERSION__`. Never add a literal
  contract version to JavaScript or HTML.
- Existing transport routes are fixed: `/v1/compatibility`, `/v1/commands`,
  `/v1/queries`, and the Rust-owned `/auth/...` and `/providers/...` routes.
- Treat every resource ID, session ID, operation ID, revision and review ID as
  opaque. Return it unchanged and never derive authority from it.
- Render `allowed_actions`; do not invent an action from the session state.
- Render `recommended_resolution` only as a preselected suggestion. It is not
  user consent and must remain changeable before submission.
- Submit the full Rust-issued `MaintenanceSurfaceView` (`surface_id` and `name`)
  already attached to a playlist. Do not reconstruct it from a playlist key or
  visible label.
- Bind resolve/refresh/authorize to the displayed `session_id` and `revision`.
  Authorization also requires the exact displayed `review_id`.
- Provider effects are displayed exactly as returned by Rust. The UI must not
  combine, omit, rewrite, or invent effects.
- `failed`, `cancelled`, `completed`, and `recoverable` stop operation polling.
  A recoverable state is a safe pause, not an automatic retry.
- A provider write remains default-no and requires an explicit confirmation of
  the exact human-readable effects.

The browser currently uses these command variants: `start_maintenance`,
`refresh_maintenance`, `resolve_maintenance`, `authorize_maintenance`,
`cancel_operation`, and `disconnect_provider`. Its read views use
`provider_connections`, `operation`, `maintenance_session`,
`library_playlists`, `library_playlist_tracks`, `library_track`,
`excluded_tracks`, `library_comparison`, and `operation_history`.

Do not add, remove, rename or reshape any variant or field in a UI-polish task.

The authenticated, same-origin `/auth/session` response now includes these
additional optional presentation fields:

- `display_name: string | null`
- `avatar_url: string | null`

Spark may consume those exact fields to render the requested top-right account
avatar/menu. Use a semantic image with the display name in its alternative text
when `avatar_url` is present; otherwise render an accessible initials/default
fallback. Keep logout available inside the account affordance, preserve full
keyboard navigation and visible focus, and do not use email as a fallback. Do
not infer, fetch, persist, transform, proxy, or rewrite identity data in the
browser. Existing sessions may return null until the user completes one fresh
Chordrift login.

## UX and safety invariants

- Signed out shows a login action. Signed in shows logout and does not also
  show login.
- The footer keeps the Rust-rendered `Chordrift v…` build identity visible.
  Never replace it with a JavaScript or HTML version literal. Use it to confirm
  the hosted preview is serving the expected container binary.
- Provider identity and connection state remain visible on every music view.
- Clearly distinguish the newest recorded provider observation from Chordrift's
  current model; neither is silently called live provider state.
- “Check provider changes” is read-only. It does not authorize a write.
- Record-only provider gestures can converge without provider authorization.
- A Chordrift-authored provider mutation appears as one exact review and is not
  applied until the user confirms it.
- Loading, empty, unavailable, recoverable, validation and failure states must
  remain visible and understandable; visual polish must not hide them.
- Preserve keyboard access, focus visibility, labels, semantic controls,
  readable contrast and responsive behavior.
- Do not perform live provider mutations while visually testing unless the user
  explicitly requests that exact acceptance test.

## When to stop and return to the Rust task

Do not work around a missing capability in the browser. Stop and write a short
**Rust escalation note** when the desired UI requires a new query/command,
field, action, policy, workflow transition, provider effect, authentication
change, database change, or server route. Include:

1. the current visible behavior;
2. the desired visible behavior;
3. the smallest missing server/DTO capability;
4. any response JSON or client-safe error already observed; and
5. the UI files that would consume the capability after Rust supplies it.

Do not implement the missing capability in JavaScript.

## Verification

Run after every coherent Web UI change:

```bash
node --check web/app.js
node --check web/maintenance-decisions.js
node --check web/library-explorer.js
node tests/web_maintenance_decisions.mjs
node tests/web_library_explorer.mjs
git diff --check
```

If request construction, route use, DTO fields, or maintenance behavior would
need to change, stop and produce a Rust escalation note instead of updating the
contract or Rust tests.

## Hosted preview after every accepted UI update

The Web assets are compiled into `chordrift-server`; copying files to Vortex is
not a deployment. After a coherent UI update and the checks above pass:

1. Confirm the diff contains only the allowed Web UI, Web helper test and
   directly relevant documentation files. Commit it so the image has an exact
   source identity.
2. On Vortex, build the existing root `Dockerfile` through
   `deploy/vortex/compose.yml` from that exact commit. Use a unique preview tag
   derived from the commit (for example `webui-<short-sha>`), the full commit as
   `CHORDRIFT_VCS_REF`, the current RFC3339 time as `CHORDRIFT_BUILD_DATE`, and
   the existing host-only secrets file as `CHORDRIFT_SECRETS_FILE`.
3. Recreate **both** `api` and `worker` from the same newly built image. Even a
   UI-only update must not leave the API and worker on different image
   identities.
4. Verify Compose reports both services healthy/running, the image's OCI
   revision label equals the commit, and both public endpoints pass:
   `https://chordrift.suhail.ink/health/live` and `/health/ready`.
5. Perform a non-mutating browser smoke: signed-out/login visibility as
   applicable, provider context, navigation, layout, and read-only library
   rendering. Do not initiate observation, disconnect a provider, record a
   decision, or authorize a provider effect merely to test visual polish.
6. Report the preview tag, full commit, image digest and health results with the
   UI handoff.

Use the existing Compose assembly; do not run `docker compose down`, prune
images/volumes, regenerate secrets, run migrations manually, alter networking,
or change the Nexus proxy. If build or health verification fails, preserve logs
and restore both services to the previously recorded image together.

This preview loop does not create a public product release. Do not bump a
version, tag Git, publish crates, reinstall the CLI, or change release notes.
Those cross-client release actions return to the Rust/release task after the UI
is accepted.

At handoff, list changed files, visual/interaction changes, tests run, known UI
limitations, any Rust escalation notes, and the hosted preview identity and
health. Do not tag, publish crates, install binaries, or change deployment
architecture during this Web-only task.

## Branch freshness

Start the Web task from the current `main` containing beta.13. Do not continue
the earlier `codex/v021-06-private-beta` beta.1 checkout and do not cherry-pick
its `web/app.js`: that stale wrapper hard-codes contract 1.5 and predates later
disconnect, maintenance and build-identity fixes. Presentation ideas from that
work may be reapplied selectively on current main only after preserving the
current request construction and workflow behavior.
