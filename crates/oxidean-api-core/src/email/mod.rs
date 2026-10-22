//! Outbound email adapters (log sink, SMTP, Resend).

mod log_sink;
mod resend;
mod smtp;

pub use log_sink::LogSink;
pub use resend::ResendSender;
pub use smtp::SmtpSender;

use std::sync::Arc;

use oxidean_db::AuthSettingsRow;
use thiserror::Error;

/// Outbound message payload shared by all adapters.
#[derive(Debug, Clone)]
pub struct OutboundEmail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
}

/// Errors from email construction or delivery.
#[derive(Debug, Error)]
pub enum EmailError {
    #[error("invalid email address: {0}")]
    InvalidAddress(String),
    #[error("email transport error: {0}")]
    Transport(String),
    #[error("email provider rejected request: {0}")]
    Provider(String),
}

#[async_trait::async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError>;
}

const DEFAULT_MAIL_FROM: &str = "Oxidean <noreply@localhost>";

fn mail_from(settings_from: Option<&str>) -> String {
    if let Some(f) = settings_from {
        if !f.trim().is_empty() {
            return f.to_string();
        }
    }
    std::env::var("OXIDEAN_MAIL_FROM").unwrap_or_else(|_| DEFAULT_MAIL_FROM.into())
}

/// Build sender from instance auth settings + ENV secrets (D-09 / AUTH-09–11).
///
/// Secrets stay in ENV; `email_provider` + `from_address` come from DB settings.
pub fn build_email_sender_for_settings(settings: &AuthSettingsRow) -> Arc<dyn EmailSender> {
    let from = mail_from(settings.from_address.as_deref());
    match settings.email_provider.as_str() {
        "resend" => {
            if let Ok(api_key) = std::env::var("OXIDEAN_RESEND_API_KEY") {
                if !api_key.is_empty() {
                    return Arc::new(resend_sender(api_key, from));
                }
            }
            tracing::warn!(
                target: "oxidean.mail",
                "email_provider=resend but OXIDEAN_RESEND_API_KEY unset; using log sink"
            );
            Arc::new(LogSink)
        }
        "smtp" => {
            if let Ok(smtp_url) = std::env::var("OXIDEAN_SMTP_URL") {
                if !smtp_url.is_empty() {
                    match SmtpSender::from_url(&smtp_url, from.clone()) {
                        Ok(sender) => return Arc::new(sender),
                        Err(e) => {
                            tracing::error!(
                                target: "oxidean.mail",
                                error = %e,
                                "failed to configure SMTP; falling back to log sink"
                            );
                        }
                    }
                }
            }
            tracing::warn!(
                target: "oxidean.mail",
                "email_provider=smtp but OXIDEAN_SMTP_URL unset; using log sink"
            );
            Arc::new(LogSink)
        }
        other => {
            let smtp_set = std::env::var("OXIDEAN_SMTP_URL")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let resend_set = std::env::var("OXIDEAN_RESEND_API_KEY")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if other == "log" && (smtp_set || resend_set) {
                tracing::warn!(
                    target: "oxidean.mail",
                    email_provider = %other,
                    smtp_configured = smtp_set,
                    resend_configured = resend_set,
                    "email_provider is log — outbound mail will not use SMTP/Resend ENV; set Admin → Auth to smtp/resend, or run make up-with-dev-auth (promotes log→smtp)"
                );
            }
            Arc::new(LogSink)
        }
    }
}

fn resend_sender(api_key: String, from: String) -> ResendSender {
    // Optional override for local stubs (`docs/dev-auth.md`); production leaves unset.
    match std::env::var("OXIDEAN_RESEND_BASE_URL") {
        Ok(base) if !base.trim().is_empty() => {
            ResendSender::with_base_url(api_key, from, base.trim())
        }
        _ => ResendSender::new(api_key, from),
    }
}

/// Build the configured email sender from environment (boot before settings load).
///
/// Selection order: Resend (`OXIDEAN_RESEND_API_KEY`) → SMTP (`OXIDEAN_SMTP_URL`) → LogSink.
/// From address: `OXIDEAN_MAIL_FROM` (default `Oxidean <noreply@localhost>`).
pub fn build_email_sender_from_env() -> Arc<dyn EmailSender> {
    let from = mail_from(None);

    if let Ok(api_key) = std::env::var("OXIDEAN_RESEND_API_KEY") {
        if !api_key.is_empty() {
            return Arc::new(resend_sender(api_key, from));
        }
    }

    if let Ok(smtp_url) = std::env::var("OXIDEAN_SMTP_URL") {
        if !smtp_url.is_empty() {
            match SmtpSender::from_url(&smtp_url, from.clone()) {
                Ok(sender) => return Arc::new(sender),
                Err(e) => {
                    tracing::error!(
                        target: "oxidean.mail",
                        error = %e,
                        "failed to configure SMTP; falling back to log sink"
                    );
                }
            }
        }
    }

    Arc::new(LogSink)
}
