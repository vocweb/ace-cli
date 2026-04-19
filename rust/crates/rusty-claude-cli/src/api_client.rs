// api_client.rs — API client, tool executor, and streaming helpers
//
// Extracted from main.rs. All items are pub(crate) so they remain
// accessible across the crate without leaking to downstream users.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use crate::render::{MarkdownStreamState, TerminalRenderer, ThinkingDisplay};
use api::{
    detect_provider_kind, AnthropicClient, ContentBlockDelta, InputContentBlock, InputMessage,
    MessageRequest, MessageResponse, OutputContentBlock, PromptCache,
    ProviderClient as ApiProviderClient, ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice,
    ToolResultContentBlock,
};
use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage, MessageRole,
    PermissionMode, PermissionPolicy, PromptCacheEvent, RuntimeError, ToolError, ToolExecutor,
};
use tools::GlobalToolRegistry;

use crate::args::{filter_tool_specs, AllowedToolSet};
use crate::formatting::{format_tool_call_start, format_tool_result, truncate_for_summary};
use crate::runtime_build::{
    InternalPromptProgressReporter, ListMcpResourcesRequest, McpToolRequest,
    ReadMcpResourceRequest, RuntimeMcpState, ToolSearchRequest,
};
use crate::{max_tokens_for_model, POST_TOOL_STALL_TIMEOUT};

// ---------------------------------------------------------------------------
// Batch 1: Hook/Permission reporters
// ---------------------------------------------------------------------------

pub(crate) struct CliHookProgressReporter;

