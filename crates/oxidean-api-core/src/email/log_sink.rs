//! Dev/log email sink — no network I/O (AUTH-09).

use crate::email::{EmailError, EmailSender, OutboundEmail};

/// Logs outbound mail via `tracing` target `oxidean.mail`.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogSink;

#[async_trait::async_trait]
impl EmailSender for LogSink {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError> {
        tracing::info!(
            target: "oxidean.mail",
            to = %msg.to,
            subject = %msg.subject,
            body = %msg.text,
            "outbound email (log sink)"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_logs_without_network() {
        let sink = LogSink;
        let result = sink
            .send(OutboundEmail {
                to: "user@example.com".into(),
                subject: "Welcome".into(),
                text: "Hello from Oxidean".into(),
                html: None,
            })
            .await;
        assert!(result.is_ok());
    }
}
