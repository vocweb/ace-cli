# Per-Project Settings Persistence — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rename `.claw/` → `.ace/` across the codebase, add auto-migration, and persist settings changed via slash commands to `.ace/settings.local.json`.

**Architecture:** Event-based — each slash command that changes a setting calls `on_setting_changed()` on `LiveCli`, which delegates to `LocalSettingsWriter` to merge the value into `.ace/settings.local.json`. On startup, `ConfigLoader::discover()` auto-migrates `.claw/` → `.ace/` if needed, then loads settings from the `.ace/` hierarchy.

**Tech Stack:** Rust, serde_json (already a dependency), std::fs

---

### Task 1: Rename `.claw` → `.ace` in `runtime/src/config.rs`

**Files:**
- Modify: `rust/crates/runtime/src/config.rs`

**Step 1: Update `default_config_home()`**

At line ~560, change the env var name and fallback paths:

```rust
pub fn default_config_home() -> PathBuf {
    std::env::var_os("ACE_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ace")))
        .unwrap_or_else(|| PathBuf::from(".ace"))
}
```

**Step 2: Update `discover()` paths**

At line ~242, replace all `.claw` references:

```rust
pub fn discover(&self) -> Vec<ConfigEntry> {
    let user_legacy_path = self.config_home.parent().map_or_else(
        || PathBuf::from(".ace.json"),
        |parent| parent.join(".ace.json"),
    );
    vec![
        ConfigEntry {
            source: ConfigSource::User,
            path: user_legacy_path,
        },
        ConfigEntry {
            source: ConfigSource::User,
            path: self.config_home.join("settings.json"),
        },
        ConfigEntry {
            source: ConfigSource::Project,
            path: self.cwd.join(".ace.json"),
        },
        ConfigEntry {
            source: ConfigSource::Project,
            path: self.cwd.join(".ace").join("settings.json"),
        },
        ConfigEntry {
            source: ConfigSource::Local,
            path: self.cwd.join(".ace").join("settings.local.json"),
        },
    ]
}
```

**Step 3: Update `read_optional_json_object()` legacy check**

At line ~675, change the legacy config filename:

```rust
let is_legacy_config = path.file_name().and_then(|name| name.to_str()) == Some(".ace.json");
```

**Step 4: Update all test code in config.rs**

Search for all `.claw` references in the test module (~lines 1264-2087) and replace with `.ace`. This includes:
- Config file paths in test setup (`.claw.json` → `.ace.json`)
- Directory names (`.claw/` → `.ace/`)
- `CLAW_CONFIG_HOME` env var → `ACE_CONFIG_HOME`

**Step 5: Run tests**

Run: `cd rust && cargo test -p runtime -- config`
Expected: All config tests pass with new paths.

**Step 6: Commit**

```bash
git add rust/crates/runtime/src/config.rs
git commit -m "refactor: rename .claw → .ace in config loader"
```

---

### Task 2: Rename `.claw` → `.ace` in remaining runtime crate files

**Files:**
- Modify: `rust/crates/runtime/src/stale_base.rs`
- Modify: `rust/crates/runtime/src/prompt.rs`
- Modify: `rust/crates/runtime/src/worker_boot.rs`
- Modify: `rust/crates/runtime/src/oauth.rs`

**Step 1: Update `stale_base.rs`**

Replace `.claw-base` with `.ace-base` in:
- `read_claw_base_file()` → rename to `read_ace_base_file()` (line ~27-28)
- Doc comments referencing `.claw-base`
- All test code that writes `.claw-base` files
- Function `resolve_expected_base()` doc comment (line ~39)

**Step 2: Update `prompt.rs`**

Replace all `.claw/CLAUDE.md`, `.claw/instructions.md`, `.claw/` references with `.ace/` equivalents (~20 references).

**Step 3: Update `worker_boot.rs`**

Replace `.claw` references (~2 at lines 575, 579, 1121) with `.ace`.

**Step 4: Update `oauth.rs`**

