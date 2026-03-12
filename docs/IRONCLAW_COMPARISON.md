# IronClaw Feature Comparison

Analysis date: 2026-03-12

IronClaw (NEAR AI) and FrankClaw both descend from OpenClaw (TypeScript) but diverge significantly. This document records what was evaluated and what was skipped.

## Features Adopted

These features were identified as valuable and adopted into FrankClaw:

1. **Circuit Breaker + Retry with Backoff** — LLM provider resilience (Closed→Open→HalfOpen state machine, exponential backoff with jitter)
2. **Credential Leak Detection** — Scan tool/LLM output for API keys, tokens, secrets before reaching user
3. **LLM Response Caching** — In-memory LRU cache keyed by SHA-256 of prompt, saves API costs
4. **Cost Tracking & Budget Guards** — Per-model cost tables, daily spend tracking, budget limits
5. **Extended Thinking / Reasoning** — Support for Claude 3.7+ thinking parameters, thinking-tag stripping, `/think` command
6. **MCP (Model Context Protocol) Client** — JSON-RPC client for stdio/HTTP MCP servers, tool discovery and forwarding
7. **Lifecycle Hooks** — Event hooks (before_inbound, before_tool_call, before_outbound, on_session_start, on_session_end)
8. **Smart Model Routing** — Complexity scorer routes simple queries to cheaper models
9. **Tunnel Support** — Auto-tunnel via cloudflared/ngrok/tailscale for webhook channels
10. **REPL Channel** — Interactive CLI chat (`frankclaw chat`) for local testing without gateway
11. **Richer Routine System** — Event-based triggers (not just cron), lightweight vs full-job execution
12. **Job State Machine** — State tracking (Pending→InProgress→Completed/Failed/Stuck) with self-repair

## Features Intentionally Skipped

### 1. WASM Tool Sandbox (wasmtime)
**What it does:** IronClaw runs untrusted third-party tools in fuel-metered, memory-limited, capability-restricted WebAssembly containers. Fresh instance per execution prevents side-channel leakage.

**Why skipped:** FrankClaw already has ai-jail (bubblewrap + landlock) providing OS-level sandboxing. WASM adds value for untrusted third-party tool distribution but wasmtime is a heavy dependency (~10MB). FrankClaw's tool model is operator-configured, not marketplace-style, so the isolation boundary is different. ai-jail provides stronger OS-level guarantees (filesystem, network, syscall filtering) than WASM for the operator-controlled use case.

### 2. Docker Container Execution (Per-Job Sandboxing)
**What it does:** IronClaw spawns per-job Docker containers with network proxy, domain allowlist, and credential injection. Orchestrator/worker pattern with HTTP API between host and container.

**Why skipped:** Requires Docker daemon — contradicts FrankClaw's zero-external-dependencies philosophy. The complexity is substantial (container lifecycle, proxy, auth, reaper for orphans). FrankClaw's ai-jail sandbox covers the same isolation needs without requiring Docker.

### 3. Full Web Dashboard UI
**What it does:** IronClaw has a full browser UI with real-time SSE log streaming, WebSocket chat, job monitoring dashboard, memory/workspace explorer, extensions & skills management, and routine editor.

**Why skipped:** FrankClaw is backend-first by design. The existing minimal web widget + comprehensive CLI provides the operator experience. A richer UI could be added later as a separate frontend project (TypeScript + Tailwind) if demand materializes, but it's not core to the gateway's value proposition.

### 4. OS Keychain Integration for Master Key
**What it does:** IronClaw stores the master encryption key in the OS keychain (macOS Security Framework, Linux secret-service D-Bus API, Windows Credential Manager) so it never touches disk.

**Why skipped:** Platform-specific code for 3 OS backends is significant maintenance burden. FrankClaw's approach (master key via environment variable or encrypted config file with passphrase) is portable and sufficient. Could be added later as an optional feature behind a feature flag.

### 5. Workspace-Based Memory (Filesystem + Hybrid Search)
**What it does:** IronClaw uses a structured filesystem workspace (MEMORY.md, IDENTITY.md, daily logs, projects/) with hybrid search combining PostgreSQL FTS + pgvector cosine similarity via Reciprocal Rank Fusion scoring.

**Why skipped:** Deeply tied to PostgreSQL/pgvector backend. FrankClaw's memory crate has traits designed for LanceDB (embedded vector DB). Implementing hybrid search over SQLite FTS5 + embedded vectors is a different project that should be done when the LanceDB backend lands. The workspace file structure (MEMORY.md, HEARTBEAT.md, etc.) is an interesting UX pattern but is agent-behavior design, not infrastructure.

### 6. Session Threading Model (Session → Thread → Turn)
**What it does:** IronClaw supports multiple conversation threads within a single session, with thread switching (`/thread <uuid>`), resume (`/resume <uuid>`), and independent state machines per thread.

**Why skipped:** FrankClaw's flat transcript model (session → entries) is simpler and maps well to how messaging channels work (one conversation per chat). Threading adds schema complexity, migration burden, and API surface without clear user demand. Most channels don't have a native threading concept that maps to IronClaw's model.

## Other IronClaw Features Not Applicable

- **NEAR AI provider** — Specific to NEAR ecosystem
- **AWS Bedrock / Google Gemini / Mistral / Yandex / Cloudflare** — More providers can be added incrementally; not architecturally interesting
- **Import from OpenClaw libSQL** — Migration tool, not a feature
- **Dual database backend (PostgreSQL + libSQL)** — FrankClaw's SQLite-only approach is intentional for zero-deps
- **Heartbeat system** — Proactive periodic execution reading a checklist file; interesting but niche. Could be implemented as a cron job that reads a workspace file.
- **Undo/Redo (20 checkpoints)** — FrankClaw has edit/delete last message. Full undo/redo adds complexity for marginal UX gain in a chat interface.
