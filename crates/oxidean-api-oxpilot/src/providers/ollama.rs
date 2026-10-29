//! Ollama adapter — `POST {base}/api/chat` NDJSON (`stream: true`).
//! Each line is one event: partial content rides
//! `message.content` with `done: false`; the terminal line carries
//! `done: true` plus `prompt_eval_count`/`eval_count` usage. Tool calls
//! arrive on `message.tool_calls[].function` with `arguments` as a JSON
//! object — serialized verbatim into `args_json`. Ollama needs no key
//! material and is exempt from the public-IP egress bound (self-hosted,
//! commonly on `127.0.0.1`/LAN) — validated in `egress::validate_base_url`.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use oxidean_api_core::oxpilot::{
    ChatMsg, ChatRequest, ChatStream, ChatStreamItem, LlmProvider, ProviderError, Usage,
};
use serde_json::{json, Value};

use super::{classify_status, egress, send_bounded, stream::NdjsonDecoder};

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";

/// Construction-only — Ollama never touches key material (custody stays
/// opaque to the adapter; a stored key is simply ignored).
pub(in crate::providers) async fn build(
    cfg: &crate::settings::EffectiveProviderConfig,
) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    let base_url = cfg
        .base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    egress::validate_base_url(Some("ollama"), &base_url)
        .await
        .map_err(|e| ProviderError {
            code: "invalid_provider".to_string(),
            message: e,
        })?;
    Ok(Arc::new(OllamaProvider {
        // The self-host exemption skips the private-target bound, so
        // there are no validated answers to pin (None).
        client: egress::provider_http_client(None),
        base_url,
    }))
}

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    /// Wire-codec test seam (NFR8.1) — see [`OpenAiProvider::new_for_testing`]
    /// (`super::openai`). Keyless: no api_key parameter.
    #[doc(hidden)]
    pub fn new_for_testing(base_url: &str) -> Self {
        Self {
            client: egress::provider_http_client(None),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        messages.push(json!({"role": "system", "content": req.system}));
        for m in &req.messages {
            messages.push(ollama_msg(m));
        }
        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "stream": true,
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
                .post(format!("{}/api/chat", self.base_url))
                .json(&body),
        )
        .await?;
        if !resp.status().is_success() {
            return Err(classify_status(resp).await);
        }
        Ok(Box::pin(decode_stream(resp)))
    }
}

/// `ChatMsg` → the Ollama wire shape. `role:"tool"` carries `name`;
/// an assistant `tool_calls_json` (`[{id,name,args_json}]`) echoes as
/// Ollama's own `tool_calls` shape — `arguments` is the parsed object
/// (matching what Ollama emits), falling back to `{}` on unparsable
/// provider output.
fn ollama_msg(m: &ChatMsg) -> Value {
    let mut msg = json!({"role": m.role, "content": m.content});
    if m.role == "tool" {
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
                            "function": {
                                "name": c["name"],
                                "arguments": c["args_json"]
                                    .as_str()
                                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                    .unwrap_or_else(|| json!({})),
                            },
                        })
                    })
                    .collect(),
            );
        }
    }
    msg
}

/// One NDJSON line → items/state. Returns `true` on the terminal
/// `done: true` line.
fn ollama_line(
    payload: &str,
    usage: &mut Usage,
    out: &mut std::collections::VecDeque<ChatStreamItem>,
) -> bool {
    let v: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            out.push_back(ChatStreamItem::Error {
                code: "provider_error".to_string(),
                msg: format!("unparseable NDJSON line: {e}"),
            });
            return false;
        }
    };
    if let Some(err) = v["error"].as_str() {
        out.push_back(ChatStreamItem::Error {
            code: "provider_error".to_string(),
            msg: err.to_string(),
        });
        return true; // the error line is terminal
    }
    usage.input_tokens = v["prompt_eval_count"]
        .as_u64()
        .unwrap_or(usage.input_tokens);
    usage.output_tokens = v["eval_count"].as_u64().unwrap_or(usage.output_tokens);
    for tc in v["message"]["tool_calls"].as_array().into_iter().flatten() {
        let f = &tc["function"];
        let args_json = match &f["arguments"] {
            // Ollama passes arguments as a JSON object; tolerate a raw
            // string so a proxy-shaped response still round-trips.
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_back(ChatStreamItem::ToolCall {
            id: f["id"].as_str().unwrap_or_default().to_string(),
            name: f["name"].as_str().unwrap_or_default().to_string(),
            args_json,
        });
    }
    if let Some(t) = v["message"]["content"].as_str() {
        if !t.is_empty() {
            out.push_back(ChatStreamItem::Delta(t.to_string()));
        }
    }
    if v["done"].as_bool() == Some(true) {
        out.push_back(ChatStreamItem::Done {
            usage: Some(*usage),
        });
        return true;
    }
    false
}

/// Byte stream → NDJSON lines → `ChatStreamItem`s — a `done: true`
/// line is the ONLY success signal; EOF without it ends on
/// `provider_error` (C4).
fn decode_stream(
    resp: reqwest::Response,
) -> impl futures_util::Stream<Item = ChatStreamItem> + Send {
    struct State {
        bytes: futures_util::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
        dec: NdjsonDecoder,
        pending: std::collections::VecDeque<ChatStreamItem>,
        usage: Usage,
        eof: bool,
        emitted_done: bool,
    }
    futures_util::stream::unfold(
        State {
            bytes: resp.bytes_stream().boxed(),
            dec: NdjsonDecoder::new(),
            pending: Default::default(),
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
                        // Bare EOF — closed without a `done: true`
                        // line: a truncated stream is a provider
                        // error, never an implicit completion (C4).
                        st.emitted_done = true;
                        let err = ChatStreamItem::Error {
                            code: "provider_error".to_string(),
                            msg: "stream closed before done:true".to_string(),
                        };
                        return Some((err, st));
                    }
                    return None;
                }
                let lines = match st.bytes.next().await {
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
                for line in lines {
                    if ollama_line(&line, &mut st.usage, &mut st.pending) {
                        st.eof = true;
                        st.emitted_done = true;
                        break;
                    }
                }
            }
        },
    )
}