Replace `.claw` reference (~1 at line 346) with `.ace`.

**Step 5: Run tests**

Run: `cd rust && cargo test -p runtime`
Expected: All runtime tests pass.

**Step 6: Commit**

```bash
git add rust/crates/runtime/src/
git commit -m "refactor: rename .claw → .ace in runtime crate"
```

---

### Task 3: Rename `.claw` → `.ace` in `commands` crate

**Files:**
- Modify: `rust/crates/commands/src/lib.rs` (~25 references)

**Step 1: Replace all `.claw` references**

These are mostly in help text, skill/command discovery paths, and test assertions. Replace:
- `.claw/settings.json` → `.ace/settings.json`
- `.claw/settings.local.json` → `.ace/settings.local.json`
- `.claw.json` → `.ace.json`
- `.claw/skills/` → `.ace/skills/`
- `.claw/commands/` → `.ace/commands/`
- `.claw/agents/` → `.ace/agents/`
- `.claw/CLAUDE.md` → `.ace/CLAUDE.md`

**Step 2: Run tests**

Run: `cd rust && cargo test -p commands`
Expected: All command tests pass.

**Step 3: Commit**

```bash
git add rust/crates/commands/src/lib.rs
git commit -m "refactor: rename .claw → .ace in commands crate"
```

---

### Task 4: Rename `.claw` → `.ace` in `rusty-claude-cli` crate

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/init.rs` (~13 references)
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` (~6 references)
- Modify: `rust/crates/rusty-claude-cli/src/session_mgr.rs` (~3 references)
- Modify: `rust/crates/rusty-claude-cli/src/args.rs` (~6 references)
- Modify: `rust/crates/rusty-claude-cli/tests/cli_flags_and_config_defaults.rs` (~8 references)
- Modify: `rust/crates/rusty-claude-cli/tests/resume_slash_commands.rs` (~4 references)

**Step 1: Update `init.rs`**

- `GITIGNORE_ENTRIES` constant (line 12): `[".ace/settings.local.json", ".ace/sessions/"]`
- `claw_dir` variable → `ace_dir`, path `.claw` → `.ace` (line 83)
- Artifact names `.claw/` → `.ace/`, `.claw.json` → `.ace.json` (lines 85, 91)
- CLAUDE.md template text referencing `.claw` → `.ace` (line 213)
- All test assertions (lines 358-410)

**Step 2: Update `main.rs`**

Replace `.claw` references (~lines 953, 956, 987, 990, 1220, 1223) with `.ace`.

**Step 3: Update `session_mgr.rs`**

Replace `.claw/sessions/` paths (~lines 97, 98, 190, 359) with `.ace/sessions/`.

**Step 4: Update `args.rs`**

Replace `CLAW_CONFIG_HOME` references in test code (~lines 1237, 1240, 1271, 1274, 1440, 1443) with `ACE_CONFIG_HOME`.

**Step 5: Update test files**

- `cli_flags_and_config_defaults.rs`: Replace `.claw` paths and `CLAW_CONFIG_HOME` (~8 refs)
- `resume_slash_commands.rs`: Replace `.claw` paths (~4 refs)

**Step 6: Run tests**

Run: `cd rust && cargo test -p rusty-claude-cli`
Expected: All CLI tests pass.

**Step 7: Commit**

```bash
git add rust/crates/rusty-claude-cli/
git commit -m "refactor: rename .claw → .ace in CLI crate"
```

---

### Task 5: Rename `.claw` → `.ace` in `tools` crate and other files

**Files:**
- Modify: `rust/crates/tools/src/lib.rs` (~30 references)
- Modify: `rust/crates/tools/.gitignore`
- Modify: `rust/.gitignore`

**Step 1: Update `tools/src/lib.rs`**

Replace all `.claw` path references with `.ace` (~30 refs — mostly in tool descriptions, path constants, and tests).

**Step 2: Update `.gitignore` files**

