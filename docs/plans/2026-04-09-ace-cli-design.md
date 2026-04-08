# ACE — Agent CLI Enterprise

## Overview

Fork of [ultraworkers/claw-code](https://github.com/ultraworkers/claw-code) (Rust, ~70K LOC), rebranded as **ACE**. Goal: achieve 100% feature parity with Claude Code, then extend with multi-provider support (Gemini, Ollama) and commercial packaging.

## Decision Log

- **Name:** ACE (Agent CLI Enterprise)
- **Language:** Rust (continue existing codebase)
- **Source:** Fork from ultraworkers/claw-code main branch
- **Approach:** Gap-fill incremental — fill all gaps batch by batch, then add providers
- **Priority:** Fill gaps first → providers second
- **Working directory:** Clone into current folder (claude-code-ui)

---

## Section 1: Rebrand & Repository Setup

### 1.1 Fork & Clone
- Fork `ultraworkers/claw-code` to personal GitHub
- Clone into current working directory
- Create `develop` branch as working branch

### 1.2 Rebrand "claw" → "ace"

| Location | Change |
|----------|--------|
| `rust/Cargo.toml` | Workspace name, metadata |
| `rust/crates/rusty-claude-cli/Cargo.toml` | Binary name `claw` → `ace` |
| `rust/crates/rusty-claude-cli/src/main.rs` | App name, version string, help text |
| `rust/crates/*/Cargo.toml` | Package names: `claw-*` → `ace-*` |
| `README.md` | Branding, description, install instructions |
| `install.sh` | Binary name |
| `.claude.json` | Project metadata |
| All source | String literals containing "claw"/"Claw"/"CLAW" |

### 1.3 License & Legal
- Add `LICENSE` (MIT for open-source core)
- Add `LICENSE-COMMERCIAL` (template for enterprise)
- Add `NOTICE` file documenting origin (fork from claw-code)
- Remove unnecessary files: `.claude/sessions/`, `.omc/`, `.clawd-todos.json`

---

## Section 2: Gap-Fill Priority & Batching

### Batch 1: Critical Runtime Gaps

| Gap | Description | File(s) |
|-----|-------------|---------|
| Bash validation merge | 9 submodules (sed, path, destructive cmd warning, etc.) — done on branch | `runtime/src/bash_validation.rs` |
| AskUserQuestion | Currently stub — needs interactive prompt in REPL | `tools/src/lib.rs`, `cli/src/main.rs` |
| RemoteTrigger | Needs real HTTP client for remote agent triggers | `tools/src/lib.rs` |

### Batch 2: Slash Command Handlers — High Usage

| Command | Status | Notes |
|---------|--------|-------|
| `/review` | Stub | Needs diff analysis + API call |
| `/commit` | Stub | Needs git integration |
| `/pr` | Stub | Needs `gh` CLI wrapper |
| `/diff` | Stub | Needs git diff output |
| `/issue` | Stub | Needs `gh` CLI wrapper |
| `/export` | Stub | Needs session → file export |
| `/compact` | Stub | Needs session compaction logic |
| `/doctor` | Stub | Needs system diagnostics |

### Batch 3: Slash Command Handlers — Medium Usage

| Command | Status |
|---------|--------|
| `/advisor`, `/insights`, `/security-review` | Stub |
| `/team`, `/subagent` | Stub — registry exists, needs CLI wiring |
| `/cron` | Stub — registry exists, needs CLI wiring |
| `/plugin` (install/uninstall) | Stub — PluginManager exists, needs CLI wiring |
| `/hooks` | Stub |
| `/branch`, `/add-dir` | Stub |

### Batch 4: Slash Command Handlers — Low Usage / Niche

| Command | Status |
|---------|--------|
| `/telemetry`, `/providers` | Stub |
| `/desktop`, `/ide` | Stub |
| `/files`, `/context` | Stub |
| Remaining (~15 commands) | Stub |

### Batch 5: Tool Stubs & Polish

| Item | Description |
|------|-------------|
| McpAuth | Full OAuth UX for MCP servers |
| Session compaction accuracy | Verify token counting vs upstream |
| TestingPermission | Test-only, low priority |
| Error messages & edge cases | Polish UX |

---

## Section 3: Provider Layer (Gemini + Ollama)

### 3.1 New file structure

```
api/src/providers/
├── mod.rs              # Add Gemini + Ollama variants
├── anthropic.rs        # Keep as-is
├── openai_compat.rs    # Add Ollama config
└── gemini.rs           # NEW — native Gemini API client
```

### 3.2 Provider strategy

| Provider | Approach | Reason |
|----------|----------|--------|
| Gemini | Native client (`gemini.rs`) | Gemini API differs significantly from OpenAI format |
| Ollama | OpenAI-compat layer | Ollama exposes OpenAI-compatible `/v1/chat/completions` |

### 3.3 Changes required

| File | Change |
|------|--------|
| `api/src/client.rs` | Add `Gemini`, `Ollama` to `ProviderClient` enum |
| `api/src/providers/mod.rs` | Add provider detection logic |
| `api/src/providers/gemini.rs` | New — Gemini REST API, streaming, tool use mapping |
| `api/src/providers/openai_compat.rs` | Add Ollama endpoint config (default `localhost:11434`) |
| `api/src/types.rs` | Type mappings if needed |
| `cli/src/main.rs` | CLI flags `--provider`, `--ollama-url` |
| Config | `ace.json` provider config section |

### 3.4 Model aliases

```
# Gemini
gemini-pro     → gemini-2.5-pro
gemini-flash   → gemini-2.5-flash

# Ollama
ollama/llama3    → llama3:latest @ localhost:11434
ollama/codestral → codestral:latest @ localhost:11434
```

### 3.5 Usage

```bash
ace --provider gemini "explain this code"
GEMINI_API_KEY=xxx ace -m gemini-pro "fix the bug"
ace --provider ollama -m codestral "write tests"
ace --provider ollama --ollama-url http://localhost:11434 "refactor this"
```

---

## Section 4: Testing & CI Strategy

### 4.1 Tests per batch

| Batch | Tests to add |
|-------|-------------|
| Batch 1 | Bash validation unit tests, AskUserQuestion interactive test |
| Batch 2 | Git command integration tests (mock `gh` CLI) |
| Batch 3 | Team/Cron registry wiring tests, plugin lifecycle tests |
| Batch 4 | Smoke tests for remaining slash commands |
| Batch 5 | McpAuth flow test, compaction accuracy test |
| Providers | Mock Gemini server, Ollama endpoint test, provider switching test |

### 4.2 CI Pipeline additions

```yaml
- cargo fmt --check
- cargo clippy -- -D warnings
- cargo test --workspace
- cargo build --release
```

### 4.3 Mock provider servers

Extend `mock-anthropic-service` → `mock-provider-service`:

| Provider | Mock endpoint |
|----------|--------------|
| Anthropic | `/v1/messages` (existing) |
| Gemini | `/v1/models/*/generateContent` |
| Ollama | `/v1/chat/completions` |

---

## Section 5: Commercial Packaging

### 5.1 License tiers

| Tier | Target | Features | License |
|------|--------|----------|---------|
| ACE Community | Individuals, OSS | Core CLI, Anthropic + Ollama, basic tools | MIT |
| ACE Pro | Paid individuals | + Gemini, priority support, advanced commands | Commercial |
| ACE Enterprise | Organizations | + SSO/OAuth, audit logs, team mgmt, self-hosted, SLA | Commercial |

### 5.2 Feature gating

Deferred — architecture-ready but not implemented yet. Future `ace-license` crate with `LicenseTier` enum and feature flag checks.

### 5.3 Distribution

| Channel | Method |
|---------|--------|
| GitHub Releases | Pre-built binaries (Linux, macOS, Windows) |
| Install script | `curl -fsSL https://acedev.io/install.sh \| sh` |
| Homebrew | `brew install ace-cli` |
| Cargo | `cargo install ace-cli` |
| Container | Containerfile (existing) |

### 5.4 Branding files

| File | Content |
|------|---------|
| `README.md` | New — ACE branding, install, usage |
| `LICENSE` | MIT for community |
| `LICENSE-COMMERCIAL` | Commercial license template |
| `CONTRIBUTING.md` | Contribution guidelines |

---

## Execution Order

```
Setup & Rebrand → Batch 1 → Batch 2 → Batch 3 → Batch 4 → Batch 5 → Providers → Commercial
```

Each batch = 1 PR, fully tested before merge.
