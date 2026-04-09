use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::error::ApiError;
use crate::http_client::build_http_client_or_default;
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

use super::preflight_message_request;

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(128);
const DEFAULT_MAX_RETRIES: u32 = 8;

#[derive(Debug, Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl GeminiClient {
    pub fn from_env() -> Result<Self, ApiError> {
        let Some(api_key) = read_env_non_empty("GEMINI_API_KEY")? else {
            return Err(ApiError::missing_credentials("Gemini", &["GEMINI_API_KEY"]));
        };
        Ok(Self::new(api_key, read_base_url()))
    }

    #[must_use]
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            http: build_http_client_or_default(),
            api_key,
            base_url,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn convert_request(&self, request: &MessageRequest) -> Value {
        convert_request(request)
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let request = MessageRequest {
            stream: false,
            ..request.clone()
        };
        preflight_message_request(&request)?;
        let response = self.send_with_retry(&request, false).await?;
        let request_id = request_id_from_headers(response.headers());
        let body = response.text().await.map_err(ApiError::from)?;
        let payload: Value = serde_json::from_str(&body).map_err(|error| {
            ApiError::json_deserialize("Gemini", &request.model, &body, error)
        })?;
        let mut normalized = convert_response(&request.model, payload)?;
        if normalized.request_id.is_none() {
            normalized.request_id = request_id;
        }
        Ok(normalized)
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<GeminiMessageStream, ApiError> {
        preflight_message_request(request)?;
        let stream_request = request.clone().with_streaming();
        let response = self.send_with_retry(&stream_request, true).await?;
        Ok(GeminiMessageStream {
            request_id: request_id_from_headers(response.headers()),
            response,
            parser: GeminiSseParser::with_context(request.model.clone()),
            pending: VecDeque::new(),
            done: false,
            state: StreamState::new(request.model.clone()),
        })
    }

    async fn send_with_retry(
        &self,
        request: &MessageRequest,
        streaming: bool,
    ) -> Result<reqwest::Response, ApiError> {
        let mut attempts = 0;

        let last_error = loop {
            attempts += 1;
            let retryable_error = match self.send_raw_request(request, streaming).await {
                Ok(response) => match expect_success(response).await {
                    Ok(response) => return Ok(response),
                    Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => {
                        error
                    }
                    Err(error) => return Err(error),
                },
                Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => error,
                Err(error) => return Err(error),
            };

            if attempts > self.max_retries {
                break retryable_error;
            }

            tokio::time::sleep(self.jittered_backoff_for_attempt(attempts)?).await;
        };

        Err(ApiError::RetriesExhausted {
            attempts,
            last_error: Box::new(last_error),
        })
    }

    async fn send_raw_request(
        &self,
        request: &MessageRequest,
        streaming: bool,
    ) -> Result<reqwest::Response, ApiError> {
        let url = if streaming {
            self.stream_endpoint(&request.model)
        } else {
            self.generate_endpoint(&request.model)
        };
        let body = convert_request(request);
        self.http
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(ApiError::from)
    }

    fn generate_endpoint(&self, model: &str) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        format!("{trimmed}/v1beta/models/{model}:generateContent?key={}", self.api_key)
    }

    fn stream_endpoint(&self, model: &str) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        format!(
            "{trimmed}/v1beta/models/{model}:streamGenerateContent?key={}&alt=sse",
            self.api_key
        )
    }

    fn backoff_for_attempt(&self, attempt: u32) -> Result<Duration, ApiError> {
        let Some(multiplier) = 1_u32.checked_shl(attempt.saturating_sub(1)) else {
            return Err(ApiError::BackoffOverflow {
                attempt,
                base_delay: self.initial_backoff,
            });
        };
        Ok(self
            .initial_backoff
            .checked_mul(multiplier)
            .map_or(self.max_backoff, |delay| delay.min(self.max_backoff)))
    }

    fn jittered_backoff_for_attempt(&self, attempt: u32) -> Result<Duration, ApiError> {
        let base = self.backoff_for_attempt(attempt)?;
        Ok(base + jitter_for_base(base))
    }
}

/// Process-wide counter that guarantees distinct jitter samples even when
/// the system clock resolution is coarser than consecutive retry sleeps.
static JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Returns a random additive jitter in `[0, base]` to decorrelate retries.
fn jitter_for_base(base: Duration) -> Duration {
    let base_nanos = u64::try_from(base.as_nanos()).unwrap_or(u64::MAX);
    if base_nanos == 0 {
        return Duration::ZERO;
    }
    let raw_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let tick = JITTER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut mixed = raw_nanos
        .wrapping_add(tick)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^= mixed >> 31;
    let jitter_nanos = mixed % base_nanos.saturating_add(1);
    Duration::from_nanos(jitter_nanos)
}

