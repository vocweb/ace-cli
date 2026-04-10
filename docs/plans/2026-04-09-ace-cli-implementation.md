# ACE CLI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fork claw-code, rebrand to ACE, fill all feature gaps to 100% Claude Code parity, add Gemini + Ollama providers, and prepare commercial packaging.

**Architecture:** Modular Rust workspace with 9 crates. Core loop: CLI (REPL) → Runtime (session, perms, config) → Tools (dispatch) → API (providers). Providers implement `Provider` trait with `send_message`/`stream_message`. Slash commands dispatched via `SlashCommandSpec` registry.

**Tech Stack:** Rust 2021, tokio, reqwest, serde_json, rustyline, crossterm, pulldown-cmark, syntect

---

## Phase 0: Repository Setup & Rebrand

### Task 0.1: Fork and Clone Repository

**Files:**
- Working directory: `/Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui/`

**Step 1: Fork the repo on GitHub**

Run: `gh repo fork ultraworkers/claw-code --clone=false`

**Step 2: Clone into current directory**

Run:
```bash
# Backup current content
mv /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui-backup

# Clone fork
gh repo clone <your-username>/claw-code /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui

# Copy over the design docs
cp -r /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui-backup/docs /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui/docs
```

**Step 3: Create develop branch**

Run: `cd /Users/tien/Documents/Source/Personal/SMB-Base/other/claude-code-ui && git checkout -b develop`

**Step 4: Verify Rust workspace builds**

Run: `cd rust && cargo build --workspace 2>&1 | tail -5`
Expected: Compiles successfully

**Step 5: Commit design docs**

```bash
git add docs/
git commit -m "docs: add ACE design and implementation plan"
```

---

### Task 0.2: Clean Up Unnecessary Files

**Files:**
- Delete: `.claude/sessions/`
- Delete: `rust/.claude/sessions/`
- Delete: `rust/.claw/sessions/`
- Delete: `rust/.omc/`
- Delete: `rust/.clawd-todos.json`
- Delete: `rust/.sandbox-home/`
- Delete: `src/` (old Python workspace — no longer needed)
- Delete: `tests/` (old Python tests)

**Step 1: Remove files**

```bash
rm -rf .claude/sessions rust/.claude/sessions rust/.claw/sessions rust/.omc rust/.clawd-todos.json rust/.sandbox-home src tests
```

**Step 2: Commit cleanup**

```bash
git add -A
git commit -m "chore: remove old sessions, Python workspace, and temp files"
```

---

### Task 0.3: Rebrand Binary — claw to ace

**Files:**
- Modify: `rust/crates/rusty-claude-cli/Cargo.toml` — binary name
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` — app name, help text, references to "claw"
- Modify: `rust/crates/rusty-claude-cli/build.rs` — if any build-time strings
- Modify: `install.sh` — binary name

**Step 1: Update Cargo.toml binary name**

In `rust/crates/rusty-claude-cli/Cargo.toml`, change:
```toml
[[bin]]
name = "claw"
```
to:
```toml
[[bin]]
name = "ace"
```

**Step 2: Update main.rs references**

Search and replace in `rust/crates/rusty-claude-cli/src/main.rs`:
- All `"claw"` string literals → `"ace"`
- All `"Claw"` → `"ACE"`
- All `"CLAW"` → `"ACE"`
- Help text: `Run \`claw --help\`` → `Run \`ace --help\``
- Session paths: `.claw/` → `.ace/`

**Step 3: Update install.sh**

Replace `claw` binary references with `ace`.

**Step 4: Verify build**

Run: `cd rust && cargo build --workspace`
Expected: Compiles, binary at `target/debug/ace`

**Step 5: Verify binary runs**

