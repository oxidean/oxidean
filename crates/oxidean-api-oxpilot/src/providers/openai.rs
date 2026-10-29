//! OpenAI adapter — `POST {base}/v1/chat/completions` with
//! `stream: true` (chat-completions SSE, `stream_options.include_usage`
//! so the terminal chunk carries token counts). Tool calls arrive as
//! indexed `delta.tool_calls` fragments — `id`/`name` land once,
//! `arguments` is a string assembled across chunks; the adapter emits
//! `ToolCall` items when `finish_reason: "tool_calls"` completes them.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use oxidean_api_core::oxpilot::{
    ChatMsg, ChatRequest, ChatStream, ChatStreamItem, LlmProvider, ProviderError, Usage,
};
use serde_json::{json, Value};

use super::{classify_status, egress, send_bounded, stream::SseDecoder};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Construction-only — key material is resolved + decrypted inside the
/// providers tree (`pub(in crate::providers)` custody), never crosses it.
pub(in crate::providers) async fn build(
    cfg: &crate::settings::EffectiveProviderConfig,
) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    let api_key =
        super::key::resolve_material(cfg.key_ciphertext.as_deref(), cfg.api_key_env.as_deref())?
            .ok_or_else(|| ProviderError {
                code: "invalid_provider".to_string(),
                message: "openai requires API key material".to_string(),
            })?;
    let base_url = cfg
        .base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    // Re-run the egress bound at construction — the boot/rebuild path
    // reaches build without settings::validate, and the bound is cheap.
    // The validated DNS answers pin into the client so connect-time
    // re-resolution can't rebind to an unchecked address.
    let pinned = egress::validate_base_url(Some("openai"), &base_url)
        .await
        .map_err(|e| ProviderError {
            code: "invalid_provider".to_string(),
            message: e,
        })?;
    Ok(Arc::new(OpenAiProvider {
        client: egress::provider_http_client(pinned.as_ref()),
        base_url,
        api_key,
    }))
}

pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiProvider {
    /// Wire-codec test seam (NFR8.1) — constructs the adapter against a
    /// loopback mock WITHOUT the egress bound. The bound itself is
    /// covered by `egress`/`settings::validate` tests; this exists so
    /// wiremock fixtures can drive `stream` on `127.0.0.1`.
    #[doc(hidden)]
    pub fn new_for_testing(base_url: &str, api_key: &str) -> Self {
        Self {
            client: egress::provider_http_client(None),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &'static str {
        "openai"
    }

    async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        messages.push(json!({"role": "system", "content": req.system}));
        for m in &req.messages {
            messages.push(openai_msg(m));
        }
        let mut body = json!({
            "model": req.model,
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters_json,
                            },
                        })
                    })
                    .collect(),
            );
        }
        let resp = send_bounded(
            self.client
                .post(format!("{}/v1/chat/completions", self.base_url))
                .bearer_auth(&self.api_key)
                .json(&body),
        )
        .await?;
        if !resp.status().is_success() {
            return Err(classify_status(resp).await);
        }
        Ok(Box::pin(decode_stream(resp)))
    }
}

/// `ChatMsg` → the OpenAI wire shape. A `role:"tool"` msg rides
/// `tool_call_id` (+`name`); an assistant msg carrying `tool_calls_json`
/// (the runtime's `[{id,name,args_json}]` mint) echoes as the provider's
/// `tool_calls` array — `arguments` stays the raw JSON string the wire
/// wants. Plain msgs are `{role, content}` unchanged.
fn openai_msg(m: &ChatMsg) -> Value {
    let mut msg = json!({"role": m.role, "content": m.content});
    if m.role == "tool" {
        if let Some(id) = &m.tool_call_id {
            msg["tool_call_id"] = json!(id);
        }
        if let Some(name) = &m.tool_name {
            msg["name"] = json!(name);
        }
    }
    if let Some(calls_json) = m.tool_calls_json.as_deref() {
        if let Ok(Value::Array(calls)) = serde_json::from_str::<Value>(calls_json) {
            msg["tool_calls"] = Value::Array(
                calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c["id"],
                            "type": "function",
                            "function": {
                                "name": c["name"],
                                "arguments": c["args_json"],
                            },
                        })
                    })
                    .collect(),
            );
        }
    }
    msg
}

/// Accumulator for one in-flight tool call — `id`/`name` land once,
/// `arguments` streams as string fragments.
#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    args: String,
}

/// Per-event payload → `ChatStreamItem`s + decoder state updates.
fn openai_event(
    payload: &str,
    calls: &mut BTreeMap<u64, PendingCall>,
    usage: &mut Option<Usage>,
    out: &mut std::collections::VecDeque<ChatStreamItem>,
) -> bool {
    if payload == "[DONE]" {
        out.push_back(ChatStreamItem::Done { usage: *usage });
        return true; // stream logically done
    }
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
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("provider error");
        out.push_back(ChatStreamItem::Error {
            code: "provider_error".to_string(),
            msg: msg.to_string(),
        });
        return true; // an in-band error payload is the last frame the API sends
    }
    if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
        *usage = Some(Usage {
            input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
        });
    }
    for choice in v["choices"].as_array().into_iter().flatten() {
        if let Some(d) = choice["delta"]["content"].as_str() {
            if !d.is_empty() {
                out.push_back(ChatStreamItem::Delta(d.to_string()));
            }
        }
        for tc in choice["delta"]["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let entry = calls.entry(tc["index"].as_u64().unwrap_or(0)).or_default();
            if let Some(id) = tc["id"].as_str() {
                entry.id = id.to_string();
            }
            if let Some(name) = tc["function"]["name"].as_str() {
                entry.name = name.to_string();
            }
            if let Some(args) = tc["function"]["arguments"].as_str() {
                entry.args.push_str(args);
            }
        }
        if choice["finish_reason"].as_str() == Some("tool_calls") {
            for (_, call) in std::mem::take(calls) {
                out.push_back(ChatStreamItem::ToolCall {
                    id: call.id,
                    name: call.name,
                    args_json: call.args,
                });
            }
        }
    }
    false
}

/// Byte stream → SSE events → `ChatStreamItem`s. `usage` rides the final
/// `data:` chunk (include_usage); `data: \[DONE\]` is the ONLY success
/// signal — a provider that closes without it ends on `provider_error`
/// (C4 — a truncated stream is never an implicit completion).
fn decode_stream(
    resp: reqwest::Response,
) -> impl futures_util::Stream<Item = ChatStreamItem> + Send {
    struct State {
        bytes: futures_util::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
        dec: SseDecoder,
        pending: std::collections::VecDeque<ChatStreamItem>,
        calls: BTreeMap<u64, PendingCall>,
        usage: Option<Usage>,
        eof: bool,
        emitted_done: bool,
    }
    futures_util::stream::unfold(
        State {
            bytes: resp.bytes_stream().boxed(),
            dec: SseDecoder::new(),
            pending: Default::default(),
            calls: Default::default(),
            usage: None,
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
                        // Bare EOF — the wire closed without `data:
                        // [DONE]`. A truncated stream is a provider
                        // error, never an implicit completion (C4: the
                        // marker is the ONLY success signal).
                        st.emitted_done = true;
                        let err = ChatStreamItem::Error {
                            code: "provider_error".to_string(),
                            msg: "stream closed before [DONE]".to_string(),
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
                    let done =
                        openai_event(&payload, &mut st.calls, &mut st.usage, &mut st.pending);
                    if done {
                        st.eof = true;
                        st.emitted_done = true;
                        break;
                    }
                }
            }
        },
    )
}
