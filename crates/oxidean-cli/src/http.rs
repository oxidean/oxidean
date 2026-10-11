//! Non-RPC HTTP surface — release assets are the one instance API that does
//! not ride `/api/rpc`: multipart `POST` for uploads and an ACL'd byte-stream
//! `GET` for downloads. Same Bearer auth as the RPC edge.

use serde_json::Value;

use crate::rpc::CallError;

/// `{instance}`-rooted HTTP client (no `/api/rpc` suffix).
pub struct HttpClient {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl HttpClient {
    pub fn new(instance: &str, token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: instance.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// `POST /api/repos/{owner}/{name}/releases/{release_id}/assets` —
    /// multipart field `asset`. Returns the response `data` payload.
    pub async fn upload_release_asset(
        &self,
        owner: &str,
        name: &str,
        release_id: &str,
        path: &std::path::Path,
    ) -> Result<Value, CallError> {
        let filename = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("asset")
            .to_string();
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| CallError::Transport(format!("reading {}: {e}", path.display())))?;
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let form = reqwest::multipart::Form::new().part("asset", part);
        let url = format!(
            "{}/api/repos/{}/{}/releases/{}/assets",
            self.base, owner, name, release_id
        );
        let mut req = self
            .http
            .post(&url)
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .multipart(form);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .map_err(|e| CallError::Transport(format!("request to {url} failed: {e}")))?;
        let status = res.status().as_u16();
        let text = res
            .text()
            .await
            .map_err(|e| CallError::Transport(format!("reading response failed: {e}")))?;
        let body: Value = serde_json::from_str(&text).map_err(|_| CallError::Http {
            status,
            body: crate::output::truncate(&text, 200),
        })?;
        if status >= 400 || body.get("ok") == Some(&Value::Bool(false)) {
            let msg = body
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("upload failed");
            let code = body
                .pointer("/error/code")
                .and_then(Value::as_str)
                .unwrap_or("release.upload_failed");
            return Err(CallError::Transport(format!("{code}: {msg}")));
        }
        Ok(body.get("data").cloned().unwrap_or(body))
    }

    /// `GET /api/releases/assets/{id}` — raw bytes plus the server's
    /// `Content-Disposition` filename (asset names are opaque to the client).
    pub async fn download_release_asset(
        &self,
        asset_id: &str,
    ) -> Result<(String, Vec<u8>), CallError> {
        let url = format!("{}/api/releases/assets/{asset_id}", self.base);
        let mut req = self
            .http
            .get(&url)
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")));
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .map_err(|e| CallError::Transport(format!("request to {url} failed: {e}")))?;
        let status = res.status().as_u16();
        let filename = res
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|d| {
                d.split("filename=\"")
                    .nth(1)
                    .map(|f| f.trim_end_matches('"').to_string())
            })
            .unwrap_or_else(|| asset_id.to_string());
        let bytes = res
            .bytes()
            .await
            .map_err(|e| CallError::Transport(format!("reading response failed: {e}")))?;
        if status >= 400 {
            let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let msg = body
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| crate::output::truncate(&String::from_utf8_lossy(&bytes), 200));
            return Err(CallError::Http {
                status,
                body: msg,
            });
        }
        Ok((filename, bytes.to_vec()))
    }
}