- `rust/.gitignore`: `.clawd-agents/` — keep as-is unless this is `.claw`-derived (check context)
- `rust/crates/tools/.gitignore`: same check

**Step 3: Run full workspace tests**

Run: `cd rust && cargo test --workspace`
Expected: All tests pass.

**Step 4: Run clippy and fmt**

Run: `cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`
Expected: Clean.

**Step 5: Commit**

```bash
git add rust/
git commit -m "refactor: rename .claw → .ace in tools crate and gitignore"
```

---

### Task 6: Add auto-migration from `.claw/` → `.ace/`

**Files:**
- Modify: `rust/crates/runtime/src/config.rs`
- Test: `rust/crates/runtime/src/config.rs` (inline tests)

**Step 1: Write the failing test**

Add to the test module in `config.rs`:

```rust
#[test]
fn auto_migrates_claw_dir_to_ace_dir() {
    let root = temp_dir();
    let config_home = root.join("config");
    fs::create_dir_all(&config_home).expect("config home");

    // Create legacy .claw/ structure
    let claw_dir = root.join(".claw");
    fs::create_dir_all(claw_dir.join("sessions")).expect("claw sessions dir");
    fs::write(
        claw_dir.join("settings.json"),
        r#"{"model": "gemma4"}"#,
    )
    .expect("write settings");

    let loader = ConfigLoader::new(root.clone(), config_home);
    let config = loader.load().expect("load config");

    // .claw/ should be renamed to .ace/
    assert!(!root.join(".claw").exists(), ".claw/ should be gone");
    assert!(root.join(".ace").exists(), ".ace/ should exist");
    assert!(root.join(".ace").join("settings.json").exists());
    assert_eq!(config.model(), Some("gemma4"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn auto_migrates_claw_json_to_ace_json() {
    let root = temp_dir();
    let config_home = root.join("config");
    fs::create_dir_all(&config_home).expect("config home");

    fs::write(
        root.join(".claw.json"),
        r#"{"model": "test-model"}"#,
    )
    .expect("write claw json");

    let loader = ConfigLoader::new(root.clone(), config_home);
    let config = loader.load().expect("load config");

    assert!(!root.join(".claw.json").exists());
    assert!(root.join(".ace.json").exists());
    assert_eq!(config.model(), Some("test-model"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn does_not_migrate_if_ace_already_exists() {
    let root = temp_dir();
    let config_home = root.join("config");
    fs::create_dir_all(&config_home).expect("config home");

    // Both exist — don't overwrite .ace/
    let claw_dir = root.join(".claw");
    fs::create_dir_all(&claw_dir).expect("claw dir");
    fs::write(claw_dir.join("settings.json"), r#"{"model": "old"}"#).expect("write");

    let ace_dir = root.join(".ace");
    fs::create_dir_all(&ace_dir).expect("ace dir");
    fs::write(ace_dir.join("settings.json"), r#"{"model": "new"}"#).expect("write");

    let loader = ConfigLoader::new(root.clone(), config_home);
    let config = loader.load().expect("load config");

    // .claw/ should still exist (not deleted when .ace/ present)
    assert!(root.join(".claw").exists());
    assert_eq!(config.model(), Some("new"));

    fs::remove_dir_all(root).expect("cleanup");
}
```

**Step 2: Run tests to verify they fail**

Run: `cd rust && cargo test -p runtime -- auto_migrates`
Expected: FAIL — `migrate_legacy_paths` doesn't exist yet.

**Step 3: Implement migration in `ConfigLoader`**

Add a `migrate_legacy_paths()` method to `ConfigLoader` and call it at the start of `load()`:

