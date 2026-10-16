use futures_util::{SinkExt, StreamExt};
use oxidean_api::{build_cors, router};
use oxidean_db::Database;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn system_echo_over_ws() {
    let cors = build_cors("development", None).unwrap();
    let app = router(Database::skipped(), cors);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let mut req = format!("ws://{addr}/api/rpc/ws")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Oxidean-RPC-Version", HeaderValue::from_static("1"));

    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws connect");
    ws.send(Message::Text(
        r#"{"procedure":"system.echo","input":{"message":"pong"}}"#.into(),
    ))
    .await
    .unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    let text = msg.into_text().unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["message"], "pong");
}
