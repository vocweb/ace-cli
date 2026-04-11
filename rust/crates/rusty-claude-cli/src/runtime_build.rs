//! Runtime construction helpers: plugin state, MCP state, progress reporting,
//! and the top-level `build_runtime` / `build_runtime_with_plugin_state` functions.

use crate::args::AllowedToolSet;
use crate::formatting::{
    extract_tool_path, first_visible_line, summarize_tool_payload, truncate_for_summary,
};
use plugins::{PluginHooks, PluginManager, PluginManagerConfig, PluginRegistry};
use runtime::{
    load_system_prompt, CompactionConfig, ConfigLoader, McpServer, McpServerManager, McpServerSpec,
    McpTool, PermissionMode, Session, ToolError,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::env;
use std::io::{self, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tools::{GlobalToolRegistry, RuntimeToolDefinition};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(crate) const INTERNAL_PROGRESS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub(crate) type RuntimePluginStateBuildOutput = (
    Option<Arc<Mutex<RuntimeMcpState>>>,
    Vec<RuntimeToolDefinition>,
);

// ---------------------------------------------------------------------------
// Batch 1: Simple structs
// ---------------------------------------------------------------------------

pub(crate) struct RuntimePluginState {
    pub(crate) feature_config: runtime::RuntimeFeatureConfig,
    pub(crate) tool_registry: GlobalToolRegistry,
    pub(crate) plugin_registry: PluginRegistry,
    pub(crate) mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ToolSearchRequest {
    pub(crate) query: String,
    pub(crate) max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct McpToolRequest {
    #[serde(rename = "qualifiedName")]
    pub(crate) qualified_name: Option<String>,
    pub(crate) tool: Option<String>,
    pub(crate) arguments: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListMcpResourcesRequest {
    pub(crate) server: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReadMcpResourceRequest {
    pub(crate) server: String,
    pub(crate) uri: String,
}

// ---------------------------------------------------------------------------
// Batch 2: RuntimeMcpState
// ---------------------------------------------------------------------------

pub(crate) struct RuntimeMcpState {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) manager: McpServerManager,
    pub(crate) pending_servers: Vec<String>,
    pub(crate) degraded_report: Option<runtime::McpDegradedReport>,
}

impl RuntimeMcpState {
    pub(crate) fn new(
        runtime_config: &runtime::RuntimeConfig,
    ) -> Result<Option<(Self, runtime::McpToolDiscoveryReport)>, Box<dyn std::error::Error>> {
        let mut manager = McpServerManager::from_runtime_config(runtime_config);
        if manager.server_names().is_empty() && manager.unsupported_servers().is_empty() {
            return Ok(None);
        }

        let runtime = tokio::runtime::Runtime::new()?;
        let discovery = runtime.block_on(manager.discover_tools_best_effort());
        let pending_servers = discovery
            .failed_servers
            .iter()
            .map(|failure| failure.server_name.clone())
            .chain(
                discovery
                    .unsupported_servers
                    .iter()
                    .map(|server| server.server_name.clone()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let available_tools = discovery
            .tools
            .iter()
            .map(|tool| tool.qualified_name.clone())
            .collect::<Vec<_>>();
        let failed_server_names = pending_servers.iter().cloned().collect::<BTreeSet<_>>();
        let working_servers = manager
            .server_names()
            .into_iter()
            .filter(|server_name| !failed_server_names.contains(server_name))
            .collect::<Vec<_>>();
        let failed_servers =
            discovery
                .failed_servers
                .iter()
                .map(|failure| runtime::McpFailedServer {
                    server_name: failure.server_name.clone(),
                    phase: runtime::McpLifecyclePhase::ToolDiscovery,
                    error: runtime::McpErrorSurface::new(
                        runtime::McpLifecyclePhase::ToolDiscovery,
                        Some(failure.server_name.clone()),
                        failure.error.clone(),
                        std::collections::BTreeMap::new(),
                        true,
                    ),
                })
                .chain(discovery.unsupported_servers.iter().map(|server| {
                    runtime::McpFailedServer {
                        server_name: server.server_name.clone(),
                        phase: runtime::McpLifecyclePhase::ServerRegistration,
                        error: runtime::McpErrorSurface::new(
                            runtime::McpLifecyclePhase::ServerRegistration,
                            Some(server.server_name.clone()),
                            server.reason.clone(),
                            std::collections::BTreeMap::from([(
                                "transport".to_string(),
                                format!("{:?}", server.transport).to_ascii_lowercase(),
                            )]),
                            false,
                        ),
                    }
                }))
                .collect::<Vec<_>>();
        let degraded_report = (!failed_servers.is_empty()).then(|| {
            runtime::McpDegradedReport::new(
                working_servers,
                failed_servers,
                available_tools.clone(),
                available_tools,
            )
        });

        Ok(Some((
            Self {
                runtime,
                manager,
                pending_servers,
                degraded_report,
            },
            discovery,
        )))
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.block_on(self.manager.shutdown())?;
        Ok(())
    }

    pub(crate) fn pending_servers(&self) -> Option<Vec<String>> {
        (!self.pending_servers.is_empty()).then(|| self.pending_servers.clone())
    }

    pub(crate) fn degraded_report(&self) -> Option<runtime::McpDegradedReport> {
        self.degraded_report.clone()
    }

    pub(crate) fn server_names(&self) -> Vec<String> {
        self.manager.server_names()
    }

    pub(crate) fn call_tool(
        &mut self,
        qualified_tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<String, ToolError> {
        let response = self
            .runtime
            .block_on(self.manager.call_tool(qualified_tool_name, arguments))
            .map_err(|error| ToolError::new(error.to_string()))?;
        if let Some(error) = response.error {
            return Err(ToolError::new(format!(
                "MCP tool `{qualified_tool_name}` returned JSON-RPC error: {} ({})",
                error.message, error.code
            )));
        }

        let result = response.result.ok_or_else(|| {
            ToolError::new(format!(
                "MCP tool `{qualified_tool_name}` returned no result payload"
            ))
        })?;
        serde_json::to_string_pretty(&result).map_err(|error| ToolError::new(error.to_string()))
    }

    pub(crate) fn list_resources_for_server(
        &mut self,
        server_name: &str,
    ) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.list_resources(server_name))
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "resources": result.resources,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    pub(crate) fn list_resources_for_all_servers(&mut self) -> Result<String, ToolError> {
        let mut resources = Vec::new();
        let mut failures = Vec::new();

        for server_name in self.server_names() {
            match self
                .runtime
                .block_on(self.manager.list_resources(&server_name))
            {
                Ok(result) => resources.push(json!({
                    "server": server_name,
                    "resources": result.resources,
                })),
                Err(error) => failures.push(json!({
                    "server": server_name,
                    "error": error.to_string(),
                })),
            }
        }

        if resources.is_empty() && !failures.is_empty() {
            let message = failures
                .iter()
                .filter_map(|failure| failure.get("error").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ToolError::new(message));
        }

        serde_json::to_string_pretty(&json!({
            "resources": resources,
            "failures": failures,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    pub(crate) fn read_resource(
        &mut self,
        server_name: &str,
        uri: &str,
    ) -> Result<String, ToolError> {
        let result = self
            .runtime
            .block_on(self.manager.read_resource(server_name, uri))
            .map_err(|error| ToolError::new(error.to_string()))?;
        serde_json::to_string_pretty(&json!({
            "server": server_name,
            "contents": result.contents,
        }))
        .map_err(|error| ToolError::new(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Batch 3: BuiltRuntime
// ---------------------------------------------------------------------------

pub(crate) struct BuiltRuntime {
    pub(crate) runtime:
        Option<runtime::ConversationRuntime<crate::AnthropicRuntimeClient, crate::CliToolExecutor>>,
    pub(crate) plugin_registry: PluginRegistry,
    pub(crate) plugins_active: bool,
    pub(crate) mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    pub(crate) mcp_active: bool,
}

impl BuiltRuntime {
    pub(crate) fn new(
        runtime: runtime::ConversationRuntime<
            crate::AnthropicRuntimeClient,
            crate::CliToolExecutor,
        >,
        plugin_registry: PluginRegistry,
        mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            plugin_registry,
            plugins_active: true,
            mcp_state,
            mcp_active: true,
        }
    }

    pub(crate) fn with_hook_abort_signal(
        mut self,
        hook_abort_signal: runtime::HookAbortSignal,
    ) -> Self {
        let runtime = self
            .runtime
            .take()
            .expect("runtime should exist before installing hook abort signal");
        self.runtime = Some(runtime.with_hook_abort_signal(hook_abort_signal));
        self
    }

    pub(crate) fn shutdown_plugins(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.plugins_active {
            self.plugin_registry.shutdown()?;
            self.plugins_active = false;
        }
        Ok(())
    }

    pub(crate) fn shutdown_mcp(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.mcp_active {
            if let Some(mcp_state) = &self.mcp_state {
                mcp_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .shutdown()?;
            }
            self.mcp_active = false;
        }
        Ok(())
    }
}

impl Deref for BuiltRuntime {
    type Target =
        runtime::ConversationRuntime<crate::AnthropicRuntimeClient, crate::CliToolExecutor>;

    fn deref(&self) -> &Self::Target {
        self.runtime
            .as_ref()
            .expect("runtime should exist while built runtime is alive")
    }
}

impl DerefMut for BuiltRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.runtime
            .as_mut()
            .expect("runtime should exist while built runtime is alive")
    }
}

impl Drop for BuiltRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown_mcp();
        let _ = self.shutdown_plugins();
    }
}

// ---------------------------------------------------------------------------
// Batch 4: HookAbortMonitor
// ---------------------------------------------------------------------------

pub(crate) struct HookAbortMonitor {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl HookAbortMonitor {
    pub(crate) fn spawn(abort_signal: runtime::HookAbortSignal) -> Self {
        Self::spawn_with_waiter(abort_signal, move |stop_rx, abort_signal| {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            runtime.block_on(async move {
                let wait_for_stop = tokio::task::spawn_blocking(move || {
                    let _ = stop_rx.recv();
                });

                tokio::select! {
                    result = tokio::signal::ctrl_c() => {
                        if result.is_ok() {
                            abort_signal.abort();
                        }
                    }
                    _ = wait_for_stop => {}
                }
            });
        })
    }

    pub(crate) fn spawn_with_waiter<F>(
        abort_signal: runtime::HookAbortSignal,
        wait_for_interrupt: F,
    ) -> Self
    where
        F: FnOnce(std::sync::mpsc::Receiver<()>, runtime::HookAbortSignal) + Send + 'static,
    {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let join_handle = std::thread::spawn(move || wait_for_interrupt(stop_rx, abort_signal));

        Self {
            stop_tx: Some(stop_tx),
            join_handle: Some(join_handle),
        }
    }

    pub(crate) fn stop(mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 5: Internal Progress types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InternalPromptProgressState {
    pub(crate) command_label: &'static str,
    pub(crate) task_label: String,
    pub(crate) step: usize,
    pub(crate) phase: String,
    pub(crate) detail: Option<String>,
    pub(crate) saw_final_text: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalPromptProgressEvent {
    Started,
    Update,
    Heartbeat,
    Complete,
    Failed,
}

#[derive(Debug)]
pub(crate) struct InternalPromptProgressShared {
    pub(crate) state: Mutex<InternalPromptProgressState>,
    pub(crate) output_lock: Mutex<()>,
    pub(crate) started_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct InternalPromptProgressReporter {
    pub(crate) shared: Arc<InternalPromptProgressShared>,
}

#[derive(Debug)]
pub(crate) struct InternalPromptProgressRun {
    pub(crate) reporter: InternalPromptProgressReporter,
    pub(crate) heartbeat_stop: Option<mpsc::Sender<()>>,
    pub(crate) heartbeat_handle: Option<thread::JoinHandle<()>>,
}

impl InternalPromptProgressReporter {
    pub(crate) fn ultraplan(task: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Ultraplan",
                    task_label: task.to_string(),
                    step: 0,
                    phase: "planning started".to_string(),
                    detail: Some(format!("task: {task}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    pub(crate) fn emit(&self, event: InternalPromptProgressEvent, error: Option<&str>) {
        let snapshot = self.snapshot();
        let line = format_internal_prompt_progress_line(event, &snapshot, self.elapsed(), error);
        self.write_line(&line);
    }

    pub(crate) fn mark_model_phase(&self) {
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = if state.step == 1 {
                "analyzing request".to_string()
            } else {
                "reviewing findings".to_string()
            };
            state.detail = Some(format!("task: {}", state.task_label));
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    pub(crate) fn mark_tool_phase(&self, name: &str, input: &str) {
        let detail = describe_tool_progress(name, input);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = format!("running {name}");
            state.detail = Some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    pub(crate) fn mark_text_phase(&self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let detail = truncate_for_summary(first_visible_line(trimmed), 120);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            if state.saw_final_text {
                return;
            }
            state.saw_final_text = true;
            state.step += 1;
            state.phase = "drafting final plan".to_string();
            state.detail = (!detail.is_empty()).then_some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    pub(crate) fn emit_heartbeat(&self) {
        let snapshot = self.snapshot();
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Heartbeat,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    pub(crate) fn snapshot(&self) -> InternalPromptProgressState {
        self.shared
            .state
            .lock()
            .expect("internal prompt progress state poisoned")
            .clone()
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.shared.started_at.elapsed()
    }

    pub(crate) fn write_line(&self, line: &str) {
        let _guard = self
            .shared
            .output_lock
            .lock()
            .expect("internal prompt progress output lock poisoned");
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}

impl InternalPromptProgressRun {
    pub(crate) fn start_ultraplan(task: &str) -> Self {
        let reporter = InternalPromptProgressReporter::ultraplan(task);
        reporter.emit(InternalPromptProgressEvent::Started, None);

        let (heartbeat_stop, heartbeat_rx) = mpsc::channel();
        let heartbeat_reporter = reporter.clone();
        let heartbeat_handle = thread::spawn(move || loop {
            match heartbeat_rx.recv_timeout(INTERNAL_PROGRESS_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => heartbeat_reporter.emit_heartbeat(),
            }
        });

        Self {
            reporter,
            heartbeat_stop: Some(heartbeat_stop),
            heartbeat_handle: Some(heartbeat_handle),
        }
    }

    pub(crate) fn reporter(&self) -> InternalPromptProgressReporter {
        self.reporter.clone()
    }

    pub(crate) fn finish_success(&mut self) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Complete, None);
    }

    pub(crate) fn finish_failure(&mut self, error: &str) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Failed, Some(error));
    }

    pub(crate) fn stop_heartbeat(&mut self) {
        if let Some(sender) = self.heartbeat_stop.take() {
            let _ = sender.send(());
        }
        if let Some(handle) = self.heartbeat_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for InternalPromptProgressRun {
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

pub(crate) fn format_internal_prompt_progress_line(
    event: InternalPromptProgressEvent,
    snapshot: &InternalPromptProgressState,
    elapsed: Duration,
    error: Option<&str>,
) -> String {
    let elapsed_seconds = elapsed.as_secs();
    let step_label = if snapshot.step == 0 {
        "current step pending".to_string()
    } else {
        format!("current step {}", snapshot.step)
    };
    let mut status_bits = vec![step_label, format!("phase {}", snapshot.phase)];
    if let Some(detail) = snapshot
        .detail
        .as_deref()
        .filter(|detail| !detail.is_empty())
    {
        status_bits.push(detail.to_string());
    }
    let status = status_bits.join(" · ");
    match event {
        InternalPromptProgressEvent::Started => {
            format!(
                "🧭 {} status · planning started · {status}",
                snapshot.command_label
            )
        }
        InternalPromptProgressEvent::Update => {
            format!("… {} status · {status}", snapshot.command_label)
        }
        InternalPromptProgressEvent::Heartbeat => format!(
            "… {} heartbeat · {elapsed_seconds}s elapsed · {status}",
            snapshot.command_label
        ),
        InternalPromptProgressEvent::Complete => format!(
            "✔ {} status · completed · {elapsed_seconds}s elapsed · {} steps total",
            snapshot.command_label, snapshot.step
        ),
        InternalPromptProgressEvent::Failed => format!(
            "✘ {} status · failed · {elapsed_seconds}s elapsed · {}",
            snapshot.command_label,
            error.unwrap_or("unknown error")
        ),
    }
}

pub(crate) fn describe_tool_progress(name: &str, input: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()));
    match name {
        "bash" | "Bash" => {
            let command = parsed
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if command.is_empty() {
                "running shell command".to_string()
            } else {
                format!("command {}", truncate_for_summary(command.trim(), 100))
            }
        }
        "read_file" | "Read" => format!("reading {}", extract_tool_path(&parsed)),
        "write_file" | "Write" => format!("writing {}", extract_tool_path(&parsed)),
        "edit_file" | "Edit" => format!("editing {}", extract_tool_path(&parsed)),
        "glob_search" | "Glob" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("glob `{pattern}` in {scope}")
        }
        "grep_search" | "Grep" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("grep `{pattern}` in {scope}")
        }
        "web_search" | "WebSearch" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .map_or_else(
                || "running web search".to_string(),
                |query| format!("query {}", truncate_for_summary(query, 100)),
            ),
        _ => {
            let summary = summarize_tool_payload(input);
            if summary.is_empty() {
                format!("running {name}")
            } else {
                format!("{name}: {summary}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 6: Build functions
// ---------------------------------------------------------------------------

pub(crate) fn build_system_prompt() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(load_system_prompt(
        env::current_dir()?,
        crate::DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )?)
}

pub(crate) fn build_runtime_plugin_state() -> Result<RuntimePluginState, Box<dyn std::error::Error>>
{
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    build_runtime_plugin_state_with_loader(&cwd, &loader, &runtime_config)
}

pub(crate) fn build_runtime_plugin_state_with_loader(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> Result<RuntimePluginState, Box<dyn std::error::Error>> {
    let plugin_manager = build_plugin_manager(cwd, loader, runtime_config);
    let plugin_registry = plugin_manager.plugin_registry()?;
    let plugin_hook_config =
        runtime_hook_config_from_plugin_hooks(plugin_registry.aggregated_hooks()?);
    let feature_config = runtime_config
        .feature_config()
        .clone()
        .with_hooks(runtime_config.hooks().merged(&plugin_hook_config));
    let (mcp_state, runtime_tools) = build_runtime_mcp_state(runtime_config)?;
    let tool_registry = GlobalToolRegistry::with_plugin_tools(plugin_registry.aggregated_tools()?)?
        .with_runtime_tools(runtime_tools)?;
    Ok(RuntimePluginState {
        feature_config,
        tool_registry,
        plugin_registry,
        mcp_state,
    })
}

pub(crate) fn build_plugin_manager(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

pub(crate) fn resolve_plugin_path(cwd: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

pub(crate) fn runtime_hook_config_from_plugin_hooks(
    hooks: PluginHooks,
) -> runtime::RuntimeHookConfig {
    runtime::RuntimeHookConfig::new(
        hooks.pre_tool_use,
        hooks.post_tool_use,
        hooks.post_tool_use_failure,
    )
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_runtime(
    session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    let runtime_plugin_state = build_runtime_plugin_state()?;
    build_runtime_with_plugin_state(
        session,
        session_id,
        model,
        system_prompt,
        enable_tools,
        emit_output,
        allowed_tools,
        permission_mode,
        progress_reporter,
        runtime_plugin_state,
    )
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_runtime_with_plugin_state(
    mut session: Session,
    session_id: &str,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
    runtime_plugin_state: RuntimePluginState,
) -> Result<BuiltRuntime, Box<dyn std::error::Error>> {
    // Persist the model in session metadata so resumed sessions can report it.
    if session.model.is_none() {
        session.model = Some(model.clone());
    }
    let RuntimePluginState {
        feature_config,
        tool_registry,
        plugin_registry,
        mcp_state,
    } = runtime_plugin_state;
    plugin_registry.initialize()?;
    let policy = crate::permission_policy(permission_mode, &feature_config, &tool_registry)
        .map_err(std::io::Error::other)?;
    let mut runtime = runtime::ConversationRuntime::new_with_features(
        session,
        crate::AnthropicRuntimeClient::new(
            session_id,
            model,
            enable_tools,
            emit_output,
            allowed_tools.clone(),
            tool_registry.clone(),
            progress_reporter,
        )?,
        crate::CliToolExecutor::new(
            allowed_tools.clone(),
            emit_output,
            tool_registry.clone(),
            mcp_state.clone(),
        ),
        policy,
        system_prompt,
        &feature_config,
    );
    if emit_output {
        runtime = runtime.with_hook_progress_reporter(Box::new(crate::CliHookProgressReporter));
    }
    Ok(BuiltRuntime::new(runtime, plugin_registry, mcp_state))
}

// ---------------------------------------------------------------------------
// Batch 7: MCP helpers
// ---------------------------------------------------------------------------

pub(crate) fn build_runtime_mcp_state(
    runtime_config: &runtime::RuntimeConfig,
) -> Result<RuntimePluginStateBuildOutput, Box<dyn std::error::Error>> {
    let Some((mcp_state, discovery)) = RuntimeMcpState::new(runtime_config)? else {
        return Ok((None, Vec::new()));
    };

    let mut runtime_tools = discovery
        .tools
        .iter()
        .map(mcp_runtime_tool_definition)
        .collect::<Vec<_>>();
    if !mcp_state.server_names().is_empty() {
        runtime_tools.extend(mcp_wrapper_tool_definitions());
    }

    Ok((Some(Arc::new(Mutex::new(mcp_state))), runtime_tools))
}

pub(crate) fn mcp_runtime_tool_definition(tool: &runtime::ManagedMcpTool) -> RuntimeToolDefinition {
    RuntimeToolDefinition {
        name: tool.qualified_name.clone(),
        description: Some(
            tool.tool
                .description
                .clone()
                .unwrap_or_else(|| format!("Invoke MCP tool `{}`.", tool.qualified_name)),
        ),
        input_schema: tool
            .tool
            .input_schema
            .clone()
            .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true })),
        required_permission: permission_mode_for_mcp_tool(&tool.tool),
    }
}

pub(crate) fn mcp_wrapper_tool_definitions() -> Vec<RuntimeToolDefinition> {
    vec![
        RuntimeToolDefinition {
            name: "MCPTool".to_string(),
            description: Some(
                "Call a configured MCP tool by its qualified name and JSON arguments.".to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "qualifiedName": { "type": "string" },
                    "arguments": {}
                },
                "required": ["qualifiedName"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        RuntimeToolDefinition {
            name: "ListMcpResourcesTool".to_string(),
            description: Some(
                "List MCP resources from one configured server or from every connected server."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" }
                },
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        RuntimeToolDefinition {
            name: "ReadMcpResourceTool".to_string(),
            description: Some("Read a specific MCP resource from a configured server.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" },
                    "uri": { "type": "string" }
                },
                "required": ["server", "uri"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
    ]
}

pub(crate) fn permission_mode_for_mcp_tool(tool: &McpTool) -> PermissionMode {
    let read_only = mcp_annotation_flag(tool, "readOnlyHint");
    let destructive = mcp_annotation_flag(tool, "destructiveHint");
    let open_world = mcp_annotation_flag(tool, "openWorldHint");

    if read_only && !destructive && !open_world {
        PermissionMode::ReadOnly
    } else if destructive || open_world {
        PermissionMode::DangerFullAccess
    } else {
        PermissionMode::WorkspaceWrite
    }
}

pub(crate) fn mcp_annotation_flag(tool: &McpTool, key: &str) -> bool {
    tool.annotations
        .as_ref()
        .and_then(|annotations| annotations.get(key))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    crate::test_env_lock()
}
