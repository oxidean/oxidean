//! Anthropic adapter — `POST {base}/v1/messages` with `stream: true`
//! (Messages SSE). Frames: `message_start` (input usage),
//! `content_block_start|delta|stop` (text_delta deltas; tool_use blocks
//! accumulate `input_json_delta` into `args_json` and emit `ToolCall` at
//! block_stop), `message_delta` (stop reason + output usage),
//! `message_stop` (terminal). `max_tokens` is API-required — 8192.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use oxidean_api_core::oxpilot::{
    ChatMsg, ChatRequest, ChatStream, ChatStreamItem, LlmProvider, ProviderError, Usage,
};
use serde_json::{json, Value};

use super::{classify_status, egress, send_bounded, stream::SseDecoder};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Messages API requires `max_tokens` — a generation ceiling, not a
/// billing dial (budgets are the row's job).
const MAX_TOKENS: u32 = 8192;

/// Construction-only — key material is resolved + decrypted inside the
/// providers tree (`pub(in crate::providers)` custody).
pub(in crate::providers) async fn build(
    cfg: &crate::settings::EffectiveProviderConfig,
) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    let api_key =
        super::key::resolve_material(cfg.key_ciphertext.as_deref(), cfg.api_key_env.as_deref())?
            .ok_or_else(|| ProviderError {
                code: "invalid_provider".to_string(),
                message: "anthropic requires API key material".to_string(),
            })?;
    let base_url = cfg
        .base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    egress::validate_base_url(Some("anthropic"), &base_url)
        .await
        .map_err(|e| ProviderError {
            code: "invalid_provider".to_string(),
            message: e,
        })?;
    Ok(Arc::new(AnthropicProvider {
        client: egress::provider_http_client(),
        base_url,
        api_key,
    }))
}

pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    /// Wire-codec test seam (NFR8.1) — see [`OpenAiProvider::new_for_testing`]
    /// (`super::openai`).
    #[doc(hidden)]
    pub fn new_for_testing(base_url: &str, api_key: &str) -> Self {
        Self {
            client: egress::provider_http_client(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let messages = anthropic_messages(&req.messages);
        let mut body = json!({
            "model": req.model,
            "max_tokens": MAX_TOKENS,
            "system": req.system,
            "messages": messages,
            "stream": true,
        });
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.parameters_json,
                        })
                    })
                    .collect(),
            );
        }
        let resp = send_bounded(
            self.client
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(&body),
        )
        .await?;
        if !resp.status().is_success() {
            return Err(classify_status(resp).await);
        }
        Ok(Box::pin(decode_stream(resp)))
    }
}

/// `ChatMsg` list → Anthropic's `messages` array. The runtime's tool
/// vocabulary maps onto Anthropic's content-block protocol: an assistant
/// msg carrying `tool_calls_json` (`[{id,name,args_json}]`) echoes as
/// `text` + `tool_use` blocks, and each `role:"tool"` msg becomes a
/// `tool_result` block inside a `user` message — consecutive results
/// share ONE user message because Anthropic requires role alternation.
fn anthropic_messages(msgs: &[ChatMsg]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(msgs.len());
    for m in msgs {
        if m.role == "tool" {
            let block = json!({
                "type": "tool_result",
                "tool_use_id": m.tool_call_id.as_deref().unwrap_or(""),
                "content": m.content,
            });
            // Merge into the tail user message when it's already a
            // tool_result carrier — never into a plain text user msg.
            let mergeable = out.last().is_some_and(|prev| {
                prev["role"].as_str() == Some("user")
                    && prev["content"]
                        .as_array()
                        .is_some_and(|a| a.iter().all(|b| b["type"] == "tool_result"))
            });
            if mergeable {
                if let Some(arr) = out.last_mut().and_then(|p| p["content"].as_array_mut()) {
                    arr.push(block);
                    continue;
                }
            }
            out.push(json!({"role": "user", "content": [block]}));
            continue;
        }
        if let Some(calls_json) = m.tool_calls_json.as_deref() {
            let mut content: Vec<Value> = Vec::new();
            if !m.content.is_empty() {
                content.push(json!({"type": "text", "text": m.content}));
            }
            if let Ok(Value::Array(calls)) = serde_json::from_str::<Value>(calls_json) {
                for c in calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": c["id"],
                        "name": c["name"],
                        "input": c["args_json"]
                            .as_str()
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or_else(|| json!({})),
                    }));
                }
            }
            out.push(json!({"role": "assistant", "content": content}));
            continue;
        }
        out.push(json!({"role": m.role, "content": m.content}));
    }
    out
}

/// One open content block — text blocks only need `text: true`; tool_use
/// blocks accumulate `input_json_delta` fragments.
struct BlockState {
    is_tool: bool,
    id: String,
    name: String,
    args: String,
}