Run: `./rust/target/debug/ace --help`
Expected: Help text shows "ace" branding

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: rebrand binary from claw to ace"
```

---

### Task 0.4: Rebrand Crate Names

**Files:**
- Modify: `rust/Cargo.toml` — workspace metadata
- Modify: `rust/crates/api/Cargo.toml` — package name
- Modify: `rust/crates/runtime/Cargo.toml`
- Modify: `rust/crates/tools/Cargo.toml`
- Modify: `rust/crates/commands/Cargo.toml`
- Modify: `rust/crates/plugins/Cargo.toml`
- Modify: `rust/crates/telemetry/Cargo.toml`
- Modify: `rust/crates/mock-anthropic-service/Cargo.toml`
- Modify: `rust/crates/compat-harness/Cargo.toml`
- Modify: `rust/crates/rusty-claude-cli/Cargo.toml` — package name + dependency refs

**Step 1: Rename crate packages**

For each crate, update `[package] name`:
- `api` → `ace-api`
- `runtime` → `ace-runtime`
- `tools` → `ace-tools`
- `commands` → `ace-commands`
- `plugins` → `ace-plugins`
- `telemetry` → `ace-telemetry`
- `mock-anthropic-service` → `ace-mock-service`
- `compat-harness` → `ace-compat-harness`
- `rusty-claude-cli` → `ace-cli`

**Step 2: Update internal dependency references**

In each `Cargo.toml` that has `[dependencies]`, update path references:
```toml
# Before
api = { path = "../api" }
# After
ace-api = { path = "../api", package = "ace-api" }
```

Note: The `path` stays the same (directory names unchanged), only `package` and import names change. Alternatively, keep `path` crates using their directory names as aliases to minimize code changes:
```toml
api = { path = "../api" }  # Cargo resolves by directory — package name doesn't affect path deps
```

**Decision: Keep internal path deps using directory names** to avoid touching every `use` statement. Only rename `[package] name` for publishing purposes.

**Step 3: Update workspace Cargo.toml metadata**

```toml
[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
publish = false
description = "ACE — Agent CLI Enterprise"
```

**Step 4: Build and test**

Run: `cd rust && cargo build --workspace && cargo test --workspace`
Expected: All pass

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: rebrand crate package names to ace-*"
```

---

### Task 0.5: Rebrand String Literals Across Source

**Files:**
- Modify: All `.rs` files containing "claw"/"Claw"/"CLAW" string literals
- Modify: `rust/README.md`
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `.claude.json`
- Modify: `Containerfile`
- Modify: `USAGE.md`
- Modify: `ROADMAP.md`

**Step 1: Find all occurrences**

Run: `cd rust && grep -rn "claw\|Claw\|CLAW" --include="*.rs" --include="*.toml" --include="*.md" --include="*.json" --include="*.sh" --include="*.yml" | grep -v target/ | grep -v ".claude/sessions"`

**Step 2: Replace systematically**

- `claw` → `ace` (binary name, session paths, config paths)
- `Claw` → `ACE` or `Ace` (display names)
- `CLAW` → `ACE` (env vars, constants)
- `claw-code` → `ace-cli` (repo references)
- `.claw/` → `.ace/` (local config directory)

**Step 3: Update session file extension constant**

In `main.rs`:
```rust
// Session directory
const SESSION_DIR: &str = ".ace";
```

**Step 4: Build and test**

Run: `cd rust && cargo build --workspace && cargo test --workspace`

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: rebrand all string literals from claw to ace"
```

---

### Task 0.6: Add License and Legal Files

**Files:**
- Create: `LICENSE` (MIT)
- Create: `LICENSE-COMMERCIAL`
- Create: `NOTICE`
- Modify: `README.md` — rewrite for ACE branding

**Step 1: Create MIT LICENSE**

Write standard MIT license to `LICENSE` with current year and author.

**Step 2: Create NOTICE**

```markdown
# NOTICE

ACE (Agent CLI Enterprise) is derived from claw-code
(https://github.com/ultraworkers/claw-code), which is itself a clean-room
rewrite inspired by Claude Code's architecture.

ACE is not affiliated with, endorsed by, or maintained by Anthropic.
```

**Step 3: Create LICENSE-COMMERCIAL**

Write placeholder commercial license template.

**Step 4: Rewrite README.md**

New README with:
- ACE branding and description
- Install instructions (`cargo install`, install script, container)
- Quick start usage examples
- Provider support table (Anthropic for now, Gemini/Ollama coming)
- License section (MIT + commercial)

**Step 5: Commit**

```bash
git add LICENSE LICENSE-COMMERCIAL NOTICE README.md
git commit -m "docs: add ACE license, notice, and rewrite README"
```

---

## Phase 1: Critical Runtime Gaps (Batch 1)

### Task 1.1: Merge Bash Validation Submodules

**Files:**
- Modify: `rust/crates/runtime/src/bash_validation.rs`
- Test: `rust/crates/runtime/tests/integration_tests.rs`

**Step 1: Check what's on the branch**

Run: `gh api repos/ultraworkers/claw-code/compare/main...feat/batch3-all --jq '.files[] | select(.filename | contains("bash_validation")) | .filename'`

**Step 2: Cherry-pick or port the bash validation improvements**

Read the branch diff for `bash_validation.rs` and apply the 9 submodule improvements:
1. sed validation
2. path validation
3. destructive command warning
4. pipe chain analysis
5. env var expansion detection
6. redirect validation
7. background process detection
8. sudo escalation checks
9. command chaining safety

**Step 3: Write tests for each validation submodule**

Add to `rust/crates/runtime/tests/integration_tests.rs`:

```rust
#[test]
fn bash_validation_blocks_destructive_rm_rf() {
    let result = validate_command("rm -rf /", PermissionMode::default(), Path::new("/tmp"));
    assert!(matches!(result, ValidationResult::Block { .. }));
}

#[test]
fn bash_validation_allows_read_only_ls() {
    let result = validate_command("ls -la", PermissionMode::default(), Path::new("/tmp"));
    assert!(matches!(result, ValidationResult::Allow));
}

#[test]
fn bash_validation_warns_on_sudo() {
    let result = validate_command("sudo apt install foo", PermissionMode::default(), Path::new("/tmp"));
    assert!(matches!(result, ValidationResult::Warn { .. } | ValidationResult::Block { .. }));
}

// Add tests for each of the 9 submodules
```

**Step 4: Run tests**

Run: `cd rust && cargo test --workspace`
Expected: All pass

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: complete bash validation submodules (9/9)"
```

---

### Task 1.2: Implement AskUserQuestion Tool

**Files:**
- Modify: `rust/crates/tools/src/lib.rs` — replace stub with real implementation
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` — add interactive prompt handling
- Modify: `rust/crates/rusty-claude-cli/src/input.rs` — add question prompt function

**Step 1: Write failing test**

In `rust/crates/tools/src/lib.rs` (test module):
```rust
#[test]
fn ask_user_question_returns_structured_payload() {
    // AskUserQuestion should return a pending payload with the question text
    let input = serde_json::json!({
        "question": "Which database should we use?",
        "options": ["PostgreSQL", "SQLite", "MySQL"]
    });
    let result = execute_ask_user_question(&input);
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["status"], "pending");
    assert_eq!(response["question"], "Which database should we use?");
}
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test ask_user_question`
Expected: FAIL — function not found or returns wrong payload

**Step 3: Implement AskUserQuestion in tools crate**

```rust
pub fn execute_ask_user_question(input: &Value) -> Result<Value, ToolError> {
    let question = input["question"].as_str().unwrap_or("Question from assistant");
    let options = input.get("options")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());

    Ok(serde_json::json!({
        "status": "pending",
        "question": question,
        "options": options,
    }))
}
```

**Step 4: Wire into CLI REPL**

In `main.rs`, when tool result has `"status": "pending"` for AskUserQuestion:
- Display the question to the user
- If options provided, show numbered list
- Read user input via rustyline
- Return the answer as tool result

**Step 5: Run tests**

Run: `cd rust && cargo test --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: implement AskUserQuestion with interactive REPL prompt"
```

---

### Task 1.3: Implement RemoteTrigger Tool

**Files:**
- Modify: `rust/crates/tools/src/lib.rs` — replace stub
- Modify: `rust/crates/runtime/src/remote.rs` — add HTTP trigger client

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn remote_trigger_sends_http_request() {
    // Mock server setup
    let trigger_input = serde_json::json!({
        "url": "http://localhost:0/trigger",
        "method": "POST",
        "body": {"task": "run-tests"}
    });
    let result = execute_remote_trigger(&trigger_input).await;
    // Should return structured response (even if connection fails, shouldn't panic)
    assert!(result.is_ok() || matches!(result, Err(ToolError::Network { .. })));
}
```