// ---------------------------------------------------------------------------
// Request conversion: internal format -> Gemini wire format
// ---------------------------------------------------------------------------

fn convert_request(request: &MessageRequest) -> Value {
    let mut body = json!({});

    // System instruction
    if let Some(system) = request.system.as_ref().filter(|s| !s.is_empty()) {
        body["systemInstruction"] = json!({
            "parts": [{"text": system}]
        });
    }

    // Contents (messages)
    let contents = translate_messages(&request.messages);
    body["contents"] = Value::Array(contents);

    // Generation config
    body["generationConfig"] = json!({
        "maxOutputTokens": request.max_tokens,
    });

    if let Some(temperature) = request.temperature {
        body["generationConfig"]["temperature"] = json!(temperature);
    }
    if let Some(top_p) = request.top_p {
        body["generationConfig"]["topP"] = json!(top_p);
    }
    if let Some(stop) = &request.stop {
        if !stop.is_empty() {
            body["generationConfig"]["stopSequences"] = json!(stop);
        }
    }

    // Tools
    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            let declarations: Vec<Value> = tools.iter().map(gemini_function_declaration).collect();
            body["tools"] = json!([{"functionDeclarations": declarations}]);
        }
    }

    // Tool choice
    if let Some(tool_choice) = &request.tool_choice {
        body["toolConfig"] = gemini_tool_config(tool_choice);
    }

    body
}

fn translate_messages(messages: &[InputMessage]) -> Vec<Value> {
    let mut contents = Vec::new();

    for message in messages {
        let role = match message.role.as_str() {
            "assistant" => "model",
            other => other,
        };

        let mut parts = Vec::new();
        let mut tool_response_parts: Vec<Value> = Vec::new();

        for block in &message.content {
            match block {
                InputContentBlock::Text { text } => {
                    if !text.is_empty() {
                        parts.push(json!({"text": text}));
                    }
                }
                InputContentBlock::ToolUse { name, input, .. } => {
                    parts.push(json!({
                        "functionCall": {
                            "name": name,
                            "args": input,
                        }
                    }));
                }
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error: _,
                } => {
                    // Gemini uses functionResponse. The tool_use_id serves as
                    // the function name fallback; in practice, the caller pairs
                    // this with a ToolUse block that carries the real name, but
                    // the id is the best we have at the wire-translation layer.
                    let response_text = flatten_tool_result_content(content);
                    tool_response_parts.push(json!({
                        "functionResponse": {
                            "name": tool_use_id,
                            "response": {
                                "content": response_text,
                            }
                        }
                    }));
                }
            }
        }

        // Emit regular content parts as one message
        if !parts.is_empty() {
            contents.push(json!({
                "role": role,
                "parts": parts,
            }));
        }

        // Tool responses must be under role "user"
        if !tool_response_parts.is_empty() {
            contents.push(json!({
                "role": "user",
                "parts": tool_response_parts,
            }));
        }
    }

    contents
}

fn flatten_tool_result_content(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|block| match block {
            ToolResultContentBlock::Text { text } => text.clone(),
            ToolResultContentBlock::Json { value } => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn gemini_function_declaration(tool: &ToolDefinition) -> Value {
    let mut decl = json!({
        "name": tool.name,
    });
    if let Some(desc) = &tool.description {
        decl["description"] = json!(desc);
    }
    // Gemini expects "parameters" as the schema object
    let mut parameters = tool.input_schema.clone();
    normalize_object_schema(&mut parameters);
    decl["parameters"] = parameters;
    decl
}

/// Recursively ensure every object-type node in a JSON Schema has
/// `"properties"` (at least `{}`).
fn normalize_object_schema(schema: &mut Value) {
    if let Some(obj) = schema.as_object_mut() {
        if obj.get("type").and_then(Value::as_str) == Some("object") {
            obj.entry("properties").or_insert_with(|| json!({}));
        }
        if let Some(props) = obj.get_mut("properties") {
            if let Some(props_obj) = props.as_object_mut() {
                let keys: Vec<String> = props_obj.keys().cloned().collect();
                for k in keys {
                    if let Some(v) = props_obj.get_mut(&k) {
                        normalize_object_schema(v);
                    }
                }
            }
        }
        if let Some(items) = obj.get_mut("items") {
            normalize_object_schema(items);
        }
    }
}

fn gemini_tool_config(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => json!({"functionCallingConfig": {"mode": "AUTO"}}),
        ToolChoice::Any => json!({"functionCallingConfig": {"mode": "ANY"}}),
        ToolChoice::Tool { name } => json!({
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": [name],
            }
        }),
    }
}

