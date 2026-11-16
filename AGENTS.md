# AGENTS.md

This file is for AI agents working in this repository. It takes precedence
over any automatically loaded instructions. `CLAUDE.md` still applies: read it
in full at the start of every session, then this file.

## What this is

Yapfast is a native, GPU-rendered Discord and Matrix client written in Rust,
with its interface in Slint. One process handles chat, voice, video, screen
sharing, friends, servers, search and local settings. Memory use is a product
goal, not an afterthought.

- Default build features: `desktop`, `discord`; Matrix is optional.
- UI: `ui/*.slint`, compiled by `crates/yapfast-ui/build.rs` from
  `ui/yapfast.slint`.
- Rust UI: `src/ui.rs`, `src/ui/`.
- Core state/actions: `crates/yapfast-core/src/application.rs`.
- Discord service: `crates/yapfast-discord/src/`.
- Matrix: `crates/yapfast-matrix/` (UNVERIFIED on this host).
- Tests and fake service: `testing/fake_discord.ts`,
  `crates/yapfast-discord/tests/`, `scripts/test-fake-discord`.
- Fork notes: `docs/fork/CODEMAP.md`, `docs/fork/ROADMAP.md`.

Talk to the user in Russian unless they switch. Write repository files,
comments, docs and commit messages in English.

## Read before changing behaviour

- `CLAUDE.md` — commands, conventions, memory and doc rules.
- `docs/architecture.md` — layers and how a UI change flows.
- `docs/building.md` — shells, dependencies, media libraries, logging.
- `docs/discord/README.md` and `docs/discord/adding-a-feature.md` — Discord
  feature layers and the test workflow.
- The one area doc for the feature (`docs/discord/people.md`,
  `messages.md`, `server-settings.md`, `servers.md`, `go-live.md`,
  `docs/voice.md`, `docs/themes.md`, `docs/chat-ui.md`).
- `docs/fork/CODEMAP.md` — the 13 fork areas, with paths and symbols.

## Verified commands (this host, 2026-11-09)

Run cargo only in the dev shell, with the `fast` profile:

```sh
nix develop path:./nix --command cargo build --profile fast
nix develop path:./nix --command cargo test --profile fast -p yapfast-core history
```

- `scripts/check --fast` — fmt, doc links, clippy, tests, build, smoke;
  about 40 s after a Rust edit, 4 min after a `.slint` edit. Run once before
  the last commit of a piece of work, not after every edit. `--matrix` adds
  Matrix builds; run that for shared code and before releases.
- `scripts/test-fake-discord [FILTER]` — service tests against
  `testing/fake_discord.ts`; run the changed area's tests by name while
  working, and the whole suite (about 30 s) before the last commit touching
  `crates/yapfast-discord`, `discord_text` or `testing/fake_discord.ts`.
  `scripts/check` does not run it. The full suite additionally needs
  `ffprobe`/`ffmpeg` on `PATH`; on this host:
  `nix shell nixpkgs#ffmpeg -c nix develop path:./nix --command scripts/test-fake-discord`.
- `scripts/test-stack up` — Matrix/LiveKit stack (UNVERIFIED this session).
- `scripts/ui-session` — private display, input and screenshots. It does not
  start on this WSL2 host (Weston GL "EGL surfaceless" failure; Slint forces
  X11 under WSL). Use the Xvfb recipe in `docs/fork/CODEMAP.md`, or ask the
  user to run a GUI check.
- `nix develop path:./nix --command cargo run --locked -- --preview` — live
  `.slint` preview; works without a logged-in account.
- New worktree: run `scripts/warm-worktree` first; it copies the main
  checkout's build.
- `scripts/check --fast` and `scripts/test-fake-discord` already enter the
  dev shell. Do not nest `nix develop` around them unless adding packages.

Limit memory: one Rust compile at a time. The UI crate needs 6–8 GiB and this
machine has 11 GiB. Run long jobs detached (`setsid ... &`) and poll a log.
`rg` is not installed outside the dev shell; use the grep tool or `grep`.

## How a change flows

`ui/*.slint` callback -> `src/ui/callbacks.rs` -> `Action` (or `Command`) ->
`crates/yapfast-core/src/application.rs::dispatch` -> `Effect`/`Command` ->
`crates/yapfast-discord/src/service.rs` -> gateway/HTTP ->
`Event`/`Live` -> `Application::apply` -> `src/ui/refresh.rs` -> Slint models
and properties.

Keep the layers apart. UI code does not do HTTP. The service returns data; the
core owns state, ordering, permissions and effects; refresh only maps existing
state to widgets. Add the fake's route/event in the same change and test the
service against it.

## Rules

- Do not edit `vendor/i-slint-*` unless the task requires it; each edit
  recompiles Slint and the UI crate for 2–3 minutes.
- Do not touch `Cargo.lock` by hand. Do not add a dependency without checking
  whether the repository already has one that fits.
- Do not log tokens, cookies, message contents or other secrets. Discord runs
  only against `testing/fake_discord.ts`; never use a real account/token.
- Do not add comments unless they explain something non-obvious that the code
  cannot. Match the surrounding style.
- Keep every change lean: share data held twice (`Arc`, borrow), make hot
  paths touch only what changed, stream large data. If you find a needless
  copy, fix it or report it with measurements.
- A behaviour change updates its doc in the same commit. History and
  measurements go in the commit message.
- Do not commit, amend, push or open a PR unless the user explicitly asks.
  Before committing, inspect `git status`, `git diff` and `git log -10`; stage
  only intended files. If the user has not set `git config user.name` and
  `user.email`, ask for them.
- Never claim a command passed without showing or having observed its output.
  Mark anything you did not run as UNVERIFIED.
- Before the final answer of a task, run `scripts/check --fast` (and the full
  fake Discord suite if the area requires it), or state why not.

## Working agreement

1. Read the relevant docs and code map; locate the existing pattern first.
2. Make the smallest change that fits the architecture; add the fake route and
   tests with the service change.
3. Build/test only what changed while iterating.
4. Run the full check once before the last commit; update docs.
5. Report in Russian: what changed, what was verified, what is UNVERIFIED.