**Step 2: Implement RemoteTrigger**

```rust
pub async fn execute_remote_trigger(input: &Value) -> Result<Value, ToolError> {
    let url = input["url"].as_str()
        .ok_or_else(|| ToolError::InvalidInput("url is required".into()))?;
    let method = input["method"].as_str().unwrap_or("POST");
    let body = input.get("body");

    let client = reqwest::Client::new();
    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        _ => return Err(ToolError::InvalidInput(format!("unsupported method: {method}"))),
    };

    if let Some(body) = body {
        request = request.json(body);
    }

    let response = request.send().await
        .map_err(|e| ToolError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();

    Ok(serde_json::json!({
        "status": status,
        "body": text,
    }))
}
```

**Step 3: Run tests**

Run: `cd rust && cargo test --workspace`

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement RemoteTrigger with HTTP client"
```

---

## Phase 2: High-Usage Slash Commands (Batch 2)

### Task 2.1: Implement /diff Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` — add handler
- Modify: `rust/crates/runtime/src/git_context.rs` — add diff function

**Step 1: Write test**

```rust
#[test]
fn git_diff_output_is_string() {
    let result = git_diff(None); // None = unstaged diff
    // Should not panic, returns Ok or Err
    assert!(result.is_ok() || result.is_err());
}
```

**Step 2: Implement git_diff in runtime**

```rust
pub fn git_diff(base: Option<&str>) -> Result<String, RuntimeError> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("diff");
    if let Some(base) = base {
        cmd.arg(base);
    }
    let output = cmd.output()
        .map_err(|e| RuntimeError::Git(format!("failed to run git diff: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

**Step 3: Wire /diff slash command**

In `main.rs` slash command handler:
```rust
"diff" => {
    let base = args.first().map(|s| s.as_str());
    match git_diff(base) {
        Ok(diff) => render_markdown(&diff),
        Err(e) => eprintln!("error: {e}"),
    }
}
```

**Step 4: Test and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: implement /diff slash command"
```

---

### Task 2.2: Implement /commit Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Modify: `rust/crates/runtime/src/git_context.rs`

**Step 1: Implement git_commit**

```rust
pub fn git_add_and_commit(message: &str) -> Result<String, RuntimeError> {
    // Stage all changes
    let add = std::process::Command::new("git")
        .args(["add", "-A"])
        .output()
        .map_err(|e| RuntimeError::Git(format!("git add failed: {e}")))?;
    if !add.status.success() {
        return Err(RuntimeError::Git(String::from_utf8_lossy(&add.stderr).to_string()));
    }

    // Commit
    let commit = std::process::Command::new("git")
        .args(["commit", "-m", message])
        .output()
        .map_err(|e| RuntimeError::Git(format!("git commit failed: {e}")))?;

    Ok(String::from_utf8_lossy(&commit.stdout).to_string())
}
```

**Step 2: Wire /commit handler**

```rust
"commit" => {
    let message = args.join(" ");
    if message.is_empty() {
        eprintln!("Usage: /commit <message>");
    } else {
        match git_add_and_commit(&message) {
            Ok(output) => println!("{output}"),
            Err(e) => eprintln!("error: {e}"),
        }
    }
}
```