```rust
impl ConfigLoader {
    /// Rename legacy `.claw` paths to `.ace` equivalents when the new
    /// paths do not already exist.
    fn migrate_legacy_paths(&self) {
        // Migrate project-level .claw/ → .ace/
        let claw_dir = self.cwd.join(".claw");
        let ace_dir = self.cwd.join(".ace");
        if claw_dir.is_dir() && !ace_dir.exists() {
            if fs::rename(&claw_dir, &ace_dir).is_ok() {
                eprintln!("Migrated .claw/ → .ace/");
            }
        }

        // Migrate project-level .claw.json → .ace.json
        let claw_json = self.cwd.join(".claw.json");
        let ace_json = self.cwd.join(".ace.json");
        if claw_json.is_file() && !ace_json.exists() {
            if fs::rename(&claw_json, &ace_json).is_ok() {
                eprintln!("Migrated .claw.json → .ace.json");
            }
        }

        // Migrate user-level ~/.claw.json → ~/.ace.json
        if let Some(parent) = self.config_home.parent() {
            let user_claw = parent.join(".claw.json");
            let user_ace = parent.join(".ace.json");
            if user_claw.is_file() && !user_ace.exists() {
                if fs::rename(&user_claw, &user_ace).is_ok() {
                    eprintln!("Migrated ~/.claw.json → ~/.ace.json");
                }
            }
        }

        // Migrate user-level ~/.claw/ → ~/.ace/  (config_home itself)
        if self.config_home.ends_with(".ace") {
            let claw_home = self.config_home.with_file_name(".claw");
            if claw_home.is_dir() && !self.config_home.exists() {
                if fs::rename(&claw_home, &self.config_home).is_ok() {
                    eprintln!("Migrated ~/.claw/ → ~/.ace/");
                }
            }
        }
    }
}
```

Then at the top of `load()` (line ~271):

```rust
pub fn load(&self) -> Result<RuntimeConfig, ConfigError> {
    self.migrate_legacy_paths();
    let mut merged = BTreeMap::new();
    // ... rest unchanged
```

**Step 4: Run tests to verify they pass**

Run: `cd rust && cargo test -p runtime -- auto_migrates`
Expected: PASS.

Run: `cd rust && cargo test -p runtime -- config`
Expected: All config tests pass.

**Step 5: Commit**

```bash
git add rust/crates/runtime/src/config.rs
git commit -m "feat: auto-migrate .claw/ → .ace/ on startup"
```

---

### Task 7: Add `LocalSettingsWriter` to runtime

**Files:**
- Modify: `rust/crates/runtime/src/config.rs`
- Test: `rust/crates/runtime/src/config.rs` (inline tests)

**Step 1: Write the failing tests**

```rust
#[test]
fn local_settings_writer_creates_file_and_writes_string_value() {
    let root = temp_dir();
    let ace_dir = root.join(".ace");
    fs::create_dir_all(&ace_dir).expect("create .ace dir");
    let path = ace_dir.join("settings.local.json");

    LocalSettingsWriter::save(&path, "model", JsonValue::String("gemma4".into()))
        .expect("save model");

    let contents = fs::read_to_string(&path).expect("read file");
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("parse json");
    assert_eq!(parsed["model"], "gemma4");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn local_settings_writer_merges_into_existing_file() {
    let root = temp_dir();
    let ace_dir = root.join(".ace");
    fs::create_dir_all(&ace_dir).expect("create .ace dir");
    let path = ace_dir.join("settings.local.json");

    // Write initial setting
    LocalSettingsWriter::save(&path, "model", JsonValue::String("gemma4".into()))
        .expect("save model");

    // Write another setting — should merge, not overwrite
    LocalSettingsWriter::save(&path, "show_thinking", JsonValue::Bool(true))
        .expect("save thinking");

    let contents = fs::read_to_string(&path).expect("read file");
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("parse json");
    assert_eq!(parsed["model"], "gemma4");
    assert_eq!(parsed["show_thinking"], true);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn local_settings_writer_overwrites_existing_key() {
    let root = temp_dir();
    let ace_dir = root.join(".ace");
    fs::create_dir_all(&ace_dir).expect("create .ace dir");
    let path = ace_dir.join("settings.local.json");

    LocalSettingsWriter::save(&path, "model", JsonValue::String("gemma4".into()))
        .expect("first save");
    LocalSettingsWriter::save(&path, "model", JsonValue::String("opus".into()))
        .expect("second save");

    let contents = fs::read_to_string(&path).expect("read file");
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("parse json");
    assert_eq!(parsed["model"], "opus");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn local_settings_writer_creates_parent_dir_if_missing() {
    let root = temp_dir();
    let path = root.join(".ace").join("settings.local.json");

    // .ace/ doesn't exist yet
    LocalSettingsWriter::save(&path, "model", JsonValue::String("test".into()))
        .expect("save with missing parent");

    assert!(path.exists());
    let contents = fs::read_to_string(&path).expect("read");
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("parse");
    assert_eq!(parsed["model"], "test");

    fs::remove_dir_all(root).expect("cleanup");
}
```

