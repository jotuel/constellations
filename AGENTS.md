# AGENTS.md

Guidance for AI coding agents working in this repository. Read this before editing.

## What this is

`cosmic-ext-constellations` ("Constellations") — a Matrix client for the COSMIC desktop, built with [libcosmic](https://github.com/pop-os/libcosmic) (over iced) and [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk). **Alpha quality**; expect breaking changes. Targets stable 1.0 alongside matrix-rust-sdk / iced stable releases.

Rust edition 2024.

## Commands

```sh
cargo run --release                # run the app (also: cargo run)
cargo build --release              # build only
cargo test                         # run unit tests (inline #[cfg(test)] mod tests + tests.rs files)
cargo test --features wgpu         # run tests with the optional wgpu feature
cargo clippy --all-targets         # lint
cargo fmt -- --check               # formatting check (no committed rustfmt.toml; use defaults)
```

Build features:
- `default = []`
- `wgpu` — enables `libcosmic/wgpu` (explicit GPU backend; libcosmic also pulls `wgpu` unconditionally today).

Linker: `.cargo/config.toml` passes `-Wl,--no-keep-memory` to the linker — relieves virtual memory pressure on low-RAM machines during LTO. Keep it unless you have a reason to drop it.

Lint policy: `[lints.rust] unused_imports = "warn"`. Do not leave unused imports.

## Architecture — Model-View-Update (Elm)

libcosmic/iced is the Elm architecture: **State** (`&mut self`), **Message** (enum), **update** (pure transition), **view** (pure render). Keep update/view pure. For the full pattern, widget styling, and lazy-store troubleshooting, load the `cosmic-development` skill — do not reinvent its guidance here.

Entry point: `src/main.rs` installs the `rustls` ring provider, force-loads i18n, sets up `tracing`, then calls `cosmic::app::run::<Constellations>(...)`. Global allocator is `mimalloc` (`#[global_allocator]`).

Module map:

| Module | Responsibility |
|---|---|
| `constellations/` | App core: `app.rs` (update/view wiring), `mod.rs` (state, messages), `state.rs`, `subscriptions.rs`, `tests.rs` |
| `view/` | Pure view functions: `app`, `chat`, `login`, `switcher`, `error` |
| `matrix/` | matrix-rust-sdk integration (large `mod.rs` + `tests.rs`): sync, timelines, E2E, room state |
| `settings/` | Config UI: `user`, `space`, `room`, `config`, `app` — each with its own MVU cycle |
| `utils/` | `handlers`, `ipc`, `item`, `preview` (markdown/plain), `rich_text`, `unified_push`, `i18n` |

Runtime: single Tokio runtime; E2E encryption, SSO login, and SQLite store via matrix-rust-sdk features.

## Conventions

**i18n — Fluent, mandatory for user-facing strings.**
- Strings live in `res/i18n/<lang>/cosmic_ext_constellations.ftl` (fallback language `en`, domain `cosmic_ext_constellations`).
- Use the exported `fl!` macro: `fl!("message_id")` or `fl!("message_id", args...)`. Never hardcode English in the UI.
- Config in `i18n.toml`; loader is `LazyLock`-initialized in `src/utils/i18n.rs`.

**Theming — never hardcode colors.** Use semantic theme colors (`theme.palette.*`) so Light/Dark modes both render. Use `self.core.system_theme()` for theme props in view helpers. Prefer libcosmic widgets over raw iced. Load the `palette` skill for widget/UX/a11y checklists.

**Performance — avoid avoidable allocations in render/subscription paths.** Load the `bolt` skill for known hotspots (RwLock contention, case-insensitive filtering, Vec reuse). Use the `profiling` Cargo profile for profiling builds: `cargo run --profile profiling` (inherits release, thin LTO, `debug = 1`, no strip).

**Security & input parsing — load the `sentinel` skill** when touching passphrase/key generation, user input parsing, or blocking I/O inside async tasks.

## Gotchas

- **Single-instance lock:** the `single-instance` libcosmic feature is active. If the app is already running, `main.rs` short-circuits (notifies the existing instance). Kill stray processes before debugging if a new run won't start.
- **Custom URI scheme:** `fi.joonastuomi.CosmicExtConstellations://` is handled as `argv[1]` in `main.rs`. `--notify` is a special argv flag for notification relaunch.
- **Lazy store decryption:** if the matrix SQLite store is recreated/cleared (e.g. keyring reset), clear associated lazy cache/search directories too, or you get `invalid MAC of the store key` later. See `cosmic-development` skill §4.
- **`edition = "2024"`** — use 2024-era Rust idioms; keep the toolchain current via `rustup`.

## Delegation

Some tasks produce or require visual artifacts the orchestrator may not be able to inspect directly — app screenshots, profiling flame graphs, UI layout captures. When a task needs **image/vision analysis** (reviewing a screenshot from the `cosmic-ui-automation` skill, inspecting a heap/CPU capture from `cosmic-profiling`, or reading any rendered image output), delegate the analysis to a subagent capable of image understanding instead of guessing from filenames or metadata. Route the work to a subagent, have it describe and verify what it sees, and act on its report.

## Skills

Project-specific skills live in `.agents/skills/`. Load the relevant one before working in its domain rather than improvising:

`bolt` (perf), `palette` (UI/UX/a11y), `sentinel` (security/stability), `testing` (unit tests), `cosmic-development` (MVU/libcosmic), `cosmic-profiling` (heap/CPU profiling), `cosmic-automation` / `cosmic-ui-automation` (programmatic control & screenshots), `linux-perf` (perf profiling), `skillshare-changelog` (CHANGELOG generation), `gitbutler` (version control via `but` instead of `git`).