**Step 3: Test and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: implement /commit slash command"
```

---

### Task 2.3: Implement /pr Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Modify: `rust/crates/runtime/src/git_context.rs`

**Step 1: Implement gh_create_pr**

```rust
pub fn gh_create_pr(title: &str, body: &str) -> Result<String, RuntimeError> {
    let output = std::process::Command::new("gh")
        .args(["pr", "create", "--title", title, "--body", body])
        .output()
        .map_err(|e| RuntimeError::Git(format!("gh pr create failed: {e}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(RuntimeError::Git(String::from_utf8_lossy(&output.stderr).to_string()))
    }
}
```

**Step 2: Wire /pr handler**

```rust
"pr" => {
    let title = args.join(" ");
    if title.is_empty() {
        eprintln!("Usage: /pr <title>");
    } else {
        match gh_create_pr(&title, "") {
            Ok(url) => println!("PR created: {url}"),
            Err(e) => eprintln!("error: {e}"),
        }
    }
}
```

**Step 3: Test and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: implement /pr slash command via gh CLI"
```

---

### Task 2.4: Implement /issue Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Modify: `rust/crates/runtime/src/git_context.rs`

**Step 1: Implement gh_issue functions**

```rust
pub fn gh_list_issues(limit: usize) -> Result<String, RuntimeError> {
    let output = std::process::Command::new("gh")
        .args(["issue", "list", "--limit", &limit.to_string()])
        .output()
        .map_err(|e| RuntimeError::Git(format!("gh issue list failed: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn gh_view_issue(number: &str) -> Result<String, RuntimeError> {
    let output = std::process::Command::new("gh")
        .args(["issue", "view", number])
        .output()
        .map_err(|e| RuntimeError::Git(format!("gh issue view failed: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

**Step 2: Wire /issue handler and commit**

```bash
git add -A
git commit -m "feat: implement /issue slash command via gh CLI"
```

---

### Task 2.5: Implement /export Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Modify: `rust/crates/runtime/src/session.rs`

**Step 1: Implement session export**

```rust
pub fn export_session_to_markdown(session: &Session) -> String {
    let mut output = String::new();
    output.push_str(&format!("# Session: {}\n\n", session.id));
    for msg in &session.messages {
        output.push_str(&format!("## {}\n\n{}\n\n", msg.role, msg.text_content()));
    }
    output
}
```

**Step 2: Wire /export handler**

Write exported markdown to `session-<id>.md` in current directory.

**Step 3: Test and commit**

```bash
git add -A
git commit -m "feat: implement /export session to markdown"
```

---

### Task 2.6: Implement /compact Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Modify: `rust/crates/runtime/src/compact.rs`

**Step 1: Wire existing compaction logic to /compact**

The `compact.rs` module already has implementation. Wire it to the slash command:

```rust
"compact" => {
    match compact_current_session(&mut session, &client).await {
        Ok(summary) => println!("Session compacted. {summary}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
```

**Step 2: Test and commit**

```bash
git add -A
git commit -m "feat: wire /compact slash command to session compaction"
```

---

### Task 2.7: Implement /doctor Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Implement doctor diagnostics**

```rust
fn run_doctor() -> String {
    let mut report = String::from("# ACE Doctor Report\n\n");

    // Check Rust version
    let rust_version = std::process::Command::new("rustc").arg("--version").output();
    report.push_str(&format!("- Rust: {}\n", match rust_version {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "NOT FOUND".to_string(),
    }));

    // Check git
    let git_version = std::process::Command::new("git").arg("--version").output();
    report.push_str(&format!("- Git: {}\n", match git_version {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "NOT FOUND".to_string(),
    }));

    // Check gh CLI
    let gh_version = std::process::Command::new("gh").arg("--version").output();
    report.push_str(&format!("- GitHub CLI: {}\n", match gh_version {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("unknown").to_string(),
        Err(_) => "NOT FOUND".to_string(),
    }));

    // Check API key
    report.push_str(&format!("- ANTHROPIC_API_KEY: {}\n",
        if std::env::var("ANTHROPIC_API_KEY").is_ok() { "set" } else { "NOT SET" }
    ));

    // Check config
    let config_path = dirs::home_dir().map(|h| h.join(".ace").join("config.json"));
    report.push_str(&format!("- Config: {}\n", match config_path {
        Some(p) if p.exists() => format!("found at {}", p.display()),
        _ => "not found (using defaults)".to_string(),
    }));

    report
}
```

**Step 2: Test and commit**

```bash
git add -A
git commit -m "feat: implement /doctor system diagnostics"
```

---

### Task 2.8: Implement /review Command

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Implement review workflow**

`/review` should:
1. Run `git diff HEAD` to get current changes
2. Send the diff to the AI with a review prompt
3. Display the review response

```rust
"review" => {
    let diff = git_diff(Some("HEAD"))?;
    if diff.trim().is_empty() {
        println!("No changes to review.");
    } else {
        let review_prompt = format!(
            "Review the following git diff for code quality, bugs, and security issues:\n\n```diff\n{diff}\n```"
        );
        // Send to conversation as a user message and let the model respond
        send_user_message(&review_prompt, &mut session, &client).await?;
    }
}
```

**Step 2: Test and commit**

```bash
git add -A
git commit -m "feat: implement /review slash command with AI diff analysis"
```

---

## Phase 3: Medium-Usage Slash Commands (Batch 3)

### Task 3.1: Wire /team and /subagent to Existing Registries

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Read: `rust/crates/runtime/src/worker_boot.rs` (WorkerRegistry)
- Read: `rust/crates/runtime/src/team_cron_registry.rs` (TeamRegistry)

**Step 1: Wire /team**

TeamRegistry already exists. Wire subcommands:
- `/team list` → `team_registry.list()`
- `/team create <name>` → `team_registry.create(name)`
- `/team delete <name>` → `team_registry.delete(name)`

**Step 2: Wire /subagent**

WorkerRegistry already exists. Wire:
- `/subagent list` → `worker_registry.list()`
- `/subagent steer <id> <message>` → `worker_registry.steer(id, message)`
- `/subagent kill <id>` → `worker_registry.kill(id)`

**Step 3: Test and commit**

```bash
git add -A
git commit -m "feat: wire /team and /subagent to existing registries"
```

---

### Task 3.2: Wire /cron to CronRegistry

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Wire subcommands**

- `/cron list` → `cron_registry.list()`
- `/cron create <schedule> <command>` → `cron_registry.create(schedule, command)`
- `/cron delete <id>` → `cron_registry.delete(id)`

**Step 2: Commit**

```bash
git add -A
git commit -m "feat: wire /cron to CronRegistry"
```

---

### Task 3.3: Wire /plugin to PluginManager

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`
- Read: `rust/crates/plugins/src/lib.rs` (PluginManager)

**Step 1: Wire subcommands**

- `/plugin list` → `plugin_manager.list()`
- `/plugin install <name>` → `plugin_manager.install(name)`
- `/plugin enable <name>` → `plugin_manager.enable(name)`
- `/plugin disable <name>` → `plugin_manager.disable(name)`
- `/plugin uninstall <name>` → `plugin_manager.uninstall(name)`

**Step 2: Commit**

```bash
git add -A
git commit -m "feat: wire /plugin install/enable/disable/uninstall to PluginManager"
```

---

### Task 3.4: Implement /hooks, /branch, /add-dir

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: /hooks**

List configured hooks from runtime config:
```rust
"hooks" => {
    let hooks = config.hooks();
    if hooks.is_empty() {
        println!("No hooks configured.");
    } else {
        for hook in hooks {
            println!("- {}: {}", hook.event, hook.command);
        }
    }
}
```

**Step 2: /branch**

```rust
"branch" => {
    let output = std::process::Command::new("git")
        .args(["branch", "-a", "--sort=-committerdate"])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
```

**Step 3: /add-dir**

Add additional directories to the session workspace context.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement /hooks, /branch, /add-dir slash commands"
```

---

### Task 3.5: Implement /advisor, /insights, /security-review

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

These are AI-powered commands. Implementation pattern: construct a specialized prompt and send to the conversation.

**Step 1: /advisor**

```rust
"advisor" => {
    let prompt = "Analyze the current project and provide architectural recommendations, potential issues, and improvement suggestions.";
    send_user_message(prompt, &mut session, &client).await?;
}
```

**Step 2: /insights**

```rust
"insights" => {
    let git_log = std::process::Command::new("git")
        .args(["log", "--oneline", "-20"])
        .output()?;
    let log_text = String::from_utf8_lossy(&git_log.stdout);
    let prompt = format!("Analyze recent development activity and provide insights:\n\n```\n{log_text}\n```");
    send_user_message(&prompt, &mut session, &client).await?;
}
```

**Step 3: /security-review**

```rust
"security-review" => {
    let diff = git_diff(Some("main"))?;
    let prompt = format!("Perform a security review of the following changes. Check for OWASP Top 10 vulnerabilities, injection risks, authentication issues, and sensitive data exposure:\n\n```diff\n{diff}\n```");
    send_user_message(&prompt, &mut session, &client).await?;
}
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement /advisor, /insights, /security-review commands"
```

---

## Phase 4: Low-Usage Slash Commands (Batch 4)

### Task 4.1: Implement Remaining Stub Commands

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

Implement each remaining stub with minimal but functional logic:

| Command | Implementation |
|---------|---------------|
| `/telemetry` | Show telemetry settings, enable/disable |
| `/providers` | List available providers and their status |
| `/desktop` | Print instructions for desktop app integration |
| `/ide` | Print instructions for IDE extension setup |
| `/files` | List files in workspace context |
| `/context` | Show current session context (model, tokens, files) |
| Remaining stubs | Each gets a minimal but real handler |

**Step 1: Implement in batches of 3-5 commands**

**Step 2: Test each batch**

Run: `cd rust && cargo test --workspace`

**Step 3: Commit per batch**

```bash
git add -A
git commit -m "feat: implement /telemetry, /providers, /desktop, /ide commands"
git add -A
git commit -m "feat: implement /files, /context and remaining stub commands"
```

---

## Phase 5: Tool Stubs & Polish (Batch 5)

### Task 5.1: Implement McpAuth Tool

**Files:**
- Modify: `rust/crates/tools/src/lib.rs`
- Modify: `rust/crates/runtime/src/oauth.rs`

**Step 1: Wire McpAuth to existing OAuth module**

The `oauth.rs` module has a full PKCE flow. Wire it to the McpAuth tool:

```rust
pub async fn execute_mcp_auth(input: &Value) -> Result<Value, ToolError> {
    let server_name = input["server"].as_str()
        .ok_or_else(|| ToolError::InvalidInput("server name required".into()))?;
    let auth_url = input["auth_url"].as_str()
        .ok_or_else(|| ToolError::InvalidInput("auth_url required".into()))?;

    let token = oauth::run_pkce_flow(auth_url, DEFAULT_OAUTH_CALLBACK_PORT).await
        .map_err(|e| ToolError::Auth(e.to_string()))?;

    oauth::persist_token(server_name, &token)?;

    Ok(serde_json::json!({
        "status": "authenticated",
        "server": server_name,
    }))
}
```

**Step 2: Test and commit**

```bash
git add -A
git commit -m "feat: implement McpAuth tool with OAuth PKCE flow"
```

---

### Task 5.2: Session Compaction Accuracy

**Files:**
- Modify: `rust/crates/runtime/src/compact.rs`
- Test: `rust/crates/runtime/tests/integration_tests.rs`

**Step 1: Add token counting accuracy tests**

```rust
#[test]
fn token_estimate_within_10_percent_of_actual() {
    let text = "Hello world, this is a test message for token estimation.";
    let estimate = estimate_tokens(text);
    // ~12 tokens for this text, allow 10% margin
    assert!(estimate >= 10 && estimate <= 15, "estimate was {estimate}");
}
```

**Step 2: Verify compaction preserves conversation coherence**

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: improve session compaction accuracy with tests"
```

---

### Task 5.3: Error Message Polish

**Files:**
- Modify: Various files across crates

**Step 1: Audit error messages for user-friendliness**

Grep for `eprintln!("error:` and review each message.

**Step 2: Add actionable suggestions to common errors**

Examples:
- "ANTHROPIC_API_KEY not set" → "ANTHROPIC_API_KEY not set. Run `ace login` or set the environment variable."
- "git not found" → "git not found. Install git: https://git-scm.com/downloads"

**Step 3: Commit**

```bash
git add -A
git commit -m "fix: improve error messages with actionable suggestions"
```

---

## Phase 6: Provider Layer — Gemini + Ollama

### Task 6.1: Add Gemini Provider Kind

**Files:**
- Modify: `rust/crates/api/src/providers/mod.rs`

**Step 1: Write failing test**

```rust
#[test]
fn detect_gemini_provider() {
    assert_eq!(detect_provider_kind("gemini-2.5-pro"), ProviderKind::Gemini);
    assert_eq!(detect_provider_kind("gemini-2.5-flash"), ProviderKind::Gemini);
}

#[test]
fn resolve_gemini_aliases() {
    assert_eq!(resolve_model_alias("gemini-pro"), "gemini-2.5-pro");
    assert_eq!(resolve_model_alias("gemini-flash"), "gemini-2.5-flash");
}
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test detect_gemini`
Expected: FAIL

**Step 3: Add Gemini to ProviderKind enum**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Xai,
    OpenAi,
    Gemini,    // NEW
    Ollama,    // NEW
}
```

**Step 4: Add model detection and aliases**

In `resolve_model_alias()`:
```rust
"gemini-pro" => "gemini-2.5-pro".to_string(),
"gemini-flash" => "gemini-2.5-flash".to_string(),
```

In `detect_provider_kind()`:
```rust
if model.starts_with("gemini") { return ProviderKind::Gemini; }
if model.starts_with("ollama/") { return ProviderKind::Ollama; }
```

In `metadata_for_model()`:
```rust
ProviderKind::Gemini => Some(ProviderMetadata {
    provider: ProviderKind::Gemini,
    auth_env: "GEMINI_API_KEY",
    base_url_env: "GEMINI_BASE_URL",
    default_base_url: "https://generativelanguage.googleapis.com",
}),
ProviderKind::Ollama => Some(ProviderMetadata {
    provider: ProviderKind::Ollama,
    auth_env: "",  // No auth needed for local Ollama
    base_url_env: "OLLAMA_BASE_URL",
    default_base_url: "http://localhost:11434",
}),
```

**Step 5: Run tests**

Run: `cd rust && cargo test --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add Gemini and Ollama to ProviderKind enum"
```

---

### Task 6.2: Implement Gemini Native Client

**Files:**
- Create: `rust/crates/api/src/providers/gemini.rs`
- Modify: `rust/crates/api/src/providers/mod.rs` — add `pub mod gemini;`

**Step 1: Write failing integration test**

```rust
// rust/crates/api/tests/gemini_integration.rs
#[tokio::test]
async fn gemini_client_serializes_request_correctly() {
    let client = GeminiClient::new("test-api-key", "https://generativelanguage.googleapis.com");
    let request = MessageRequest {
        model: "gemini-2.5-pro".to_string(),
        max_tokens: 1024,
        messages: vec![InputMessage::user_text("Hello")],
        ..Default::default()
    };
    let gemini_request = client.convert_request(&request);
    // Verify Gemini format
    assert!(gemini_request["contents"].is_array());
    assert_eq!(gemini_request["contents"][0]["role"], "user");
}
```

**Step 2: Implement GeminiClient**

```rust
// rust/crates/api/src/providers/gemini.rs

use crate::types::*;
use crate::error::ApiError;
use crate::http_client::build_http_client;

#[derive(Debug, Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl GeminiClient {
    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| ApiError::MissingCredential("GEMINI_API_KEY".into()))?;
        let base_url = std::env::var("GEMINI_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
        Ok(Self {
            http: build_http_client()?,
            api_key,
            base_url,
        })
    }

    pub fn convert_request(&self, request: &MessageRequest) -> serde_json::Value {
        let contents: Vec<serde_json::Value> = request.messages.iter().map(|msg| {
            let parts: Vec<serde_json::Value> = msg.content.iter().filter_map(|block| {
                match block {
                    InputContentBlock::Text { text } => Some(serde_json::json!({"text": text})),
                    InputContentBlock::ToolResult { content, .. } => {
                        let text = content.iter().filter_map(|c| match c {
                            ToolResultContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        Some(serde_json::json!({"text": text}))
                    },
                    _ => None,
                }
            }).collect();

            serde_json::json!({
                "role": if msg.role == "assistant" { "model" } else { "user" },
                "parts": parts,
            })
        }).collect();

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "maxOutputTokens": request.max_tokens,
            }
        });

        if let Some(system) = &request.system {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": system}]
            });
        }

        if let Some(temp) = request.temperature {
            body["generationConfig"]["temperature"] = serde_json::json!(temp);
        }

        if let Some(tools) = &request.tools {
            let function_declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            }).collect();
            body["tools"] = serde_json::json!([{
                "functionDeclarations": function_declarations
            }]);
        }

        body
    }

    fn convert_response(&self, json: serde_json::Value) -> Result<MessageResponse, ApiError> {
        let candidate = json["candidates"].get(0)
            .ok_or_else(|| ApiError::InvalidResponse("no candidates".into()))?;

        let parts = candidate["content"]["parts"].as_array()
            .ok_or_else(|| ApiError::InvalidResponse("no parts".into()))?;

        let content: Vec<OutputContentBlock> = parts.iter().filter_map(|part| {
            if let Some(text) = part["text"].as_str() {
                Some(OutputContentBlock::Text { text: text.to_string() })
            } else if let Some(fc) = part.get("functionCall") {
                Some(OutputContentBlock::ToolUse {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: fc["name"].as_str().unwrap_or_default().to_string(),
                    input: fc["args"].clone(),
                })
            } else {
                None
            }
        }).collect();

        let usage = json.get("usageMetadata").map(|u| Usage {
            input_tokens: u["promptTokenCount"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
            ..Default::default()
        }).unwrap_or_default();

        let stop_reason = candidate["finishReason"].as_str().map(|r| match r {
            "STOP" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "content_filter",
            other => other,
        }.to_string());

        Ok(MessageResponse {
            id: json["responseId"].as_str().unwrap_or("").to_string(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content,
            model: String::new(),
            stop_reason,
            stop_sequence: None,
            usage,
            request_id: None,
        })
    }

    pub async fn send_message(&self, request: &MessageRequest) -> Result<MessageResponse, ApiError> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, request.model, self.api_key
        );
        let body = self.convert_request(request);

        let response = self.http.post(&url)
            .json(&body)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = response.status();
        let json: serde_json::Value = response.json().await
            .map_err(|e| ApiError::InvalidResponse(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::ProviderError {
                status: status.as_u16(),
                message: json["error"]["message"].as_str().unwrap_or("unknown error").to_string(),
            });
        }

        self.convert_response(json)
    }

    pub async fn stream_message(&self, request: &MessageRequest) -> Result<MessageStream, ApiError> {
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
            self.base_url, request.model, self.api_key
        );
        let body = self.convert_request(request);

        let response = self.http.post(&url)
            .json(&body)
            .send().await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        // Parse SSE stream and convert to internal MessageStream format
        // Implementation follows the same SSE parsing pattern as anthropic.rs
        todo!("SSE stream parsing — follow anthropic.rs pattern")
    }
}
```

**Step 3: Run tests**

Run: `cd rust && cargo test gemini`

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement Gemini native API client"
```