**Step 2: Run tests to verify they fail**

Run: `cd rust && cargo test -p runtime -- local_settings_writer`
Expected: FAIL — `LocalSettingsWriter` doesn't exist.

**Step 3: Implement `LocalSettingsWriter`**

Add to `config.rs`, near the bottom but above the test module:

```rust
/// Reads and writes individual settings to a local JSON settings file.
/// Used to persist user changes from slash commands (e.g. `/model`, `/permissions`).
pub struct LocalSettingsWriter;

impl LocalSettingsWriter {
    /// Merge a single key-value pair into the given JSON settings file.
    /// Creates the file (and parent directories) if they don't exist.
    pub fn save(path: &Path, key: &str, value: JsonValue) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        }

        let mut object = match fs::read_to_string(path) {
            Ok(contents) if !contents.trim().is_empty() => {
                match JsonValue::parse(&contents) {
                    Ok(parsed) => parsed
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                    Err(_) => BTreeMap::new(),
                }
            }
            _ => BTreeMap::new(),
        };

        object.insert(key.to_string(), value);

        let json = JsonValue::Object(object);
        let formatted = serde_json::to_string_pretty(
            &serde_json::Value::from(json),
        ).map_err(|e| ConfigError::Parse(e.to_string()))?;

        fs::write(path, formatted).map_err(ConfigError::Io)?;
        Ok(())
    }

    /// Return the default local settings path for a working directory.
    pub fn default_path(cwd: &Path) -> PathBuf {
        cwd.join(".ace").join("settings.local.json")
    }
}
```

Note: The exact serialization approach depends on the project's `JsonValue` type. If it's a custom type, adapt accordingly. If the project uses `serde_json::Value` directly, simplify the conversion. Check how `JsonValue` serializes — there may be a `to_json_string()` or similar method already available.

**Step 4: Run tests to verify they pass**

Run: `cd rust && cargo test -p runtime -- local_settings_writer`
Expected: PASS.

**Step 5: Commit**

```bash
git add rust/crates/runtime/src/config.rs
git commit -m "feat: add LocalSettingsWriter for persisting slash command settings"
```

---

### Task 8: Wire up `on_setting_changed()` in LiveCli

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/cli.rs`

**Step 1: Add `on_setting_changed()` method to `LiveCli`**

```rust
/// Persist a setting change to the project-local settings file.
fn on_setting_changed(&self, key: &str, value: JsonValue) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let path = LocalSettingsWriter::default_path(&cwd);
    if let Err(e) = LocalSettingsWriter::save(&path, key, value) {
        eprintln!("warning: failed to persist setting: {e}");
    }
}
```

**Step 2: Wire into `set_model()` (line ~652)**

After `self.model.clone_from(&model);` (line 652), add:

```rust
self.on_setting_changed("model", JsonValue::String(model.clone()));
```

**Step 3: Wire into `set_permissions()` (line ~700)**

After `self.replace_runtime(runtime)?;` and before the print, add:

```rust
self.on_setting_changed("permissions", JsonValue::String(normalized.to_string()));
```

**Step 4: Wire into `/thinking` handler (line ~511)**

After `self.set_show_thinking(new_state);` (line 512), add:

```rust
self.on_setting_changed("show_thinking", JsonValue::Bool(new_state));
```

**Step 5: Add import for `LocalSettingsWriter` and `JsonValue`**

At the top of `cli.rs`, add import for `LocalSettingsWriter` from the runtime crate and whatever `JsonValue` type is used.

**Step 6: Run tests**

Run: `cd rust && cargo test --workspace`
Expected: All tests pass.

Run: `cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`
Expected: Clean.

**Step 7: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/cli.rs
git commit -m "feat: persist model, permissions, and thinking settings via slash commands"
```