// ---------------------------------------------------------------------------
// Response conversion: Gemini wire format -> internal format
// ---------------------------------------------------------------------------

fn convert_response(model: &str, payload: Value) -> Result<MessageResponse, ApiError> {
    let candidates = payload["candidates"]
        .as_array()
        .ok_or(ApiError::InvalidSseFrame(
            "Gemini response missing candidates",
        ))?;
    let candidate = candidates
        .first()
        .ok_or(ApiError::InvalidSseFrame(
            "Gemini response has empty candidates",
        ))?;

    let mut content = Vec::new();
    if let Some(parts) = candidate["content"]["parts"].as_array() {
        for part in parts {
            if let Some(text) = part["text"].as_str() {
                if !text.is_empty() {
                    content.push(OutputContentBlock::Text {
                        text: text.to_string(),
                    });
                }
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or("").to_string();
                let args = fc.get("args").cloned().unwrap_or(json!({}));
                let id = format!("call_{name}_{}", content.len());
                content.push(OutputContentBlock::ToolUse {
                    id,
                    name,
                    input: args,
                });
            }
        }
    }

    let finish_reason = candidate["finishReason"]
        .as_str()
        .map(normalize_finish_reason);

    let usage = extract_usage(&payload);

    Ok(MessageResponse {
        id: payload["responseId"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content,
        model: model.to_string(),
        stop_reason: finish_reason,
        stop_sequence: None,
        usage,
        request_id: None,
    })
}

fn extract_usage(payload: &Value) -> Usage {
    let usage_meta = &payload["usageMetadata"];
    Usage {
        input_tokens: usage_meta["promptTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: usage_meta["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32,
    }
}

fn normalize_finish_reason(reason: &str) -> String {
    match reason {
        "STOP" => "end_turn".to_string(),
        "MAX_TOKENS" => "max_tokens".to_string(),
        "SAFETY" => "safety".to_string(),
        "RECITATION" => "recitation".to_string(),
        // Gemini uses this when a function call is the final output
        "FUNCTION_CALL" => "tool_use".to_string(),
        other => other.to_lowercase(),
    }
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct GeminiMessageStream {
    request_id: Option<String>,
    response: reqwest::Response,
    parser: GeminiSseParser,
    pending: VecDeque<StreamEvent>,
    done: bool,
    state: StreamState,
}

impl GeminiMessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }

            if self.done {
                self.pending.extend(self.state.finish()?);
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    for parsed in self.parser.push(&chunk)? {
                        self.pending.extend(self.state.ingest_chunk(parsed)?);
                    }
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct GeminiSseParser {
    buffer: Vec<u8>,
    model: String,
}

impl GeminiSseParser {
    fn with_context(model: impl Into<String>) -> Self {
        Self {
            buffer: Vec::new(),
            model: model.into(),
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(frame) = next_sse_frame(&mut self.buffer) {
            if let Some(event) = parse_sse_frame(&frame, &self.model)? {
                events.push(event);
            }
        }

        Ok(events)
    }
}

fn next_sse_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let separator = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| (position, 4))
        })?;

    let (position, separator_len) = separator;
    let frame = buffer.drain(..position + separator_len).collect::<Vec<_>>();
    let frame_len = frame.len().saturating_sub(separator_len);
    Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
}

fn parse_sse_frame(frame: &str, model: &str) -> Result<Option<Value>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut data_lines = Vec::new();
    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }
    serde_json::from_str::<Value>(&payload)
        .map(Some)
        .map_err(|error| ApiError::json_deserialize("Gemini", model, &payload, error))
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
struct StreamState {
    model: String,
    message_started: bool,
    text_started: bool,
    text_finished: bool,
    finished: bool,
    stop_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<usize, ToolCallState>,
}

impl StreamState {
    fn new(model: String) -> Self {
        Self {
            model,
            message_started: false,
            text_started: false,
            text_finished: false,
            finished: false,
            stop_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
        }
    }