---

### Task 6.3: Implement Gemini Streaming

**Files:**
- Modify: `rust/crates/api/src/providers/gemini.rs`

**Step 1: Implement SSE stream parsing for Gemini**

Follow the same pattern as `anthropic.rs` SSE parser, but parse Gemini's streaming format.

**Step 2: Test streaming**

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: implement Gemini streaming support"
```

---

### Task 6.4: Add Ollama via OpenAI-Compat

**Files:**
- Modify: `rust/crates/api/src/providers/openai_compat.rs`

**Step 1: Write failing test**

```rust
#[test]
fn ollama_config_defaults() {
    let config = OpenAiCompatConfig::ollama();
    assert_eq!(config.default_base_url, "http://localhost:11434");
    assert_eq!(config.api_key_env, ""); // No auth needed
}
```

**Step 2: Add Ollama config**

```rust
impl OpenAiCompatConfig {
    #[must_use]
    pub const fn ollama() -> Self {
        Self {
            provider_name: "ollama",
            api_key_env: "",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: "http://localhost:11434",
        }
    }
}
```

**Step 3: Handle no-auth case in OpenAiCompatClient**

Modify `from_env()` to allow empty API key for Ollama:

```rust
pub fn from_env(config: OpenAiCompatConfig) -> Result<Self, ApiError> {
    let api_key = if config.api_key_env.is_empty() {
        String::new()  // No auth required (e.g., Ollama)
    } else {
        std::env::var(config.api_key_env)
            .map_err(|_| ApiError::MissingCredential(config.api_key_env.into()))?
    };
    // ... rest unchanged
}
```

**Step 4: Strip model prefix for Ollama**

When model is `ollama/llama3`, send `llama3` to the Ollama API:
```rust
fn resolve_ollama_model(model: &str) -> &str {
    model.strip_prefix("ollama/").unwrap_or(model)
}
```

**Step 5: Run tests and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: add Ollama support via OpenAI-compat layer"
```

