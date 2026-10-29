//! U4 provider-adapter wire tests (NFR8.1/BR5.3) — Wiremock ONLY, never
//! a live provider call. Golden SSE/NDJSON fixtures drive the full
//! `ChatStreamItem` vocabulary (Delta → ToolCall → Done{usage}), the
//! shared status table classifies canned error replies, and the send
//! boundary proves sent-but-answered calls never retry.

use futures_util::StreamExt;
use oxidean_api_core::oxpilot::{ChatMsg, ChatRequest, ChatStreamItem, LlmProvider, ToolSpec};
use oxidean_api_oxpilot::providers::{anthropic, ollama, openai};
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request() -> ChatRequest {
    ChatRequest {
        model: "m-1".to_string(),
        system: "you are terse".to_string(),
        messages: vec![ChatMsg {
            role: "user".to_string(),
            content: "hi".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls_json: None,
        }],
        tools: vec![ToolSpec {
            name: "lookup".to_string(),
            description: "look a thing up".to_string(),
            parameters_json: json!({"type": "object", "properties": {"q": {"type": "string"}}}),
        }],
    }
}

async fn collect(stream: oxidean_api_core::oxpilot::ChatStream) -> Vec<ChatStreamItem> {
    let mut items = Vec::new();
    let mut s = stream;
    while let Some(i) = s.next().await {
        items.push(i);
    }
    items
}

fn kinds(items: &[ChatStreamItem]) -> Vec<String> {
    items
        .iter()
        .map(|i| match i {
            ChatStreamItem::Delta(d) => format!("delta:{d}"),
            ChatStreamItem::ToolCall {
                id,
                name,
                args_json,
            } => format!("tool:{id}:{name}:{args_json}"),
            ChatStreamItem::Done { usage } => format!(
                "done:{}:{}",
                usage.map(|u| u.input_tokens).unwrap_or(0),
                usage.map(|u| u.output_tokens).unwrap_or(0)
            ),
            ChatStreamItem::Error { code, msg } => format!("err:{code}:{msg}"),
        })
        .collect()
}

async fn posted_bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

// ---------- openai ----------

#[tokio::test]
async fn openai_golden_stream_deltas_toolcall_done_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(include_str!("fixtures/openai/sse_chat.txt")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = openai::OpenAiProvider::new_for_testing(&server.uri(), "sk-test");
    assert_eq!(provider.id(), "openai");
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hello",
            "delta: world",
            "tool:call_abc:get_weather:{\"city\":\"Paris\"}",
            "done:12:7",
        ]
    );
    let bodies = posted_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["model"], "m-1");
    assert_eq!(bodies[0]["stream"], true);
    assert_eq!(bodies[0]["stream_options"]["include_usage"], true);
    assert_eq!(bodies[0]["messages"][0]["role"], "system");
    assert_eq!(bodies[0]["messages"][0]["content"], "you are terse");
    assert_eq!(bodies[0]["messages"][1]["role"], "user");
    assert_eq!(bodies[0]["tools"][0]["function"]["name"], "lookup");
}

#[tokio::test]
async fn openai_compatible_keyless_sends_no_authorization_header() {
    // LiteLLM-style endpoint with no key — the wire must omit the
    // Authorization header entirely, not send an empty bearer.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(include_str!("fixtures/openai/sse_chat.txt")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = openai::OpenAiProvider::new_compat_for_testing(&server.uri());
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(kinds(&items).last().unwrap(), "done:12:7");
    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
    assert!(!reqs[0]
        .headers
        .iter()
        .any(|(k, _)| k.as_str() == "authorization"));
}

// ---------- anthropic ----------

#[tokio::test]
async fn anthropic_golden_stream_deltas_toolcall_done_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-ant"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(include_str!("fixtures/anthropic/sse_messages.txt")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = anthropic::AnthropicProvider::new_for_testing(&server.uri(), "sk-ant");
    assert_eq!(provider.id(), "anthropic");
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hi there",
            "tool:toolu_1:lookup:{\"q\":\"oxidean\"}",
            "done:15:9",
        ]
    );
    let bodies = posted_bodies(&server).await;
    assert_eq!(bodies[0]["model"], "m-1");
    assert_eq!(bodies[0]["system"], "you are terse");
    assert_eq!(bodies[0]["stream"], true);
    assert_eq!(bodies[0]["tools"][0]["name"], "lookup");
    assert_eq!(bodies[0]["tools"][0]["input_schema"]["type"], "object");
}

