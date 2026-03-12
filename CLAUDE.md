# CLAUDE.md — FrankClaw Development Guide

## Project

FrankClaw is a security-hardened Rust rewrite of OpenClaw (a TypeScript AI assistant gateway). It connects messaging channels (Telegram, Discord, Slack, etc.) to AI model providers (OpenAI, Anthropic, Ollama) via a local WebSocket gateway.

## Build & Test

```bash
cargo check          # Type-check the whole workspace
cargo test           # Run all tests (~467)
cargo build          # Build everything (debug)
cargo build -r       # Build release (LTO, stripped)
cargo build -p frankclaw  # Build just the CLI binary
```

The binary is at `target/debug/frankclaw` (or `target/release/frankclaw`).

## Architecture

13 crates in a Cargo workspace under `crates/`:

| Crate | Purpose |
|-------|---------|
| `frankclaw-core` | Shared types, traits, error hierarchy, SSRF IP blocklist |
| `frankclaw-crypto` | ChaCha20-Poly1305 encryption, Argon2id hashing, HMAC-SHA256 KDF |
| `frankclaw-gateway` | Axum WS+HTTP server, auth middleware, rate limiter, broadcast |
| `frankclaw-sessions` | SQLite session store with encrypted-at-rest transcripts |
| `frankclaw-models` | OpenAI, Anthropic, Ollama providers with failover chain |
| `frankclaw-channels` | Channel adapters (Telegram, Web, Discord, Slack, Signal, WhatsApp) |
| `frankclaw-runtime` | Agent runtime, prompt templates (markdown), subagent orchestration, context compaction |
| `frankclaw-tools` | Tool registry, bash execution (with optional ai-jail sandbox), browser tools (CDP) |
| `frankclaw-memory` | Vector search traits (LanceDB backend TBD) |
| `frankclaw-cron` | Scheduled job service with cron expression parsing |
| `frankclaw-media` | File store with SSRF-safe fetcher, filename sanitization |
| `frankclaw-plugin-sdk` | Channel plugin registry |
| `frankclaw-cli` | CLI binary entry point (setup, doctor, audit, start/stop, gateway) |

## Code Conventions

- **Edition 2024**, MSRV Rust 1.93+
- `#![forbid(unsafe_code)]` on every crate — no exceptions
- All errors use `thiserror` with explicit variants (no catch-all `anyhow` in library crates)
- Secrets wrapped in `secrecy::SecretString` (zeroed on drop, `[REDACTED]` in Debug)
- Async runtime: `tokio` with structured concurrency (`CancellationToken`, `JoinSet`)
- Config hot-reload via `arc_swap::ArcSwap` (lock-free pointer swap)
- Concurrent maps: `dashmap::DashMap` (sharded locking)
- All file I/O permissions: `0o600` (owner-only) for sensitive data, `0o700` for directories
- Token comparison always constant-time
- No `.unwrap()` in production code; use `.expect("invariant: reason")` only for provably safe cases

## Feature Development Rules

- When adding new features, refactor where it makes sense instead of duplicating logic.
- Abstract shared behavior once there are multiple call sites or a clear stable boundary.
- Prefer small, composable components over large feature-specific codepaths.
- Every feature addition should include unit tests for the new behavior and any extracted shared logic.
- Treat regression resistance as part of feature work: do not land new capability without test coverage that protects the existing path.

## Security Rules

- Gateway **refuses** to bind to non-loopback addresses without auth configured (hard error, not a warning)
- SSRF protection on all outbound HTTP: blocks private IPs, CGNAT, link-local, documentation ranges
- Media filenames sanitized (path traversal prevention, leading dots stripped)
- Passwords hashed with Argon2id (t=3, m=64MB, p=4)
- Session transcripts encrypted at rest with ChaCha20-Poly1305 when master key is provided
- Bash tool execution controlled by `BashPolicy` (deny-all default) + optional `ai-jail` sandbox
- `FRANKCLAW_SANDBOX=ai-jail` or `ai-jail-lockdown` wraps commands in bubblewrap+landlock isolation
- `frankclaw audit` reports severity-rated findings (CRIT/HIGH/MED/LOW/INFO) with CI exit codes

## Key Paths

- Config: `~/.local/share/frankclaw/frankclaw.json` (or `FRANKCLAW_STATE_DIR`)
- Sessions DB: `<state_dir>/sessions.db`
- PID file: `<state_dir>/frankclaw.pid` (daemon mode)
- Prompt templates: `crates/frankclaw-runtime/prompts/*.md` (embedded at compile time)
- Default gateway port: `18789`
- OpenClaw reference: `openclaw/` (gitignored, not part of the build)

## Key Environment Variables

| Variable | Description |
|----------|-------------|
| `FRANKCLAW_CONFIG` | Config file path |
| `FRANKCLAW_STATE_DIR` | State directory |
| `FRANKCLAW_BASH_POLICY` | `deny-all` (default), `allow-all`, or comma-separated allowlist |
| `FRANKCLAW_SANDBOX` | `ai-jail` or `ai-jail-lockdown` (requires ai-jail binary) |
| `FRANKCLAW_ALLOW_BROWSER_MUTATIONS` | `1` to enable browser click/type/press |
| `FRANKCLAW_BROWSER_DEVTOOLS_URL` | Chromium DevTools endpoint |

## Adding a New Channel

1. Create `crates/frankclaw-channels/src/<channel>.rs`
2. Implement `frankclaw_core::channel::ChannelPlugin` trait
3. Register in `crates/frankclaw-channels/src/lib.rs`
4. Add channel-specific config to `frankclaw_core::config::ChannelConfig`

## Adding a New Model Provider

1. Create `crates/frankclaw-models/src/<provider>.rs`
2. Implement `frankclaw_core::model::ModelProvider` trait
3. Register in `crates/frankclaw-models/src/lib.rs`
4. Add to `FailoverChain` in CLI startup

## Parity Work Process

When working through `PARITY_TODO.md` features:

1. **One feature at a time** — complete, test, commit before starting the next.
2. **Compare with OpenClaw** (`openclaw/` directory) for functional requirements, but do NOT copy 1:1. Prefer Rust idioms, slim design, and security hardening over feature-identical ports.
3. **Drop what's unnecessary** — if an OpenClaw feature is over-engineered, Node-specific, or adds complexity without clear value, skip it and note why in the TODO.
4. **Add tests** for every new feature. Tests must pass before committing.
5. **Commit per feature** with a clear message describing what was added.
6. **Update `PARITY_TODO.md`** — mark the feature done and add notes on what was implemented vs dropped.
7. **Follow priority order** in `PARITY_TODO.md` (Tier 1 → Tier 2 → Tier 3 → Tier 4).
8. **Frontend**: if UI is needed, keep it slim (TypeScript + Tailwind, no heavy frameworks).

## CI Expectations

- `cargo check` must pass with zero errors
- `cargo test` must pass all tests
- `cargo clippy` should be clean
- `cargo audit` should report no known vulnerabilities