---

### Task 6.5: Wire Providers into ProviderClient

**Files:**
- Modify: `rust/crates/api/src/client.rs`

**Step 1: Add Gemini and Ollama variants**

```rust
#[derive(Debug, Clone)]
pub enum ProviderClient {
    Anthropic(AnthropicClient),
    Xai(OpenAiCompatClient),
    OpenAi(OpenAiCompatClient),
    Gemini(GeminiClient),      // NEW
    Ollama(OpenAiCompatClient), // NEW
}
```

**Step 2: Update from_model_with_anthropic_auth**

```rust
ProviderKind::Gemini => Ok(Self::Gemini(GeminiClient::from_env()?)),
ProviderKind::Ollama => Ok(Self::Ollama(OpenAiCompatClient::from_env(
    OpenAiCompatConfig::ollama(),
)?)),
```

**Step 3: Update send_message and stream_message dispatch**

```rust
pub async fn send_message(&self, request: &MessageRequest) -> Result<MessageResponse, ApiError> {
    match self {
        Self::Anthropic(c) => c.send_message(request).await,
        Self::Xai(c) | Self::OpenAi(c) | Self::Ollama(c) => c.send_message(request).await,
        Self::Gemini(c) => c.send_message(request).await,
    }
}
```

**Step 4: Run tests and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: wire Gemini and Ollama into ProviderClient dispatch"
```

---

### Task 6.6: Add CLI Flags for Provider Selection

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Add --provider and --ollama-url flags**

```rust
// In CLI arg parsing
.arg(Arg::new("provider")
    .long("provider")
    .help("AI provider: anthropic, gemini, ollama")
    .value_parser(["anthropic", "gemini", "ollama"]))