/// Per-event payload → items/state. Returns `true` when the payload
/// logically ends the stream (`message_stop` / `error` frames).
fn anthropic_event(
    payload: &str,
    blocks: &mut BTreeMap<u64, BlockState>,
    usage: &mut Usage,
    out: &mut std::collections::VecDeque<ChatStreamItem>,
) -> bool {
    let v: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            out.push_back(ChatStreamItem::Error {
                code: "provider_error".to_string(),
                msg: format!("unparseable SSE payload: {e}"),
            });
            return false;
        }
    };
    match v["type"].as_str() {
        Some("message_start") => {
            usage.input_tokens = v["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
        }
        Some("content_block_start") => {
            let idx = v["index"].as_u64().unwrap_or(0);
            let block = &v["content_block"];
            let is_tool = block["type"].as_str() == Some("tool_use");
            blocks.insert(
                idx,
                BlockState {
                    is_tool,
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    args: String::new(),
                },
            );
        }
        Some("content_block_delta") => {
            let idx = v["index"].as_u64().unwrap_or(0);
            let delta = &v["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => {
                    if let Some(t) = delta["text"].as_str() {
                        if !t.is_empty() {
                            out.push_back(ChatStreamItem::Delta(t.to_string()));
                        }
                    }
                }
                Some("input_json_delta") => {
                    if let Some(block) = blocks.get_mut(&idx) {
                        if let Some(part) = delta["partial_json"].as_str() {
                            block.args.push_str(part);
                        }
                    }
                }
                _ => {}
            }
        }
        Some("content_block_stop") => {
            let idx = v["index"].as_u64().unwrap_or(0);
            if let Some(block) = blocks.remove(&idx) {
                if block.is_tool {
                    out.push_back(ChatStreamItem::ToolCall {
                        id: block.id,
                        name: block.name,
                        args_json: block.args,
                    });
                }
            }
        }
        Some("message_delta") => {
            usage.output_tokens = v["usage"]["output_tokens"].as_u64().unwrap_or(0);
        }
        Some("message_stop") => {
            out.push_back(ChatStreamItem::Done {
                usage: Some(*usage),
            });
            return true;
        }
        Some("error") => {
            let err = &v["error"];
            let kind = err["type"].as_str().unwrap_or("provider_error");
            let msg = err["message"].as_str().unwrap_or("provider error");
            // `overloaded_error`/`rate_limit_error` classify like the
            // status table; everything else is `provider_error`.
            let code = match kind {
                "rate_limit_error" => "provider_rate_limited",
                "overloaded_error" | "api_error" => "provider_unavailable",
                "invalid_request_error" if msg.to_ascii_lowercase().contains("context") => {
                    "context_length"
                }
                _ => "provider_error",
            };
            out.push_back(ChatStreamItem::Error {
                code: code.to_string(),
                msg: msg.to_string(),
            });
            return true;
        }
        _ => {} // ping, message_delta variants we don't consume
    }
    false
}

/// Byte stream → SSE events → `ChatStreamItem`s — `message_stop` (or a
/// terminal `error` frame) is the ONLY success signal; EOF without it
/// ends on `provider_error` (C4).
fn decode_stream(
    resp: reqwest::Response,
) -> impl futures_util::Stream<Item = ChatStreamItem> + Send {
    struct State {
        bytes: futures_util::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
        dec: SseDecoder,
        pending: std::collections::VecDeque<ChatStreamItem>,
        blocks: BTreeMap<u64, BlockState>,
        usage: Usage,
        eof: bool,
        emitted_done: bool,
    }
    futures_util::stream::unfold(
        State {
            bytes: resp.bytes_stream().boxed(),
            dec: SseDecoder::new(),
            pending: Default::default(),
            blocks: Default::default(),
            usage: Usage::default(),
            eof: false,
            emitted_done: false,
        },
        |mut st| async move {
            loop {
                if let Some(item) = st.pending.pop_front() {
                    return Some((item, st));
                }
                if st.eof {
                    if !st.emitted_done {
                        // Bare EOF — closed without `message_stop`
                        // (or a terminal `error` frame): a truncated
                        // stream is a provider error, never an implicit
                        // completion (C4).
                        st.emitted_done = true;
                        let err = ChatStreamItem::Error {
                            code: "provider_error".to_string(),
                            msg: "stream closed before message_stop".to_string(),
                        };
                        return Some((err, st));
                    }
                    return None;
                }
                let events = match st.bytes.next().await {
                    Some(Ok(chunk)) => st.dec.feed(&chunk),
                    Some(Err(e)) => {
                        st.pending.push_back(ChatStreamItem::Error {
                            code: "provider_unavailable".to_string(),
                            msg: e.to_string(),
                        });
                        // Terminal — the pending Error drains next
                        // iteration, then the eof arm returns None.
                        st.eof = true;
                        st.emitted_done = true;
                        continue;
                    }
                    None => {
                        st.eof = true;
                        st.dec.finish()
                    }
                };
                for payload in events {
                    if anthropic_event(&payload, &mut st.blocks, &mut st.usage, &mut st.pending) {
                        st.eof = true;
                        st.emitted_done = true;
                        break;
                    }
                }
            }
        },
    )
}
