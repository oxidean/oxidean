//! Notification fan-out → outbound email (NOT-01).
//!
//! `fanout_inner` ends with [`maybe_spawn`]: a detached `tokio::spawn`
//! resolves the current sender (`db.get_auth_settings` →
//! `build_email_sender_for_settings`, env fallback), resolves the subject's
//! repo/actor context, maps each recipient to their primary verified
//! address, and sends one [`OutboundEmail`] per addressed recipient —
//! sequential, soft-fail per send. Fire-and-forget mirrors
//! `webhook::deliver::spawn_deliver` (D-HOOK-11); the durable queue that
//! digests (NOT-02) want can later sit behind this same seam.

use std::collections::HashMap;
use std::sync::Arc;

use oxidean_db::Database;

use crate::email::{self, EmailSender, OutboundEmail};

use super::NotifySubject;

/// `OXIDEAN_NOTIFY_EMAIL_ENABLED` master switch — same truthy parse as
/// `mcp_env_truthy` (unset/anything but 0|false|no|off means on).
pub fn env_notify_email_enabled() -> bool {
    match std::env::var("OXIDEAN_NOTIFY_EMAIL_ENABLED") {
        Ok(raw) => {
            let t = raw.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        }
        Err(_) => true,
    }
}

/// Sender resolved inside the detached task — always the current admin
/// configuration, identical from RPC, sweep, and scheduler contexts.
async fn resolve_sender(db: &Database) -> Arc<dyn EmailSender> {
    match db.get_auth_settings().await {
        Ok(settings) => email::build_email_sender_for_settings(&settings),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "notify mail: auth settings lookup failed; using env sender"
            );
            email::build_email_sender_from_env()
        }
    }
}

/// Repo owner slug + repo name for links — the same join the inbox uses
/// (`oxidean-api-notification::owner_slug_for_repo`). `None` when the repo
/// or its owner no longer resolves: mail is skipped, the row stays.
async fn subject_repo_context(db: &Database, repo_id: &str) -> Option<(String, String)> {
    let repo = match db.find_repository_by_id(repo_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, repo_id = %repo_id, "notify mail: repo lookup failed");
            return None;
        }
    };
    let owner = if repo.owner_type == "org" {
        match db.find_organization_by_id(&repo.owner_id).await {
            Ok(Some(o)) => o.slug,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(error = %e, repo_id = %repo_id, "notify mail: org lookup failed");
                return None;
            }
        }
    } else {
        match db.find_user_by_id(&repo.owner_id).await {
            Ok(Some(u)) => u.username,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(error = %e, repo_id = %repo_id, "notify mail: owner lookup failed");
                return None;
            }
        }
    };
    if owner.is_empty() {
        return None;
    }
    Some((owner, repo.name))
}

/// Actor display name for the body — username, falling back to a neutral
/// phrase when the actor no longer resolves.
async fn actor_display_name(db: &Database, actor_id: &str) -> String {
    match db.find_user_by_id(actor_id).await {
        Ok(Some(u)) if !u.username.is_empty() => u.username,
        Ok(_) | Err(_) => "someone".to_string(),
    }
}

/// Subject deep link, mirroring `subjectHref` in
/// `apps/web/src/lib/notification-queries.ts` (path only — callers prefix
/// the public origin).
pub fn subject_path(subject: &NotifySubject, owner: &str, repo: &str) -> String {
    fn path_segment(value: &str) -> String {
        let mut url = url::Url::parse("https://forge.invalid").expect("static URL");
        url.path_segments_mut()
            .expect("base URL supports path segments")
            .push(value);
        url.path().trim_start_matches('/').to_string()
    }

    match subject.kind {
        "pull_request" => format!("/{owner}/{repo}/pull/{}", subject.number),
        "release" => match &subject.subject_ref {
            Some(tag) => format!("/{owner}/{repo}/releases/{}", path_segment(tag)),
            None => format!("/{owner}/{repo}/releases"),
        },
        "workflow_run" => match &subject.subject_ref {
            Some(run) => format!("/{owner}/{repo}/actions/{}", path_segment(run)),
            None => format!("/{owner}/{repo}/actions"),
        },
        "push" => match &subject.subject_ref {
            Some(r) => {
                let encoded = r.split('/').map(path_segment).collect::<Vec<_>>().join("/");
                format!("/{owner}/{repo}/commits/{encoded}")
            }
            None => format!("/{owner}/{repo}"),
        },
        _ => format!("/{owner}/{repo}/issues/{}", subject.number),
    }
}