.arg(Arg::new("ollama-url")
    .long("ollama-url")
    .help("Ollama server URL (default: http://localhost:11434)")
    .default_value("http://localhost:11434"))
```

**Step 2: Use provider flag in client construction**

```rust
let client = match provider.as_deref() {
    Some("gemini") => ProviderClient::Gemini(GeminiClient::from_env()?),
    Some("ollama") => {
        std::env::set_var("OLLAMA_BASE_URL", ollama_url);
        ProviderClient::Ollama(OpenAiCompatClient::from_env(OpenAiCompatConfig::ollama())?)
    },
    _ => ProviderClient::from_model(&model)?,
};
```

**Step 3: Test and commit**

```bash
cd rust && cargo test --workspace
git add -A
git commit -m "feat: add --provider and --ollama-url CLI flags"
```

---

### Task 6.7: Add Mock Provider Test Server

**Files:**
- Modify: `rust/crates/mock-anthropic-service/src/lib.rs` — add Gemini + Ollama routes
- Modify: `rust/crates/mock-anthropic-service/src/main.rs`
- Rename crate: `mock-anthropic-service` → consider keeping name to minimize churn

**Step 1: Add Gemini mock endpoint**

```rust
// Route: POST /v1beta/models/:model:generateContent
async fn handle_gemini_generate(body: Value) -> impl IntoResponse {
    Json(serde_json::json!({
        "candidates": [{
            "content": {
                "parts": [{"text": "Mock Gemini response"}],
                "role": "model"
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 10,
            "candidatesTokenCount": 5
        }
    }))
}
```

**Step 2: Add Ollama mock endpoint**

```rust
// Route: POST /v1/chat/completions (OpenAI-compat format)
// Already partially handled — verify Ollama-specific behavior
```

**Step 3: Write provider integration tests**

```rust
#[tokio::test]
async fn gemini_provider_roundtrip_with_mock() {
    let server = start_mock_server().await;
    let client = GeminiClient::new("test-key", &server.url());
    let request = MessageRequest { /* ... */ };
    let response = client.send_message(&request).await.unwrap();
    assert!(!response.content.is_empty());
}

#[tokio::test]
async fn ollama_provider_roundtrip_with_mock() {
    let server = start_mock_server().await;
    std::env::set_var("OLLAMA_BASE_URL", server.url());
    let client = OpenAiCompatClient::from_env(OpenAiCompatConfig::ollama()).unwrap();
    let request = MessageRequest { /* ... */ };
    let response = client.send_message(&request).await.unwrap();
    assert!(!response.content.is_empty());
}
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add Gemini and Ollama mock endpoints for testing"
```

---

## Phase 7: CI & Distribution

### Task 7.1: Update CI Pipeline

**Files:**
- Modify: `.github/workflows/rust-ci.yml`

**Step 1: Ensure CI runs all checks**

```yaml
name: Rust CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Format check
        run: cd rust && cargo fmt --check
      - name: Clippy
        run: cd rust && cargo clippy --workspace --all-targets -- -D warnings
      - name: Test
        run: cd rust && cargo test --workspace
      - name: Build release
        run: cd rust && cargo build --release
```

**Step 2: Commit**

```bash
git add -A
git commit -m "ci: update rust-ci workflow with fmt, clippy, test, build"
```

---

### Task 7.2: Update Release Workflow

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Ensure release builds for all targets**

Add cross-compilation targets:
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Binary name: `ace` (not `claw`)

**Step 2: Commit**

```bash
git add -A
git commit -m "ci: update release workflow for ace binary across all targets"
```

---

## Phase 8: Commercial Packaging (Deferred)

### Task 8.1: Placeholder — License Crate Structure

**Note:** This task is deferred until initial release. Create the crate structure but don't implement gating.

**Files:**
- Create: `rust/crates/license/Cargo.toml`
- Create: `rust/crates/license/src/lib.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseTier {
    Community,
    Pro,
    Enterprise,
}

impl Default for LicenseTier {
    fn default() -> Self { Self::Community }
}

pub fn current_tier() -> LicenseTier {
    // Future: check license key from env or config
    LicenseTier::Community
}

pub fn check_feature(_tier: LicenseTier, _feature: &str) -> bool {
    true // All features enabled for now
}
```

**Step 1: Create crate, add to workspace, commit**

```bash
git add -A
git commit -m "feat: add ace-license crate placeholder for future commercial tiers"
```

---

## Execution Checklist

| Phase | Tasks | Description |
|-------|-------|-------------|
| **0** | 0.1–0.6 | Fork, clean, rebrand, license |
| **1** | 1.1–1.3 | Bash validation, AskUserQuestion, RemoteTrigger |
| **2** | 2.1–2.8 | /diff, /commit, /pr, /issue, /export, /compact, /doctor, /review |
| **3** | 3.1–3.5 | /team, /subagent, /cron, /plugin, /hooks, /branch, /advisor |
| **4** | 4.1 | Remaining stub commands |
| **5** | 5.1–5.3 | McpAuth, compaction accuracy, error polish |
| **6** | 6.1–6.7 | Gemini client, Ollama support, CLI flags, mock tests |
| **7** | 7.1–7.2 | CI pipeline, release workflow |
| **8** | 8.1 | License crate placeholder |

Total: **~30 tasks**, each independently testable and committable.