impl runtime::HookProgressReporter for CliHookProgressReporter {
    fn on_event(&mut self, event: &runtime::HookProgressEvent) {
        match event {
            runtime::HookProgressEvent::Started {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
            runtime::HookProgressEvent::Completed {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook done {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
            runtime::HookProgressEvent::Cancelled {
                event,
                tool_name,
                command,
            } => eprintln!(
                "[hook cancelled {event_name}] {tool_name}: {command}",
                event_name = event.as_str()
            ),
        }
    }
}

pub(crate) struct CliPermissionPrompter {
    current_mode: PermissionMode,
}

impl CliPermissionPrompter {
    pub(crate) fn new(current_mode: PermissionMode) -> Self {
        Self { current_mode }
    }
}

impl runtime::PermissionPrompter for CliPermissionPrompter {
    fn decide(
        &mut self,
        request: &runtime::PermissionRequest,
    ) -> runtime::PermissionPromptDecision {
        println!();
        println!("Permission approval required");
        println!("  Tool             {}", request.tool_name);
        println!("  Current mode     {}", self.current_mode.as_str());
        println!("  Required mode    {}", request.required_mode.as_str());
        if let Some(reason) = &request.reason {
            println!("  Reason           {reason}");
        }
        println!("  Input            {}", request.input);
        print!("Approve this tool call? [y/N]: ");
        let _ = io::stdout().flush();

        let mut response = String::new();
        match io::stdin().read_line(&mut response) {
            Ok(_) => {
                let normalized = response.trim().to_ascii_lowercase();
                if matches!(normalized.as_str(), "y" | "yes") {
                    runtime::PermissionPromptDecision::Allow
                } else {
                    runtime::PermissionPromptDecision::Deny {
                        reason: format!(
                            "tool '{}' denied by user approval prompt",
                            request.tool_name
                        ),
                    }
                }
            }
            Err(error) => runtime::PermissionPromptDecision::Deny {
                reason: format!("permission approval failed: {error}"),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 2: AnthropicRuntimeClient
// ---------------------------------------------------------------------------

// NOTE: Despite the historical name `AnthropicRuntimeClient`, this struct
// now holds an `ApiProviderClient` which dispatches to Anthropic, xAI,
// OpenAI, or DashScope at construction time based on
// `detect_provider_kind(&model)`. The struct name is kept to avoid
// churning `BuiltRuntime` and every Deref/DerefMut site that references
// it. See ROADMAP #29 for the provider-dispatch routing fix.
pub(crate) struct AnthropicRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ApiProviderClient,
    session_id: String,
    model: String,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    progress_reporter: Option<InternalPromptProgressReporter>,
    reasoning_effort: Option<String>,
    show_thinking: bool,
}

impl AnthropicRuntimeClient {
    pub(crate) fn new(
        session_id: &str,
        model: String,
        enable_tools: bool,
        emit_output: bool,
        allowed_tools: Option<AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        progress_reporter: Option<InternalPromptProgressReporter>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Dispatch to the correct provider at construction time.
        // `ApiProviderClient` (exposed by the api crate as
        // `ProviderClient`) is an enum over Anthropic / xAI / OpenAI
        // variants, where xAI and OpenAI both use the OpenAI-compat
        // wire format under the hood. We consult
        // `detect_provider_kind(&resolved_model)` so model-name prefix
        // routing (`openai/`, `gpt-`, `grok`, `qwen/`) wins over
        // env-var presence.
        //
        // For Anthropic we build the client directly instead of going
        // through `ApiProviderClient::from_model_with_anthropic_auth`
        // so we can explicitly apply `api::read_base_url()` — that
        // reads `ANTHROPIC_BASE_URL` and is required for the local
        // mock-server test harness
        // (`crates/rusty-claude-cli/tests/compact_output.rs`) to point
        // ace at its fake Anthropic endpoint. We also attach a
        // session-scoped prompt cache on the Anthropic path; the
        // prompt cache is Anthropic-only so non-Anthropic variants
        // skip it.
        let resolved_model = api::resolve_model_alias(&model);
        let client = match detect_provider_kind(&resolved_model) {
            ProviderKind::Anthropic => {
                let auth = crate::oauth::resolve_cli_auth_source()?;
                let inner = AnthropicClient::from_auth(auth)
                    .with_base_url(api::read_base_url())
                    .with_prompt_cache(PromptCache::new(session_id));
                ApiProviderClient::Anthropic(inner)
            }
            ProviderKind::Xai
            | ProviderKind::OpenAi
            | ProviderKind::Gemini
            | ProviderKind::Ollama => {
                // The api crate's `ProviderClient::from_model_with_anthropic_auth`
                // with `None` for the anthropic auth routes via
                // `detect_provider_kind` and builds the matching provider
                // client internally. This covers OpenAI, OpenRouter, xAI,
                // DashScope, Gemini, Ollama, and any other endpoint users
                // configure via the provider-specific BASE_URL env var.
                ApiProviderClient::from_model_with_anthropic_auth(&resolved_model, None)?
            }
        };
        Ok(Self {
            runtime: tokio::runtime::Runtime::new()?,
            client,
            session_id: session_id.to_string(),
            model,
            enable_tools,
            emit_output,
            allowed_tools,
            tool_registry,
            progress_reporter,
            reasoning_effort: None,
            show_thinking: false,
        })
    }

    pub(crate) fn set_reasoning_effort(&mut self, effort: Option<String>) {
        self.reasoning_effort = effort;
    }

    pub(crate) fn set_show_thinking(&mut self, enabled: bool) {
        self.show_thinking = enabled;
    }
}

impl ApiClient for AnthropicRuntimeClient {
    #[allow(clippy::too_many_lines)]
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if let Some(progress_reporter) = &self.progress_reporter {
            progress_reporter.mark_model_phase();
        }
        let is_post_tool = request_ends_with_tool_result(&request);
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: self
                .enable_tools
                .then(|| filter_tool_specs(&self.tool_registry, self.allowed_tools.as_ref())),
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            stream: true,
            reasoning_effort: self.reasoning_effort.clone(),
            ..Default::default()
        };

        self.runtime.block_on(async {
            // When resuming after tool execution, apply a stall timeout on the
            // first stream event.  If the model does not respond within the
            // deadline we drop the stalled connection and re-send the request as
            // a continuation nudge (one retry only).
            let max_attempts: usize = if is_post_tool { 2 } else { 1 };

            for attempt in 1..=max_attempts {
                let result = self
                    .consume_stream(&message_request, is_post_tool && attempt == 1)
                    .await;
                match result {
                    Ok(events) => return Ok(events),
                    Err(error)
                        if error.to_string().contains("post-tool stall")
                            && attempt < max_attempts =>
                    {
                        // Stalled after tool completion — nudge the model by
                        // re-sending the same request.
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }

            Err(RuntimeError::new("post-tool continuation nudge exhausted"))
        })
    }
}

impl AnthropicRuntimeClient {
    /// Consume a single streaming response, optionally applying a stall
    /// timeout on the first event for post-tool continuations.
    #[allow(clippy::too_many_lines)]
    async fn consume_stream(
        &self,
        message_request: &MessageRequest,
        apply_stall_timeout: bool,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let mut stream = self
            .client
            .stream_message(message_request)
            .await
            .map_err(|error| {
                RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
            })?;
        let mut stdout = io::stdout();
        let mut sink = io::sink();
        let out: &mut dyn Write = if self.emit_output {
            &mut stdout
        } else {
            &mut sink
        };
        let renderer = TerminalRenderer::new();
        let mut markdown_stream = MarkdownStreamState::default();
        let mut events = Vec::new();
        let mut pending_tool: Option<(String, String, String)> = None;
        let mut block_has_thinking_summary = false;
        let mut saw_stop = false;
        let mut received_any_event = false;
        let mut thinking_display = ThinkingDisplay::new(7);
        let mut in_thinking_block = false;

        loop {
            let next = if apply_stall_timeout && !received_any_event {
                match tokio::time::timeout(POST_TOOL_STALL_TIMEOUT, stream.next_event()).await {
                    Ok(inner) => inner.map_err(|error| {
                        RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
                    })?,
                    Err(_elapsed) => {
                        return Err(RuntimeError::new(
                            "post-tool stall: model did not respond within timeout",
                        ));
                    }
                }
            } else {
                stream.next_event().await.map_err(|error| {
                    RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
                })?
            };

            let Some(event) = next else {
                break;
            };
            received_any_event = true;

            match event {
                ApiStreamEvent::MessageStart(start) => {
                    for block in start.message.content {
                        push_output_block(
                            block,
                            out,
                            &mut events,
                            &mut pending_tool,
                            true,
                            &mut block_has_thinking_summary,
                        )?;
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    in_thinking_block = matches!(
                        start.content_block,
                        OutputContentBlock::Thinking { .. }
                            | OutputContentBlock::RedactedThinking { .. }
                    );
                    if !(in_thinking_block && self.show_thinking) {
                        push_output_block(
                            start.content_block,
                            out,
                            &mut events,
                            &mut pending_tool,
                            true,
                            &mut block_has_thinking_summary,
                        )?;
                    }
                }
                ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if !text.is_empty() {
                            if let Some(progress_reporter) = &self.progress_reporter {
                                progress_reporter.mark_text_phase(&text);
                            }
                            if let Some(rendered) = markdown_stream.push(&renderer, &text) {
                                write!(out, "{rendered}")
                                    .and_then(|()| out.flush())
                                    .map_err(|error| RuntimeError::new(error.to_string()))?;
                            }
                            events.push(AssistantEvent::TextDelta(text));
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, _, input)) = &mut pending_tool {
                            input.push_str(&partial_json);
                        }
                    }
                    ContentBlockDelta::ThinkingDelta { thinking } => {
                        if self.show_thinking {
                            thinking_display.push(&thinking, out).map_err(
                                |error: std::io::Error| RuntimeError::new(error.to_string()),
                            )?;
                        } else if !block_has_thinking_summary {
                            render_thinking_block_summary(out, None, false)?;
                            block_has_thinking_summary = true;
                        }
                    }
                    ContentBlockDelta::SignatureDelta { .. } => {}
                },
                ApiStreamEvent::ContentBlockStop(_) => {
                    if in_thinking_block && self.show_thinking {
                        let char_count = thinking_display.total_chars();
                        thinking_display
                            .finish(out)
                            .map_err(|error: std::io::Error| {
                                RuntimeError::new(error.to_string())
                            })?;
                        // Emit a thinking summary event so it appears in session
                        // messages as a separate ContentBlock::Text.
                        events.push(AssistantEvent::ThinkingSummary { char_count });
                    }
                    in_thinking_block = false;
                    block_has_thinking_summary = false;
                    if let Some(rendered) = markdown_stream.flush(&renderer) {
                        write!(out, "{rendered}")
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                    }
                    if let Some((id, name, input)) = pending_tool.take() {
                        if let Some(progress_reporter) = &self.progress_reporter {
                            progress_reporter.mark_tool_phase(&name, &input);
                        }
                        // Display tool call now that input is fully accumulated
                        writeln!(out, "\n{}", format_tool_call_start(&name, &input))
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                        events.push(AssistantEvent::ToolUse { id, name, input });
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    events.push(AssistantEvent::Usage(delta.usage.token_usage()));
                }
                ApiStreamEvent::MessageStop(_) => {
                    saw_stop = true;
                    if let Some(rendered) = markdown_stream.flush(&renderer) {
                        write!(out, "{rendered}")
                            .and_then(|()| out.flush())
                            .map_err(|error| RuntimeError::new(error.to_string()))?;
                    }
                    events.push(AssistantEvent::MessageStop);
                }
            }
        }

        push_prompt_cache_record(&self.client, &mut events);

        if !saw_stop
            && events.iter().any(|event| {
                matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                    || matches!(event, AssistantEvent::ToolUse { .. })
            })
        {
            events.push(AssistantEvent::MessageStop);
        }

        if events
            .iter()
            .any(|event| matches!(event, AssistantEvent::MessageStop))
        {
            return Ok(events);
        }

        let response = self
            .client
            .send_message(&MessageRequest {
                stream: false,
                ..message_request.clone()
            })
            .await
            .map_err(|error| {
                RuntimeError::new(format_user_visible_api_error(&self.session_id, &error))
            })?;
        let mut events = response_to_events(response, out)?;
        push_prompt_cache_record(&self.client, &mut events);
        Ok(events)
    }
}

// ---------------------------------------------------------------------------
// Batch 3: Error handling & message processing
// ---------------------------------------------------------------------------

/// Returns `true` when the conversation ends with a tool-result message,
/// meaning the model is expected to continue after tool execution.
pub(crate) fn request_ends_with_tool_result(request: &ApiRequest) -> bool {
    request
        .messages
        .last()
        .is_some_and(|message| message.role == MessageRole::Tool)
}

pub(crate) fn format_user_visible_api_error(session_id: &str, error: &api::ApiError) -> String {
    if error.is_context_window_failure() {
        format_context_window_blocked_error(session_id, error)
    } else if error.is_generic_fatal_wrapper() {
        let mut qualifiers = vec![format!("session {session_id}")];
        if let Some(request_id) = error.request_id() {
            qualifiers.push(format!("trace {request_id}"));
        }
        format!(
            "{} ({}): {}",
            error.safe_failure_class(),
            qualifiers.join(", "),
            error
        )
    } else {
        error.to_string()
    }
}

pub(crate) fn format_context_window_blocked_error(
    session_id: &str,
    error: &api::ApiError,
) -> String {
    let mut lines = vec![
        "Context window blocked".to_string(),
        "  Failure class    context_window_blocked".to_string(),
        format!("  Session          {session_id}"),
    ];

    if let Some(request_id) = error.request_id() {
        lines.push(format!("  Trace            {request_id}"));
    }

    match error {
        api::ApiError::ContextWindowExceeded {
            model,
            estimated_input_tokens,
            requested_output_tokens,
            estimated_total_tokens,
            context_window_tokens,
        } => {
            lines.push(format!("  Model            {model}"));
            lines.push(format!(
                "  Input estimate   ~{estimated_input_tokens} tokens (heuristic)"
            ));
            lines.push(format!(
                "  Requested output {requested_output_tokens} tokens"
            ));
            lines.push(format!(
                "  Total estimate   ~{estimated_total_tokens} tokens (heuristic)"
            ));
            lines.push(format!("  Context window   {context_window_tokens} tokens"));
        }
        api::ApiError::Api { message, body, .. } => {
            let detail = message.as_deref().unwrap_or(body).trim();
            if !detail.is_empty() {
                lines.push(format!(
                    "  Detail           {}",
                    truncate_for_summary(detail, 120)
                ));
            }
        }
        api::ApiError::RetriesExhausted { last_error, .. } => {
            let detail = match last_error.as_ref() {
                api::ApiError::Api { message, body, .. } => message.as_deref().unwrap_or(body),
                other => return format_context_window_blocked_error(session_id, other),
            }
            .trim();
            if !detail.is_empty() {
                lines.push(format!(
                    "  Detail           {}",
                    truncate_for_summary(detail, 120)
                ));
            }
        }
        _ => {}
    }

    lines.push(String::new());
    lines.push("Recovery".to_string());
    lines.push("  Compact          /compact".to_string());
    lines.push(format!(
        "  Resume compact   ace --resume {session_id} /compact"
    ));
    lines.push("  Fresh session    /clear --confirm".to_string());
    lines.push(
        "  Reduce scope     remove large pasted context/files or ask for a smaller slice"
            .to_string(),
    );
    lines.push("  Retry            rerun after compacting or reducing the request".to_string());

    lines.join("\n")
}

pub(crate) fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

pub(crate) fn collect_tool_uses(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .assistant_messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
                "id": id,
                "name": name,
                "input": input,
            })),
            _ => None,
        })
        .collect()
}