/// Display suffix, mirroring `subjectTail`: `#n` for numbered subjects,
/// the ref/tag for the rest, never for workflow runs.
pub fn subject_tail(subject: &NotifySubject) -> String {
    if subject.kind == "issue" || subject.kind == "pull_request" {
        return format!("#{}", subject.number);
    }
    if subject.kind == "workflow_run" {
        return String::new();
    }
    subject.subject_ref.clone().unwrap_or_default()
}

/// Body verb phrase, mirroring `reasonLabel` in notification-queries.ts.
pub fn reason_phrase(reason: &str) -> String {
    match reason {
        "issue_opened" => "opened an issue".into(),
        "issue_closed" => "closed an issue".into(),
        "issue_reopened" => "reopened an issue".into(),
        "issue_comment" => "commented on".into(),
        "issue_assigned" => "assigned you".into(),
        "issue_unassigned" => "unassigned you".into(),
        "issue_mention" => "mentioned you".into(),
        "pr_opened" => "opened a pull request".into(),
        "pr_closed" => "closed a pull request".into(),
        "pr_reopened" => "reopened a pull request".into(),
        "pr_merged" => "merged a pull request".into(),
        "pr_review" => "reviewed".into(),
        "pr_comment" => "commented on".into(),
        "pr_review_requested" => "requested your review".into(),
        "pr_mention" => "mentioned you".into(),
        "release_published" => "published a release on".into(),
        "release_edited" => "edited a release on".into(),
        "release_deleted" => "deleted a release on".into(),
        "workflow_run_success" => "workflow run succeeded on".into(),
        "workflow_run_failure" => "workflow run failed on".into(),
        "workflow_run_cancelled" => "workflow run cancelled on".into(),
        other => other.replace('_', " "),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn flatten_header_value(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolved context for one fan-out event — shared by every recipient's
/// message.
pub struct MailContext {
    pub origin: String,
    pub owner: String,
    pub repo: String,
    pub actor: String,
}

/// Build one recipient's message (pure — unit-testable).
///
/// Subject mirrors GitHub shape: `[owner/repo] Title (#42)` — the tail is
/// `#n` for numbered subjects, the release tag / ref otherwise, and
/// omitted when empty. Text body is the contract; `html` carries the same
/// content for clients that render it.
pub fn build_notification_email(
    to: &str,
    subject: &NotifySubject,
    reason: &str,
    ctx: &MailContext,
) -> OutboundEmail {
    let tail = subject_tail(subject);
    let title_line = if tail.is_empty() {
        format!("[{}/{}] {}", ctx.owner, ctx.repo, subject.title)
    } else {
        format!("[{}/{}] {} ({})", ctx.owner, ctx.repo, subject.title, tail)
    };
    let title_line = flatten_header_value(&title_line);
    let url = format!(
        "{}{}",
        ctx.origin,
        subject_path(subject, &ctx.owner, &ctx.repo)
    );
    let phrase = reason_phrase(reason);
    let headline = if tail.is_empty() {
        format!("{} {} {}/{}:", ctx.actor, phrase, ctx.owner, ctx.repo)
    } else {
        format!(
            "{} {} {}/{} {}:",
            ctx.actor, phrase, ctx.owner, ctx.repo, tail
        )
    };
    let settings_url = format!("{}/settings/notifications", ctx.origin);
    let text = format!(
        "{headline}\n\n{}\n\n{url}\n\n—\nYou are receiving this because you watch {}/{}.\nManage notifications: {settings_url}\n",
        subject.title, ctx.owner, ctx.repo
    );
    let html = format!(
        "<p>{} {} <a href=\"{}\">{}/{} {}</a>:</p>\n<p>{}</p>\n<p><a href=\"{}\">{}</a></p>\n<p style=\"color:#6a737d;font-size:small\">You are receiving this because you watch {}/{}.<br/>Manage notifications: <a href=\"{}\">{}</a></p>\n",
        escape_html(&ctx.actor),
        escape_html(&phrase),
        escape_html(&url),
        escape_html(&ctx.owner),
        escape_html(&ctx.repo),
        escape_html(&tail),
        escape_html(&subject.title),
        escape_html(&url),
        escape_html(&url),
        escape_html(&ctx.owner),
        escape_html(&ctx.repo),
        escape_html(&settings_url),
        escape_html(&settings_url),
    );
    OutboundEmail {
        to: to.to_string(),
        subject: title_line,
        text,
        html: Some(html),
    }
}

/// The delivery pass after spawn: resolve context + addresses once, then
/// build and send per addressed recipient in fan-out order. Per-recipient
/// failures log and continue — mail is best-effort like the inserts.
async fn deliver(
    db: Database,
    sender: Arc<dyn EmailSender>,
    actor_id: String,
    recipients: Vec<String>,
    reason: String,
    subject: NotifySubject,
) {
    let Some((owner, repo)) = subject_repo_context(&db, &subject.repo_id).await else {
        tracing::warn!(
            repo_id = %subject.repo_id,
            "notify mail: subject repo unresolvable; skipping mail for fan-out"
        );
        return;
    };
    let actor = actor_display_name(&db, &actor_id).await;
    let addresses: HashMap<String, String> =
        match db.list_primary_verified_emails_for_users(&recipients).await {
            Ok(pairs) => pairs.into_iter().collect(),
            Err(e) => {
                tracing::warn!(error = %e, "notify mail: address lookup failed; skipping fan-out");
                return;
            }
        };
    if addresses.is_empty() {
        return;
    }
    let ctx = MailContext {
        origin: crate::public_origin::resolve_public_origin(),
        owner,
        repo,
        actor,
    };
    for recipient_id in &recipients {
        let Some(addr) = addresses.get(recipient_id) else {
            continue;
        };
        let msg = build_notification_email(addr, &subject, &reason, &ctx);
        if let Err(e) = sender.send(msg).await {
            tracing::warn!(
                error = %e,
                recipient_id = %recipient_id,
                reason = %reason,
                "notification email send failed (soft-fail)"
            );
        }
    }
}

/// Fire-and-forget dispatch for one fan-out event — called by
/// `fanout_inner` with the set of recipients whose rows inserted.
/// No-op when the env switch is off or the set is empty.
pub fn maybe_spawn(
    db: &Database,
    actor_id: &str,
    recipients: &[String],
    reason: &str,
    subject: &NotifySubject,
) {
    if recipients.is_empty() || !env_notify_email_enabled() {
        return;
    }
    let db = db.clone();
    let actor_id = actor_id.to_string();
    let recipients = recipients.to_vec();
    let reason = reason.to_string();
    let subject = subject.clone();
    tokio::spawn(async move {
        let sender = resolve_sender(&db).await;
        deliver(db, sender, actor_id, recipients, reason, subject).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::email::EmailError;
    use oxidean_core::Role;
    use std::sync::Mutex;

    fn subject(
        kind: &'static str,
        number: i64,
        title: &str,
        subject_ref: Option<&str>,
    ) -> NotifySubject {
        NotifySubject {
            kind,
            repo_id: "r1".into(),
            number,
            title: title.into(),
            subject_ref: subject_ref.map(str::to_string),
        }
    }

    fn ctx() -> MailContext {
        MailContext {
            origin: "https://forge.test".into(),
            owner: "alice".into(),
            repo: "app".into(),
            actor: "bob".into(),
        }
    }

    #[test]
    fn subject_path_matches_web_shapes() {
        assert_eq!(
            subject_path(&subject("issue", 42, "t", None), "alice", "app"),
            "/alice/app/issues/42"
        );
        assert_eq!(
            subject_path(&subject("pull_request", 7, "t", None), "alice", "app"),
            "/alice/app/pull/7"
        );
        assert_eq!(
            subject_path(&subject("release", 0, "t", Some("v1.0")), "alice", "app"),
            "/alice/app/releases/v1.0"
        );
        assert_eq!(
            subject_path(&subject("release", 0, "t", Some("v1#rc1")), "alice", "app"),
            "/alice/app/releases/v1%23rc1"
        );
        assert_eq!(
            subject_path(&subject("release", 0, "t", None), "alice", "app"),
            "/alice/app/releases"
        );
        assert_eq!(
            subject_path(
                &subject("workflow_run", 0, "t", Some("run-9")),
                "alice",
                "app"
            ),
            "/alice/app/actions/run-9"
        );
        assert_eq!(
            subject_path(
                &subject("push", 0, "t", Some("refs/heads/main")),
                "alice",
                "app"
            ),
            "/alice/app/commits/refs/heads/main"
        );
        assert_eq!(
            subject_path(
                &subject("push", 0, "t", Some("refs/heads/feature#1")),
                "alice",
                "app"
            ),
            "/alice/app/commits/refs/heads/feature%231"
        );
    }

    #[test]
    fn subject_tail_matches_web_display() {
        assert_eq!(subject_tail(&subject("issue", 42, "t", None)), "#42");
        assert_eq!(subject_tail(&subject("pull_request", 7, "t", None)), "#7");
        assert_eq!(
            subject_tail(&subject("release", 0, "t", Some("v1.0"))),
            "v1.0"
        );
        // Run ids exist for deep links only — never displayed.
        assert_eq!(
            subject_tail(&subject("workflow_run", 0, "t", Some("run-9"))),
            ""
        );
    }

    #[test]
    fn reason_phrase_mirrors_reason_label() {
        assert_eq!(reason_phrase("issue_comment"), "commented on");
        assert_eq!(reason_phrase("pr_mention"), "mentioned you");
        assert_eq!(
            reason_phrase("pr_review_requested"),
            "requested your review"
        );
        assert_eq!(reason_phrase("release_published"), "published a release on");
        assert_eq!(
            reason_phrase("workflow_run_failure"),
            "workflow run failed on"
        );
        assert_eq!(reason_phrase("some_new_reason"), "some new reason");
    }

    #[test]
    fn env_switch_truth_table() {
        // Only this test touches the var — no other case reads it.
        std::env::set_var("OXIDEAN_NOTIFY_EMAIL_ENABLED", "0");
        assert!(!env_notify_email_enabled());
        std::env::set_var("OXIDEAN_NOTIFY_EMAIL_ENABLED", "false");
        assert!(!env_notify_email_enabled());
        std::env::set_var("OXIDEAN_NOTIFY_EMAIL_ENABLED", "off");
        assert!(!env_notify_email_enabled());
        std::env::set_var("OXIDEAN_NOTIFY_EMAIL_ENABLED", "1");
        assert!(env_notify_email_enabled());
        std::env::remove_var("OXIDEAN_NOTIFY_EMAIL_ENABLED");
        assert!(env_notify_email_enabled());
    }

    #[test]
    fn build_email_subject_body_and_escaping() {
        let s = subject("issue", 42, "Fix <b> & \"quotes\"", None);
        let msg = build_notification_email("u@ex.com", &s, "issue_comment", &ctx());
        assert_eq!(msg.to, "u@ex.com");
        assert_eq!(msg.subject, "[alice/app] Fix <b> & \"quotes\" (#42)");
        assert!(msg.text.contains("bob commented on alice/app #42:"));
        assert!(msg.text.contains("https://forge.test/alice/app/issues/42"));
        assert!(msg
            .text
            .contains("https://forge.test/settings/notifications"));
        let html = msg.html.expect("html body");
        assert!(html.contains("Fix &lt;b&gt; &amp; &quot;quotes&quot;"));
        assert!(html.contains("href=\"https://forge.test/alice/app/issues/42\""));
    }

    #[test]
    fn build_email_flattens_control_characters_from_subject_header() {
        let s = subject("issue", 42, "Fix issue\r\nBcc: attacker@example.com", None);
        let msg = build_notification_email("u@ex.com", &s, "issue_comment", &ctx());
        assert_eq!(
            msg.subject,
            "[alice/app] Fix issue Bcc: attacker@example.com (#42)"
        );
        assert!(!msg.subject.chars().any(char::is_control));
    }

    #[test]
    fn build_email_omits_tail_when_empty() {
        let s = subject("workflow_run", 0, "CI", Some("run-9"));
        let msg = build_notification_email("u@ex.com", &s, "workflow_run_failure", &ctx());
        assert_eq!(msg.subject, "[alice/app] CI");
        assert!(msg.text.contains("workflow run failed on alice/app:"));
        assert!(msg
            .text
            .contains("https://forge.test/alice/app/actions/run-9"));
    }

    #[derive(Default)]
    struct RecordingSender {
        sent: Mutex<Vec<OutboundEmail>>,
        fail_substr: String,
    }

    #[async_trait::async_trait]
    impl EmailSender for RecordingSender {
        async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError> {
            let fail = !self.fail_substr.is_empty() && msg.to.contains(&self.fail_substr);
            self.sent.lock().unwrap().push(msg);
            if fail {
                Err(EmailError::Transport("recorded failure".into()))
            } else {
                Ok(())
            }
        }
    }

    async fn seeded_db() -> (tempfile::TempDir, Database, String, String, String, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("mail.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        let now = "2026-01-01T00:00:00Z";
        let owner = db
            .create_user(
                "u-mail-owner",
                "owner@ex.com",
                "owner-user",
                Some("h"),
                "O",
                "",
                None,
                Role::User,
            )
            .await
            .expect("owner");
        let a = db
            .create_user(
                "u-mail-a",
                "a@ex.com",
                "a-user",
                Some("h"),
                "A",
                "",
                None,
                Role::User,
            )
            .await
            .expect("a");
        let b = db
            .create_user(
                "u-mail-b",
                "b@ex.com",
                "b-user",
                Some("h"),
                "B",
                "",
                None,
                Role::User,
            )
            .await
            .expect("b");
        let c = db
            .create_user(
                "u-mail-c",
                "c@ex.com",
                "c-user",
                Some("h"),
                "C",
                "",
                None,
                Role::User,
            )
            .await
            .expect("c");
        for uid in [&owner.id, &a.id, &c.id] {
            db.set_user_email_verified_at(&format!("{uid}-primary-email"), Some(now))
                .await
                .expect("verify");
        }
        // c also has a verified secondary — delivery must use primary only.
        db.create_user_email("u-mail-c-sec", &c.id, "c-sec@ex.com", false, Some(now))
            .await
            .expect("secondary");
        let repo = db
            .insert_repository("r-mail", &owner.id, "user", "app", "public", "", "main")
            .await
            .expect("repo");
        (dir, db, owner.id, a.id, b.id, repo.id)
    }

    #[tokio::test]
    async fn deliver_sends_to_primary_verified_recipients_only() {
        let (_dir, db, _owner, a, b, repo_id) = seeded_db().await;
        let c_id = "u-mail-c".to_string();
        let sender = Arc::new(RecordingSender::default());
        let mut s = subject("issue", 5, "T", None);
        s.repo_id = repo_id;
        deliver(
            db,
            sender.clone(),
            "u-mail-owner".to_string(),
            vec![a.clone(), b.clone(), c_id.clone(), "u-missing".to_string()],
            "issue_comment".to_string(),
            s,
        )
        .await;
        let sent = sender.sent.lock().unwrap();
        let tos: Vec<&str> = sent.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(tos, ["a@ex.com", "c@ex.com"], "verified primaries in order");
        assert!(sent[0].subject.contains("[owner-user/app]"));
        assert!(sent[0].text.contains("owner-user commented on"));
        drop(sent);
    }

    #[tokio::test]
    async fn deliver_continues_past_send_failure() {
        let (_dir, db, _owner, a, _b, repo_id) = seeded_db().await;
        let c_id = "u-mail-c".to_string();
        let sender = Arc::new(RecordingSender {
            sent: Mutex::new(Vec::new()),
            fail_substr: "a@ex.com".into(),
        });
        let mut s = subject("issue", 5, "T", None);
        s.repo_id = repo_id;
        deliver(
            db,
            sender.clone(),
            "u-mail-owner".to_string(),
            vec![a.clone(), c_id.clone()],
            "issue_comment".to_string(),
            s,
        )
        .await;
        let sent = sender.sent.lock().unwrap();
        assert_eq!(sent.len(), 2, "second recipient still attempted");
    }

    #[tokio::test]
    async fn deliver_skips_when_repo_unresolvable() {
        let (_dir, db, _owner, a, _b, _repo_id) = seeded_db().await;
        let sender = Arc::new(RecordingSender::default());
        let s = NotifySubject {
            kind: "issue",
            repo_id: "r-gone".into(),
            number: 1,
            title: "T".into(),
            subject_ref: None,
        };
        deliver(
            db,
            sender.clone(),
            "u-mail-owner".to_string(),
            vec![a.clone()],
            "issue_comment".to_string(),
            s,
        )
        .await;
        assert!(sender.sent.lock().unwrap().is_empty());
    }
}
