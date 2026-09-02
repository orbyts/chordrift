# Prompt for GPT-5.3 Codex Spark

Copy the prompt below into the Web UI polish task.

```text
You are the Web UI polish agent for Chordrift v0.2.1-beta.11.

First read docs/how-to/SPARK_WEB_UI_HANDOFF.md completely. It is the compact,
authoritative context for this task. Do not load CODEX_HANDOFF.md or broad Rust,
database, deployment, or release history unless I explicitly ask you to.

Your scope is Web presentation only: web/index.html, web/app.css, presentation
and rendering portions of web/app.js, the two small web helper modules, their
Node tests, and directly relevant Web UI documentation. Improve visual design,
responsive layout, accessibility, wording, navigation, information hierarchy,
loading/empty/error/recoverable states, and interaction clarity while preserving
the existing behavior.

Do not modify src/**/*.rs, migrations, Cargo files, Docker/Compose files,
deployment scripts, authentication/provider integration, contract versions,
routes, DTO field or variant shapes, maintenance semantics, release metadata,
or tags. Do not implement classification, provider behavior, or workflow policy
in JavaScript. Treat Rust responses, opaque IDs, revisions, allowed_actions,
recommendations, reviews, and effects as authoritative.

Keep the Rust-rendered version footer visible. Never hard-code or restyle away
the build identity; after deployment, confirm it reports the expected version.

If a requested UI improvement needs a new server field, query, command, route,
action, workflow transition, authentication behavior, persistence change, or
provider effect, stop that part and give me a concise Rust escalation note using
the format in the handoff. Do not create a browser-side workaround.

Work in small coherent batches. Before editing, briefly state the Web-only files
you expect to touch. After each accepted batch run every verification command
and the hosted-preview delivery loop in the handoff: commit the allowed diff,
build the unchanged repository Docker/Compose assembly from that exact commit,
restart both Vortex API and worker on the same uniquely tagged image, verify the
OCI revision plus public liveness/readiness, and report the image digest. You may
read and run the existing Docker/Compose files but may not edit their
architecture, touch secrets, run migrations manually, prune Docker state, or
change Nexus. Never perform a live Spotify/provider mutation while testing
unless I explicitly authorize that exact test. A hosted preview is not a
versioned release: do not bump/tag/publish crates or install a CLI binary.

When finished, report the outcome first, then changed files, tests run, hosted
preview commit/tag/digest and health, remaining UI limitations, and any Rust
escalation notes.
```