pub(crate) fn collect_tool_results(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .tool_results
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Some(serde_json::json!({
                "tool_use_id": tool_use_id,
                "tool_name": tool_name,
                "output": output,
                "is_error": is_error,
            })),
            _ => None,
        })
        .collect()
}

pub(crate) fn collect_prompt_cache_events(
    summary: &runtime::TurnSummary,
) -> Vec<serde_json::Value> {
    summary
        .prompt_cache_events
        .iter()
        .map(|event| {
            serde_json::json!({
                "unexpected": event.unexpected,
                "reason": event.reason,
                "previous_cache_read_input_tokens": event.previous_cache_read_input_tokens,
                "current_cache_read_input_tokens": event.current_cache_read_input_tokens,
                "token_drop": event.token_drop,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Batch 4: Tool executor
// ---------------------------------------------------------------------------

pub(crate) struct CliToolExecutor {
    renderer: TerminalRenderer,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
}

impl CliToolExecutor {
    pub(crate) fn new(
        allowed_tools: Option<AllowedToolSet>,
        emit_output: bool,
        tool_registry: GlobalToolRegistry,
        mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    ) -> Self {
        Self {
            renderer: TerminalRenderer::new(),
            emit_output,
            allowed_tools,
            tool_registry,
            mcp_state,
        }
    }

    fn execute_search_tool(&self, value: serde_json::Value) -> Result<String, ToolError> {
        let input: ToolSearchRequest = serde_json::from_value(value)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let (pending_mcp_servers, mcp_degraded) =
            self.mcp_state.as_ref().map_or((None, None), |state| {
                let state = state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (state.pending_servers(), state.degraded_report())
            });
        serde_json::to_string_pretty(&self.tool_registry.search(
            &input.query,
            input.max_results.unwrap_or(5),
            pending_mcp_servers,
            mcp_degraded,
        ))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn execute_runtime_tool(
        &self,
        tool_name: &str,
        value: serde_json::Value,
    ) -> Result<String, ToolError> {
        let Some(mcp_state) = &self.mcp_state else {
            return Err(ToolError::new(format!(
                "runtime tool `{tool_name}` is unavailable without configured MCP servers"
            )));
        };
        let mut mcp_state = mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match tool_name {
            "MCPTool" => {
                let input: McpToolRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                let qualified_name = input
                    .qualified_name
                    .or(input.tool)
                    .ok_or_else(|| ToolError::new("missing required field `qualifiedName`"))?;
                mcp_state.call_tool(&qualified_name, input.arguments)
            }
            "ListMcpResourcesTool" => {
                let input: ListMcpResourcesRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                match input.server {
                    Some(server_name) => mcp_state.list_resources_for_server(&server_name),
                    None => mcp_state.list_resources_for_all_servers(),
                }
            }
            "ReadMcpResourceTool" => {
                let input: ReadMcpResourceRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                mcp_state.read_resource(&input.server, &input.uri)
            }
            _ => mcp_state.call_tool(tool_name, Some(value)),
        }
    }
}

impl ToolExecutor for CliToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let result = if tool_name == "ToolSearch" {
            self.execute_search_tool(value)
        } else if self.tool_registry.has_runtime_tool(tool_name) {
            self.execute_runtime_tool(tool_name, value)
        } else {
            self.tool_registry
                .execute(tool_name, &value)
                .map_err(ToolError::new)
        };
        match result {
            Ok(output) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &output, false);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|error: std::io::Error| ToolError::new(error.to_string()))?;
                }
                Ok(output)
            }
            Err(error) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &error.to_string(), true);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|stream_error: std::io::Error| {
                            ToolError::new(stream_error.to_string())
                        })?;
                }
                Err(error)
            }
        }
    }
}