---

### Task 9: Load persisted permissions and show_thinking on startup

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/args.rs`
- Modify: `rust/crates/rusty-claude-cli/src/tui/event.rs` (or wherever REPL init happens)

**Step 1: Add config loader for permissions**

In `args.rs`, add a function similar to `config_model_for_current_dir()`:

```rust
pub(crate) fn config_permission_mode_for_current_dir() -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().ok()?;
    config.get("permissions")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

pub(crate) fn config_show_thinking_for_current_dir() -> Option<bool> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().ok()?;
    config.get("show_thinking")
        .and_then(|v| v.as_bool())
}
```

Note: `RuntimeConfig` already parses `permission_mode` via `parse_optional_permission_mode()`. Check if it already handles the `"permissions"` key. If it uses a different key name, align with that. If `RuntimeConfig::permission_mode()` already returns the right value, use that instead.

**Step 2: Apply persisted values in REPL/TUI startup**

In the REPL initialization (wherever `LiveCli::new()` is called), after creating the CLI, apply persisted settings:

```rust
// After LiveCli::new(...)
if let Some(show_thinking) = config_show_thinking_for_current_dir() {
    cli.set_show_thinking(show_thinking);
}
```

For permissions: check if `resolve_repl_model()` pattern can be replicated — if user didn't pass `--permissions` flag explicitly, fall back to config file value.

**Step 3: Run tests**

Run: `cd rust && cargo test --workspace`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/
git commit -m "feat: load persisted permissions and show_thinking on startup"
```

---

### Task 10: Update `.claw-base` → `.ace-base` in stale_base.rs

**Files:**
- Modify: `rust/crates/runtime/src/stale_base.rs`

**Step 1: Rename function and update paths**

- `read_claw_base_file()` → `read_ace_base_file()`
- `.claw-base` → `.ace-base` in path construction
- Update doc comments
- Update all callers (grep for `read_claw_base_file`)

**Step 2: Update test code**

Replace all `.claw-base` file references in tests with `.ace-base`.

**Step 3: Run tests**

Run: `cd rust && cargo test -p runtime -- stale_base`
Expected: PASS.

**Step 4: Commit**

```bash
git add rust/crates/runtime/src/stale_base.rs
git commit -m "refactor: rename .claw-base → .ace-base"
```

---

### Task 11: Final verification and cleanup

**Files:**
- All files in `rust/`

**Step 1: Search for any remaining `.claw` references**

Run: `grep -r "\.claw" rust/ --include="*.rs" --include="*.toml"`
Expected: Zero results (or only in migration code that references legacy `.claw` paths).

**Step 2: Run full test suite**

Run: `cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: All clean.

**Step 3: Manual smoke test**

1. Build: `cd rust && cargo build`
2. Run: `./target/debug/ace --tui`
3. Type `/model gemma4` — should create `.ace/settings.local.json` with `{"model": "gemma4"}`
4. Exit and restart `./ace --tui`
5. Type `/model` — should show `gemma4` as current model
6. Test `/thinking` toggle persists across restart
7. Test `/permissions workspace-write` persists across restart

**Step 4: Test auto-migration**

1. Create a `.claw/settings.json` with `{"model": "test"}`
2. Run `./ace --tui`
3. Verify `.claw/` is renamed to `.ace/`
4. Verify model loads correctly

**Step 5: Commit any remaining fixes**

```bash
git add -A
git commit -m "chore: final cleanup for .claw → .ace rename and settings persistence"
```