// ---------- ollama ----------

#[tokio::test]
async fn ollama_golden_stream_deltas_toolcall_done_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/x-ndjson")
                .set_body_string(include_str!("fixtures/ollama/ndjson_chat.txt")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = ollama::OllamaProvider::new_for_testing(&server.uri());
    assert_eq!(provider.id(), "ollama");
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hel",
            "delta:lo",
            "tool::search:{\"q\":\"oxidean\"}",
            "done:21:8",
        ]
    );
    let bodies = posted_bodies(&server).await;
    assert_eq!(bodies[0]["model"], "m-1");
    assert_eq!(bodies[0]["stream"], true);
    assert_eq!(bodies[0]["messages"][0]["role"], "system");
    assert_eq!(bodies[0]["tools"][0]["function"]["name"], "lookup");
}

// ---------- error-status set (shared classify table) ----------

#[tokio::test]
async fn error_status_table_classifies_across_adapters() {
    let cases: [(u16, &str, &str); 6] = [
        (401, "{}", "provider_unauthenticated"),
        (403, "{}", "provider_forbidden"),
        (429, "{}", "provider_rate_limited"),
        (
            400,
            "{\"error\":{\"message\":\"maximum context length exceeded\"}}",
            "context_length",
        ),
        (503, "{}", "provider_unavailable"),
        (301, "{}", "provider_redirect"),
    ];
    for (status, body, code) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        // The table is shared — prove it through two wire shapes.
        let openai_err = openai::OpenAiProvider::new_for_testing(&server.uri(), "k")
            .stream(request())
            .await
            .err()
            .expect("openai must error");
        assert_eq!(openai_err.code, code, "openai status {status}");
        let server2 = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .expect(1)
            .mount(&server2)
            .await;
        let ollama_err = ollama::OllamaProvider::new_for_testing(&server2.uri())
            .stream(request())
            .await
            .err()
            .expect("ollama must error");
        assert_eq!(ollama_err.code, code, "ollama status {status}");
    }
}

// ---------- send boundary (BR4.1) ----------

#[tokio::test]
async fn answered_requests_never_retry() {
    // A 5xx ANSWER is one wire-completed call — `.expect(1)` proves the
    // bounded send did not retry an already-transmitted request.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .expect(1)
        .mount(&server)
        .await;
    let err = ollama::OllamaProvider::new_for_testing(&server.uri())
        .stream(request())
        .await
        .err()
        .expect("500 must error");
    assert_eq!(err.code, "provider_unavailable");
    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
}

#[tokio::test]
async fn connect_refused_classifies_provider_unavailable() {
    // Closed loopback port → `is_connect()` path: at most one retry,
    // terminal `provider_unavailable` either way.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // port now refuses
    let err = ollama::OllamaProvider::new_for_testing(&format!("http://127.0.0.1:{port}"))
        .stream(request())
        .await
        .err()
        .expect("refused connect must error");
    assert_eq!(err.code, "provider_unavailable");
}

// ---------- truncated wire ⇒ provider_error, never Done (R-04) ----------

#[tokio::test]
async fn openai_truncated_stream_is_provider_error() {
    // TCP close mid-body without `data: [DONE]` — partial content must
    // not be misclassified complete (C4: the marker is the ONLY
    // success signal).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = openai::OpenAiProvider::new_for_testing(&server.uri(), "sk-test");
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hello",
            "err:provider_error:stream closed before [DONE]"
        ]
    );
}

#[tokio::test]
async fn anthropic_truncated_stream_is_provider_error() {
    // EOF without `message_stop` — same contract.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude\",\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = anthropic::AnthropicProvider::new_for_testing(&server.uri(), "sk-ant");
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hi",
            "err:provider_error:stream closed before message_stop"
        ]
    );
}

#[tokio::test]
async fn ollama_truncated_stream_is_provider_error() {
    // EOF without a `done: true` line — same contract.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                "{\"model\":\"llama3.1\",\"created_at\":\"t\",\"message\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"done\":false}\n",
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = ollama::OllamaProvider::new_for_testing(&server.uri());
    let items = collect(provider.stream(request()).await.unwrap()).await;
    assert_eq!(
        kinds(&items),
        vec![
            "delta:Hel",
            "err:provider_error:stream closed before done:true"
        ]
    );
}