    fn ingest_chunk(&mut self, chunk: Value) -> Result<Vec<StreamEvent>, ApiError> {
        let mut events = Vec::new();

        if !self.message_started {
            self.message_started = true;
            events.push(StreamEvent::MessageStart(MessageStartEvent {
                message: MessageResponse {
                    id: chunk["responseId"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    kind: "message".to_string(),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    model: self.model.clone(),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: Usage {
                        input_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        output_tokens: 0,
                    },
                    request_id: None,
                },
            }));
        }

        // Extract usage if present
        let chunk_usage = extract_usage(&chunk);
        if chunk_usage.input_tokens > 0 || chunk_usage.output_tokens > 0 {
            self.usage = Some(chunk_usage);
        }

        // Process candidates
        if let Some(candidates) = chunk["candidates"].as_array() {
            for candidate in candidates {
                if let Some(parts) = candidate["content"]["parts"].as_array() {
                    for part in parts {
                        // Text content
                        if let Some(text) = part["text"].as_str() {
                            if !text.is_empty() {
                                if !self.text_started {
                                    self.text_started = true;
                                    events.push(StreamEvent::ContentBlockStart(
                                        ContentBlockStartEvent {
                                            index: 0,
                                            content_block: OutputContentBlock::Text {
                                                text: String::new(),
                                            },
                                        },
                                    ));
                                }
                                events.push(StreamEvent::ContentBlockDelta(
                                    ContentBlockDeltaEvent {
                                        index: 0,
                                        delta: ContentBlockDelta::TextDelta {
                                            text: text.to_string(),
                                        },
                                    },
                                ));
                            }
                        }

                        // Function calls
                        if let Some(fc) = part.get("functionCall") {
                            let name =
                                fc["name"].as_str().unwrap_or("").to_string();
                            let args = fc
                                .get("args")
                                .cloned()
                                .unwrap_or(json!({}));
                            let idx = self.tool_calls.len();
                            let block_index = (idx + 1) as u32;
                            let id = format!("call_{name}_{idx}");
                            let state = ToolCallState {
                                name: name.clone(),
                                args: args.to_string(),
                                id: id.clone(),
                                started: true,
                                stopped: false,
                            };
                            self.tool_calls.insert(idx, state);

                            events.push(StreamEvent::ContentBlockStart(
                                ContentBlockStartEvent {
                                    index: block_index,
                                    content_block: OutputContentBlock::ToolUse {
                                        id,
                                        name,
                                        input: json!({}),
                                    },
                                },
                            ));
                            events.push(StreamEvent::ContentBlockDelta(
                                ContentBlockDeltaEvent {
                                    index: block_index,
                                    delta: ContentBlockDelta::InputJsonDelta {
                                        partial_json: args.to_string(),
                                    },
                                },
                            ));
                        }
                    }
                }

                // Check finish reason
                if let Some(reason) = candidate["finishReason"].as_str() {
                    self.stop_reason = Some(normalize_finish_reason(reason));

                    // Close tool call blocks
                    for (idx, state) in &mut self.tool_calls {
                        if state.started && !state.stopped {
                            state.stopped = true;
                            events.push(StreamEvent::ContentBlockStop(
                                ContentBlockStopEvent {
                                    index: (*idx as u32) + 1,
                                },
                            ));
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<StreamEvent>, ApiError> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.finished = true;

        let mut events = Vec::new();
        if self.text_started && !self.text_finished {
            self.text_finished = true;
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: 0,
            }));
        }

        for (idx, state) in &mut self.tool_calls {
            if state.started && !state.stopped {
                state.stopped = true;
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: (*idx as u32) + 1,
                }));
            }
        }

        if self.message_started {
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some(
                        self.stop_reason
                            .clone()
                            .unwrap_or_else(|| "end_turn".to_string()),
                    ),
                    stop_sequence: None,
                },
                usage: self.usage.clone().unwrap_or(Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                }),
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        Ok(events)
    }
}

#[derive(Debug)]
struct ToolCallState {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    args: String,
    #[allow(dead_code)]
    id: String,
    started: bool,
    stopped: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(super::dotenv_value(key)),
        Err(error) => Err(ApiError::from(error)),
    }
}