pub(crate) fn permission_policy(
    mode: PermissionMode,
    feature_config: &runtime::RuntimeFeatureConfig,
    tool_registry: &GlobalToolRegistry,
) -> Result<PermissionPolicy, String> {
    Ok(tool_registry.permission_specs(None)?.into_iter().fold(
        PermissionPolicy::new(mode).with_permission_rules(feature_config.permission_rules()),
        |policy, (name, required_permission)| {
            policy.with_tool_requirement(name, required_permission)
        },
    ))
}

// ---------------------------------------------------------------------------
// Batch 5: Message conversion
// ---------------------------------------------------------------------------

pub(crate) fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Batch 6: Streaming/output helpers
// ---------------------------------------------------------------------------

pub(crate) fn response_to_events(
    response: MessageResponse,
    out: &mut (impl Write + ?Sized),
) -> Result<Vec<AssistantEvent>, RuntimeError> {
    let mut events = Vec::new();
    let mut pending_tool = None;

    for block in response.content {
        let mut block_has_thinking_summary = false;
        push_output_block(
            block,
            out,
            &mut events,
            &mut pending_tool,
            false,
            &mut block_has_thinking_summary,
        )?;
        if let Some((id, name, input)) = pending_tool.take() {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    events.push(AssistantEvent::Usage(response.usage.token_usage()));
    events.push(AssistantEvent::MessageStop);
    Ok(events)
}

pub(crate) fn push_output_block(
    block: OutputContentBlock,
    out: &mut (impl Write + ?Sized),
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    streaming_tool_input: bool,
    block_has_thinking_summary: &mut bool,
) -> Result<(), RuntimeError> {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let rendered = TerminalRenderer::new().markdown_to_ansi(&text);
                write!(out, "{rendered}")
                    .and_then(|()| out.flush())
                    .map_err(|error| RuntimeError::new(error.to_string()))?;
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            // During streaming, the initial content_block_start has an empty input ({}).
            // The real input arrives via input_json_delta events. In
            // non-streaming responses, preserve a legitimate empty object.
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { thinking, .. } => {
            render_thinking_block_summary(out, Some(thinking.chars().count()), false)?;
            *block_has_thinking_summary = true;
        }
        OutputContentBlock::RedactedThinking { .. } => {
            render_thinking_block_summary(out, None, true)?;
            *block_has_thinking_summary = true;
        }
    }
    Ok(())
}

pub(crate) fn push_prompt_cache_record(
    client: &ApiProviderClient,
    events: &mut Vec<AssistantEvent>,
) {
    // `ApiProviderClient::take_last_prompt_cache_record` is a pass-through
    // to the Anthropic variant and returns `None` for OpenAI-compat /
    // xAI variants, which do not have a prompt cache. So this helper
    // remains a no-op on non-Anthropic providers without any extra
    // branching here.
    if let Some(record) = client.take_last_prompt_cache_record() {
        if let Some(event) = prompt_cache_record_to_runtime_event(record) {
            events.push(AssistantEvent::PromptCache(event));
        }
    }
}

pub(crate) fn prompt_cache_record_to_runtime_event(
    record: api::PromptCacheRecord,
) -> Option<PromptCacheEvent> {
    let cache_break = record.cache_break?;
    Some(PromptCacheEvent {
        unexpected: cache_break.unexpected,
        reason: cache_break.reason,
        previous_cache_read_input_tokens: cache_break.previous_cache_read_input_tokens,
        current_cache_read_input_tokens: cache_break.current_cache_read_input_tokens,
        token_drop: cache_break.token_drop,
    })
}

pub(crate) fn render_thinking_block_summary(
    out: &mut (impl Write + ?Sized),
    char_count: Option<usize>,
    redacted: bool,
) -> Result<(), RuntimeError> {
    let summary = if redacted {
        "\n▶ Thinking block hidden by provider\n".to_string()
    } else if let Some(char_count) = char_count {
        format!("\n▶ Thinking ({char_count} chars hidden)\n")
    } else {
        "\n▶ Thinking hidden\n".to_string()
    };
    write!(out, "{summary}")
        .and_then(|()| out.flush())
        .map_err(|error| RuntimeError::new(error.to_string()))
}

/// Slash commands that are registered in the spec list but not yet implemented
/// in this build. Used to filter both REPL completions and help output so the
/// discovery surface only shows commands that actually work (ROADMAP #39).
pub(crate) const STUB_COMMANDS: &[&str] = &[];

pub(crate) fn slash_command_completion_candidates_with_sessions(
    model: &str,
    active_session_id: Option<&str>,
    recent_session_ids: Vec<String>,
) -> Vec<String> {
    use commands::slash_command_specs;

    let mut completions = BTreeSet::new();

    for spec in slash_command_specs() {
        if STUB_COMMANDS.contains(&spec.name) {
            continue;
        }
        completions.insert(format!("/{}", spec.name));
        for alias in spec.aliases {
            if !STUB_COMMANDS.contains(alias) {
                completions.insert(format!("/{alias}"));
            }
        }
    }

    for candidate in [
        "/clear --confirm",
        "/config ",
        "/config env",
        "/config hooks",
        "/config model",
        "/config plugins",
        "/mcp ",
        "/mcp list",
        "/mcp show ",
        "/export ",
        "/model ",
        "/model opus",
        "/model sonnet",
        "/model haiku",
        "/permissions ",
        "/permissions read-only",
        "/permissions workspace-write",
        "/permissions danger-full-access",
        "/plugin list",
        "/plugin install ",
        "/plugin enable ",
        "/plugin disable ",
        "/plugin uninstall ",
        "/plugin update ",
        "/plugins list",
        "/resume ",
        "/session list",
        "/session switch ",
        "/session fork ",
        "/teleport ",
        "/agents help",
        "/mcp help",
        "/skills help",
    ] {
        completions.insert(candidate.to_string());
    }

    if !model.trim().is_empty() {
        completions.insert(format!(
            "/model {}",
            crate::args::resolve_model_alias(model)
        ));
        completions.insert(format!("/model {model}"));
    }

    if let Some(active_session_id) = active_session_id.filter(|value| !value.trim().is_empty()) {
        completions.insert(format!("/resume {active_session_id}"));
        completions.insert(format!("/session switch {active_session_id}"));
    }

    for session_id in recent_session_ids
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .take(10)
    {
        completions.insert(format!("/resume {session_id}"));
        completions.insert(format!("/session switch {session_id}"));
    }

    completions.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        collect_prompt_cache_events, collect_tool_results, collect_tool_uses, convert_messages,
        format_context_window_blocked_error, format_user_visible_api_error, permission_policy,
        push_output_block, response_to_events, slash_command_completion_candidates_with_sessions,
        STUB_COMMANDS,
    };
    use api::{ApiError, MessageResponse, OutputContentBlock, Usage};
    use plugins::{PluginTool, PluginToolDefinition, PluginToolPermission};
    use runtime::{AssistantEvent, ContentBlock, ConversationMessage, MessageRole, PermissionMode};
    use serde_json::json;
    use tools::GlobalToolRegistry;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_env_lock()
    }

    fn registry_with_plugin_tool() -> GlobalToolRegistry {
        GlobalToolRegistry::with_plugin_tools(vec![PluginTool::new(
            "plugin-demo@external",
            "plugin-demo",
            PluginToolDefinition {
                name: "plugin_echo".to_string(),
                description: Some("Echo plugin payload".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }),
            },
            "echo".to_string(),
            Vec::new(),
            PluginToolPermission::WorkspaceWrite,
            None,
        )])
        .expect("plugin tool registry should build")
    }

    #[test]
    fn opaque_provider_wrapper_surfaces_failure_class_session_and_trace() {
        let error = ApiError::Api {
            status: "500".parse().expect("status"),
            error_type: Some("api_error".to_string()),
            message: Some(
                "Something went wrong while processing your request. Please try again, or use /new to start a fresh session."
                    .to_string(),
            ),
            request_id: Some("req_jobdori_789".to_string()),
            body: String::new(),
            retryable: true,
        };

        let rendered = format_user_visible_api_error("session-issue-22", &error);
        assert!(rendered.contains("provider_internal"));
        assert!(rendered.contains("session session-issue-22"));
        assert!(rendered.contains("trace req_jobdori_789"));
    }

    #[test]
    fn retry_exhaustion_uses_retry_failure_class_for_generic_provider_wrapper() {
        let error = ApiError::RetriesExhausted {
            attempts: 3,
            last_error: Box::new(ApiError::Api {
                status: "502".parse().expect("status"),
                error_type: Some("api_error".to_string()),
                message: Some(
                    "Something went wrong while processing your request. Please try again, or use /new to start a fresh session."
                        .to_string(),
                ),
                request_id: Some("req_jobdori_790".to_string()),
                body: String::new(),
                retryable: true,
            }),
        };

        let rendered = format_user_visible_api_error("session-issue-22", &error);
        assert!(rendered.contains("provider_retry_exhausted"), "{rendered}");
        assert!(rendered.contains("session session-issue-22"));
        assert!(rendered.contains("trace req_jobdori_790"));
    }

    #[test]
    fn context_window_preflight_errors_render_recovery_steps() {
        let error = ApiError::ContextWindowExceeded {
            model: "claude-sonnet-4-6".to_string(),
            estimated_input_tokens: 182_000,
            requested_output_tokens: 64_000,
            estimated_total_tokens: 246_000,
            context_window_tokens: 200_000,
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("Context window blocked"), "{rendered}");
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Session          session-issue-32"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Model            claude-sonnet-4-6"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Input estimate   ~182000 tokens (heuristic)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Total estimate   ~246000 tokens (heuristic)"),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Resume compact   ace --resume session-issue-32 /compact"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Fresh session    /clear --confirm"),
            "{rendered}"
        );
        assert!(rendered.contains("Reduce scope"), "{rendered}");
        assert!(rendered.contains("Retry            rerun"), "{rendered}");
    }

    #[test]
    fn provider_context_window_errors_are_reframed_with_same_guidance() {
        let error = ApiError::Api {
            status: "400".parse().expect("status"),
            error_type: Some("invalid_request_error".to_string()),
            message: Some(
                "This model's maximum context length is 200000 tokens, but your request used 230000 tokens."
                    .to_string(),
            ),
            request_id: Some("req_ctx_456".to_string()),
            body: String::new(),
            retryable: false,
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Trace            req_ctx_456"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("Detail           This model's maximum context length is 200000 tokens"),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Fresh session    /clear --confirm"),
            "{rendered}"
        );
    }

    #[test]
    fn retry_wrapped_context_window_errors_keep_recovery_guidance() {
        let error = ApiError::RetriesExhausted {
            attempts: 2,
            last_error: Box::new(ApiError::Api {
                status: "413".parse().expect("status"),
                error_type: Some("invalid_request_error".to_string()),
                message: Some("Request is too large for this model's context window.".to_string()),
                request_id: Some("req_ctx_retry_789".to_string()),
                body: String::new(),
                retryable: false,
            }),
        };

        let rendered = format_user_visible_api_error("session-issue-32", &error);
        assert!(rendered.contains("Context window blocked"), "{rendered}");
        assert!(rendered.contains("context_window_blocked"), "{rendered}");
        assert!(
            rendered.contains("Trace            req_ctx_retry_789"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("Detail           Request is too large for this model's context window."),
            "{rendered}"
        );
        assert!(rendered.contains("Compact          /compact"), "{rendered}");
        assert!(
            rendered.contains("Resume compact   ace --resume session-issue-32 /compact"),
            "{rendered}"
        );
    }

    #[test]
    fn permission_policy_uses_plugin_tool_permissions() {
        let feature_config = runtime::RuntimeFeatureConfig::default();
        let policy = permission_policy(
            PermissionMode::ReadOnly,
            &feature_config,
            &registry_with_plugin_tool(),
        )
        .expect("permission policy should build");
        let required = policy.required_mode_for("plugin_echo");
        assert_eq!(required, PermissionMode::WorkspaceWrite);
    }

    #[test]
    fn completion_candidates_include_workflow_shortcuts_and_dynamic_sessions() {
        let completions = slash_command_completion_candidates_with_sessions(
            "sonnet",
            Some("session-current"),
            vec!["session-old".to_string()],
        );

        assert!(completions.contains(&"/model claude-sonnet-4-6".to_string()));
        assert!(completions.contains(&"/permissions workspace-write".to_string()));
        assert!(completions.contains(&"/session list".to_string()));
        assert!(completions.contains(&"/session switch session-current".to_string()));
        assert!(completions.contains(&"/resume session-old".to_string()));
        assert!(completions.contains(&"/mcp list".to_string()));
        assert!(completions.contains(&"/agents help".to_string()));
    }

    #[test]
    fn converts_tool_roundtrip_messages() {
        let messages = vec![
            ConversationMessage::user_text("hello"),
            ConversationMessage::assistant(vec![ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "bash".to_string(),
                input: "{\"command\":\"pwd\"}".to_string(),
            }]),
            ConversationMessage {
                role: MessageRole::Tool,
                blocks: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    tool_name: "bash".to_string(),
                    output: "ok".to_string(),
                    is_error: false,
                }],
                usage: None,
            },
        ];

        let converted = convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "user");
    }

    #[test]
    fn push_output_block_renders_markdown_text() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;
        let mut block_has_thinking_summary = false;

        push_output_block(
            OutputContentBlock::Text {
                text: "# Heading".to_string(),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            false,
            &mut block_has_thinking_summary,
        )
        .expect("text block should render");

        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("Heading"));
        assert!(rendered.contains('\u{1b}'));
    }

    #[test]
    fn push_output_block_skips_empty_object_prefix_for_tool_streams() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;
        let mut block_has_thinking_summary = false;

        push_output_block(
            OutputContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: json!({}),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            true,
            &mut block_has_thinking_summary,
        )
        .expect("tool block should accumulate");

        assert!(events.is_empty());
        assert_eq!(
            pending_tool,
            Some(("tool-1".to_string(), "read_file".to_string(), String::new(),))
        );
    }

    #[test]
    fn response_to_events_preserves_empty_object_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-1".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{}"
        ));
    }

    #[test]
    fn response_to_events_preserves_non_empty_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-2".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-2".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "rust/Cargo.toml" }),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{\"path\":\"rust/Cargo.toml\"}"
        ));
    }

    #[test]
    fn response_to_events_renders_collapsed_thinking_summary() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-3".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![
                    OutputContentBlock::Thinking {
                        thinking: "step 1".to_string(),
                        signature: Some("sig_123".to_string()),
                    },
                    OutputContentBlock::Text {
                        text: "Final answer".to_string(),
                    },
                ],
                stop_reason: Some("end_turn".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::TextDelta(text) if text == "Final answer"
        ));
        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("▶ Thinking (6 chars hidden)"));
        assert!(!rendered.contains("step 1"));
    }

    #[test]
    fn stub_commands_absent_from_repl_completions() {
        let candidates =
            slash_command_completion_candidates_with_sessions("claude-3-5-sonnet", None, vec![]);
        for stub in STUB_COMMANDS {
            let with_slash = format!("/{stub}");
            assert!(
                !candidates.contains(&with_slash),
                "stub command {with_slash} should not appear in REPL completions"
            );
        }
    }
}
