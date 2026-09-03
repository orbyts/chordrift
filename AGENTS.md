# Chordrift agent guide

## Canonical checkout

Work only in `$CRATES/chordrift`. On Suhail's current machines this resolves to
the Dropbox-synchronized `matrix/crates/chordrift` repository. Before editing,
print the resolved path, fetch GitHub, and confirm the work begins at current
`origin/main`. Do not create or use duplicate Chordrift repositories under
Documents, ChatGPT, Music, or `/private/tmp`.

Keep `main` releasable. Use a short `codex/...` branch for a coherent change,
then merge the verified result back to `main` and remove the merged branch.
Never discard uncommitted or unpushed work while cleaning branches.

## Architecture boundary

Rust owns product behavior, authentication, provider access, DTOs, persistence,
maintenance policy, and durable operations. The CLI and Web UI are thin clients
of that same contract. A Web-only task may refine presentation in `web/` and its
small JavaScript harnesses but must not recreate Rust behavior in JavaScript.

For a narrow Web task, read `docs/how-to/SPARK_WEB_UI_HANDOFF.md` and
`docs/how-to/SPARK_WEB_UI_PROMPT.md`. Stop and return a Rust escalation note if
the requested behavior needs a new field, command, query, route, action, policy,
workflow transition, provider effect, authentication change, or persistence.

## Verification and delivery

Run the repository checks appropriate to the change. Every accepted hosted Web
update is committed, built into the existing root Dockerfile through
`deploy/vortex/compose.yml`, and deployed to both Vortex `api` and `worker` from
the same exact commit and image. Verify the OCI revision plus public liveness
and readiness. Do not copy loose Web files to the server, expose secrets, run
manual production migrations, prune Docker state, or mutate Spotify merely for
a visual smoke test.

Only a release task may bump versions, tag, publish crates, reinstall the CLI,
or replace the numbered Vortex release image. A release must use one immutable
tagged source identity for GitHub, crates.io, the installed CLI, and both hosted
containers.