#[must_use]
pub fn read_base_url() -> String {
    std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

#[must_use]
pub fn has_api_key() -> bool {
    read_env_non_empty("GEMINI_API_KEY")
        .ok()
        .and_then(std::convert::identity)
        .is_some()
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let request_id = request_id_from_headers(response.headers());
    let body = response.text().await.unwrap_or_default();
    let retryable = is_retryable_status(status);

    // Gemini error format: {"error": {"code": N, "message": "...", "status": "..."}}
    let parsed_error = serde_json::from_str::<Value>(&body).ok();
    let error_type = parsed_error
        .as_ref()
        .and_then(|v| v["error"]["status"].as_str())
        .map(ToOwned::to_owned);
    let message = parsed_error
        .as_ref()
        .and_then(|v| v["error"]["message"].as_str())
        .map(ToOwned::to_owned);

    Err(ApiError::Api {
        status,
        error_type,
        message,
        request_id,
        body,
        retryable,
    })
}

const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 409 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputMessage, MessageRequest, ToolDefinition};
    use serde_json::json;

    #[test]
    fn gemini_client_converts_request() {
        let request = MessageRequest {
            model: "gemini-2.5-pro-preview-05-06".to_string(),
            max_tokens: 1024,
            messages: vec![InputMessage::user_text("Hello")],
            system: Some("You are helpful".to_string()),
            ..Default::default()
        };
        let body = convert_request(&request);
        assert!(body["contents"].is_array());
        assert_eq!(body["contents"][0]["role"], "user");
        assert!(body["systemInstruction"]["parts"][0]["text"]
            .as_str()
            .is_some());
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are helpful"
        );
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 1024);
    }

    #[test]
    fn converts_tool_definitions() {
        let request = MessageRequest {
            model: "gemini-2.5-pro-preview-05-06".to_string(),
            max_tokens: 1024,
            messages: vec![InputMessage::user_text("What is the weather?")],
            tools: Some(vec![ToolDefinition {
                name: "weather".to_string(),
                description: Some("Get weather".to_string()),
                input_schema: json!({"type": "object", "properties": {"city": {"type": "string"}}}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        let body = convert_request(&request);
        assert!(body["tools"][0]["functionDeclarations"].is_array());
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            "weather"
        );
        assert_eq!(
            body["toolConfig"]["functionCallingConfig"]["mode"],
            "AUTO"
        );
    }

    #[test]
    fn converts_response_with_text() {
        let payload = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello there!"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            },
            "responseId": "resp_123"
        });

        let response =
            convert_response("gemini-2.5-pro-preview-05-06", payload).expect("should convert");
        assert_eq!(response.id, "resp_123");
        assert_eq!(response.role, "assistant");
        assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            OutputContentBlock::Text { text } => assert_eq!(text, "Hello there!"),
            other => panic!("expected text block, got {other:?}"),
        }
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[test]
    fn converts_response_with_function_call() {
        let payload = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "weather",
                            "args": {"city": "Paris"}
                        }
                    }],
                    "role": "model"
                },
                "finishReason": "FUNCTION_CALL"
            }],
            "usageMetadata": {
                "promptTokenCount": 15,
                "candidatesTokenCount": 8
            },
            "responseId": "resp_456"
        });

        let response =
            convert_response("gemini-2.5-pro-preview-05-06", payload).expect("should convert");
        assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            OutputContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "weather");
                assert_eq!(input["city"], "Paris");
            }
            other => panic!("expected tool use block, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_gemini_finish_reasons() {
        assert_eq!(normalize_finish_reason("STOP"), "end_turn");
        assert_eq!(normalize_finish_reason("MAX_TOKENS"), "max_tokens");
        assert_eq!(normalize_finish_reason("FUNCTION_CALL"), "tool_use");
        assert_eq!(normalize_finish_reason("SAFETY"), "safety");
    }

    #[test]
    fn generate_endpoint_format() {
        let client = GeminiClient::new(
            "test-key".to_string(),
            "https://generativelanguage.googleapis.com".to_string(),
        );
        let url = client.generate_endpoint("gemini-2.5-pro-preview-05-06");
        assert!(url.contains("/v1beta/models/gemini-2.5-pro-preview-05-06:generateContent"));
        assert!(url.contains("key=test-key"));
    }

    #[test]
    fn stream_endpoint_format() {
        let client = GeminiClient::new(
            "test-key".to_string(),
            "https://generativelanguage.googleapis.com".to_string(),
        );
        let url = client.stream_endpoint("gemini-2.5-pro-preview-05-06");
        assert!(url.contains("/v1beta/models/gemini-2.5-pro-preview-05-06:streamGenerateContent"));
        assert!(url.contains("key=test-key"));
        assert!(url.contains("alt=sse"));
    }
}
