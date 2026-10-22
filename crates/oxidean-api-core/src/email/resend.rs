//! Resend HTTP email adapter (AUTH-11).

use reqwest::Client;
use serde_json::json;

use crate::email::{EmailError, EmailSender, OutboundEmail};

const DEFAULT_BASE_URL: &str = "https://api.resend.com";
const EMAILS_PATH: &str = "/emails";
/// Full public send endpoint (AUTH-11).
const RESEND_EMAILS_URL: &str = "https://api.resend.com/emails";
const USER_AGENT: &str = "oxidean-api/0.1";

/// Sends mail via Resend's REST API.
pub struct ResendSender {
    api_key: String,
    from: String,
    client: Client,
    base_url: String,
}

impl ResendSender {
    /// Create a Resend sender targeting the public API.
    pub fn new(api_key: impl Into<String>, from: impl Into<String>) -> Self {
        Self::with_base_url(api_key, from, DEFAULT_BASE_URL)
    }

    /// Create a sender with a custom API base URL (tests / mirrors).
    pub fn with_base_url(
        api_key: impl Into<String>,
        from: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            from: from.into(),
            client: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait::async_trait]
impl EmailSender for ResendSender {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError> {
        // Structured JSON fields — never raw header concatenation (T-04-05).
        let mut body = json!({
            "from": self.from,
            "to": [msg.to],
            "subject": msg.subject,
        });
        if let Some(html) = msg.html {
            body["html"] = json!(html);
        } else {
            body["text"] = json!(msg.text);
        }

        let url = if self.base_url == DEFAULT_BASE_URL {
            RESEND_EMAILS_URL.to_string()
        } else {
            format!("{}{}", self.base_url, EMAILS_PATH)
        };
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", USER_AGENT)
            .json(&body)
            .send()
            .await
            .map_err(|e| EmailError::Transport(sanitize_reqwest_error(&e)))?;

        if !response.status().is_success() {
            let status = response.status();
            // Avoid logging response bodies that might echo secrets; status only.
            return Err(EmailError::Provider(format!("resend HTTP {status}")));
        }
        Ok(())
    }
}

fn sanitize_reqwest_error(err: &reqwest::Error) -> String {
    // reqwest errors should not include the API key; still avoid dumping URL query secrets.
    let mut s = err.to_string();
    s = s.replace("Bearer ", "Bearer ***");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, header_regex, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn resend_includes_user_agent() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/emails"))
            .and(header("User-Agent", "oxidean-api/0.1"))
            .and(header_regex("Authorization", r"(?i)^Bearer\s+.+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "email_test"})))
            .mount(&server)
            .await;

        let sender = ResendSender::with_base_url(
            "re_test_key",
            "Oxidean <noreply@example.com>",
            server.uri(),
        );

        let result = sender
            .send(OutboundEmail {
                to: "user@example.com".into(),
                subject: "Welcome".into(),
                text: "Hello".into(),
                html: Some("<p>Hello</p>".into()),
            })
            .await;

        assert!(result.is_ok(), "send failed: {result:?}");
    }
}
