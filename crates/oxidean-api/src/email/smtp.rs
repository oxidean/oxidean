//! SMTP email adapter via lettre (AUTH-10).

use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};

use crate::email::{EmailError, EmailSender, OutboundEmail};

/// Sends mail through SMTP using `AsyncSmtpTransport` + lettre typed addresses.
pub struct SmtpSender {
    from: String,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpSender {
    /// Build a sender from an SMTP URL (`lettre` `from_url`) and From mailbox string.
    pub fn from_url(smtp_url: &str, from: impl Into<String>) -> Result<Self, EmailError> {
        let from = from.into();
        // Validate From early so misconfiguration fails at boot, not first send.
        parse_mailbox(&from)?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
            .map_err(|e| EmailError::Transport(redact_transport_error(&e.to_string())))?
            .build();
        Ok(Self { from, transport })
    }

    /// Build a lettre [`Message`] without sending (used by unit tests).
    pub fn build_message(&self, msg: &OutboundEmail) -> Result<Message, EmailError> {
        let from = parse_mailbox(&self.from)?;
        let to = parse_mailbox(&msg.to)?;
        let mut builder = Message::builder().from(from).to(to).subject(&msg.subject);

        if let Some(html) = &msg.html {
            builder = builder.header(ContentType::TEXT_HTML);
            builder
                .body(html.clone())
                .map_err(|e| EmailError::Transport(e.to_string()))
        } else {
            builder = builder.header(ContentType::TEXT_PLAIN);
            builder
                .body(msg.text.clone())
                .map_err(|e| EmailError::Transport(e.to_string()))
        }
    }
}

fn parse_mailbox(addr: &str) -> Result<Mailbox, EmailError> {
    addr.parse::<Mailbox>()
        .map_err(|_| EmailError::InvalidAddress(addr.to_string()))
}

/// Never echo SMTP URL credentials that may appear in lettre error strings.
fn redact_transport_error(raw: &str) -> String {
    // Strip common `user:pass@` credentials if present in the message.
    if let Some(at) = raw.find('@') {
        if let Some(scheme_end) = raw.find("://") {
            let after_scheme = scheme_end + 3;
            if after_scheme < at {
                let mut out = String::new();
                out.push_str(&raw[..after_scheme]);
                out.push_str("***:***");
                out.push_str(&raw[at..]);
                return out;
            }
        }
    }
    raw.to_string()
}

#[async_trait::async_trait]
impl EmailSender for SmtpSender {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError> {
        let email = self.build_message(&msg)?;
        self.transport
            .send(email)
            .await
            .map_err(|e| EmailError::Transport(redact_transport_error(&e.to_string())))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtp_builds_message() {
        // Valid construction without connecting: parse addresses via Mailbox.
        let from: Mailbox = "Oxidean <noreply@example.com>".parse().expect("from");
        let to: Mailbox = "user@example.com".parse().expect("to");
        let message = Message::builder()
            .from(from)
            .to(to)
            .subject("Welcome")
            .header(ContentType::TEXT_PLAIN)
            .body("Hello".to_string())
            .expect("message builds");
        let raw = message.formatted();
        let formatted = String::from_utf8_lossy(&raw);
        assert!(formatted.contains("Subject: Welcome"));

        // Invalid To rejects before send.
        let err = parse_mailbox("not-an-email").unwrap_err();
        assert!(matches!(err, EmailError::InvalidAddress(_)));
    }
}
