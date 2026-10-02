//! In-process russh Git SSH listener (D-SSH-01 / D-SSH-03 / D-SSH-04 / D-SSH-07).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxidean_db::Database;
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Server as RusshServer, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::auth;
use super::host_keys;
use super::pack::{self, AuthzDecision, PackCommand};
use super::rate_limit::SshAuthLimiter;

/// Shared state for all SSH connections.
#[derive(Clone)]
pub struct SshState {
    pub db: Database,
    pub repos_dir: PathBuf,
    pub auth_limiter: Arc<Mutex<SshAuthLimiter>>,
}

#[derive(Clone)]
struct SshServer {
    state: SshState,
}

struct SshHandler {
    state: SshState,
    peer: Option<SocketAddr>,
    user_id: Option<String>,
    key_id: Option<String>,
    session_channel: Option<Channel<Msg>>,
}

impl RusshServer for SshServer {
    type Handler = SshHandler;

    fn new_client(&mut self, peer: Option<SocketAddr>) -> Self::Handler {
        SshHandler {
            state: self.state.clone(),
            peer,
            user_id: None,
            key_id: None,
            session_channel: None,
        }
    }
}

impl Handler for SshHandler {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let ip = self
            .peer
            .map(|p| p.ip().to_string())
            .unwrap_or_else(|| "unknown".into());
        let fp = auth::fingerprint_of(public_key);

        {
            let mut lim = self.state.auth_limiter.lock().unwrap_or_else(|e| e.into_inner());
            if lim.check_ip(&ip).is_err() || lim.check_user(&fp).is_err() {
                return Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                });
            }
        }

        if user != "git" {
            tracing::debug!(%user, "SSH reject: username must be git");
            let mut lim = self.state.auth_limiter.lock().unwrap_or_else(|e| e.into_inner());
            lim.record_ip(&ip);
            lim.record_user(&fp);
            return Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            });
        }

        match auth::find_registered_key(&self.state.db, public_key).await {
            Ok(Some(row)) => {
                self.user_id = Some(row.user_id.clone());
                self.key_id = Some(row.id.clone());
                let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                let _ = self
                    .state
                    .db
                    .touch_ssh_key_last_used(&row.id, &now, Some(&ip))
                    .await;
                {
                    let mut lim = self.state.auth_limiter.lock().unwrap_or_else(|e| e.into_inner());
                    lim.clear_user(&fp);
                }
                Ok(Auth::Accept)
            }
            Ok(None) => {
                let mut lim = self.state.auth_limiter.lock().unwrap_or_else(|e| e.into_inner());
                lim.record_ip(&ip);
                lim.record_user(&fp);
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
            Err(e) => {
                tracing::error!(error = %e, "SSH fingerprint lookup failed");
                let mut lim = self.state.auth_limiter.lock().unwrap_or_else(|e| e.into_inner());
                lim.record_ip(&ip);
                lim.record_user(&fp);
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.session_channel = Some(channel);
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(user_id) = self.user_id.clone() else {
            session.channel_failure(channel)?;
            return Ok(());
        };

        let Some(cmd) = pack::parse_pack_exec(data) else {
            session.channel_failure(channel)?;
            return Ok(());
        };

        let decision = pack::authorize_pack(
            &self.state.db,
            &self.state.repos_dir,
            &user_id,
            &cmd,
        )
        .await;

        match decision {
            AuthzDecision::Deny { message } => {
                session.channel_success(channel)?;
                let handle = session.handle();
                tokio::spawn(async move {
                    pack::write_git_stderr_deny(&handle, channel, &message).await;
                });
                Ok(())
            }
            AuthzDecision::Allow {
                bare,
                repo_id,
                owner_slug,
                repo_name,
                is_push,
                capability,
            } => {
                let program = match &cmd {
                    PackCommand::UploadPack { .. } => "upload-pack",
                    PackCommand::ReceivePack { .. } => "receive-pack",
                };
                session.channel_success(channel)?;
                let Some(mut ch) = self.session_channel.take() else {
                    session.channel_failure(channel)?;
                    return Ok(());
                };
                let handle = session.handle();
                let db = self.state.db.clone();
                let repos_dir = self.state.repos_dir.clone();
                let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
                let user_id = user_id.clone();
                let actor_capability = pack::capability_env_label(capability);
                tokio::spawn(async move {
                    let git: std::sync::Arc<dyn oxidean_git::GitBackend> =
                        std::sync::Arc::new(oxidean_git::CliGitBackend::new());
                    // Capture refs before receive-pack so we can synthesize update
                    // triples for webhooks / PR sync / Actions (SSH has no pkt-line body).
                    let before_refs = if is_push {
                        git.list_refs(&bare).await.unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let writer = ch.make_writer();
                    let stderr_writer = ch.make_writer_ext(Some(1));
                    let reader = ch.make_reader();
                    // D-PKG-01: receive-pack gets helper/DB/repos/capability/ENV for hooks.
                    let protection_pairs = if is_push {
                        let db_url = std::env::var("OXIDEAN_DATABASE_URL")
                            .or_else(|_| std::env::var("DATABASE_URL"))
                            .unwrap_or_default();
                        let helper = crate::protection::resolve_protection_helper();
                        let oxidean_env = std::env::var("OXIDEAN_ENV").ok();
                        if db_url.is_empty() {
                            None
                        } else {
                            Some(pack::receive_pack_protection_env(
                                &db_url,
                                &repos_dir,
                                actor_capability,
                                helper.as_deref(),
                                oxidean_env.as_deref(),
                            ))
                        }
                    } else {
                        None
                    };
                    let code = pack::run_pack_command(
                        program,
                        &bare,
                        reader,
                        writer,
                        stderr_writer,
                        protection_pairs.as_deref(),
                    )
                    .await
                    .unwrap_or(1);
                    if code == 0 && is_push {
                        if let Ok(Some(user)) = db.find_user_by_id(&user_id).await {
                            // GIT-25: refresh cached size_bytes for UI/admin surfaces.
                            if let Err(e) = crate::git::quota::refresh_repo_size_bytes(
                                &db, &repo_id, &bare,
                            )
                            .await
                            {
                                tracing::warn!(error = %e, "refresh repo size_bytes failed");
                            }
                            let after_refs = git.list_refs(&bare).await.unwrap_or_default();
                            let updates = ref_updates_from_lists(&before_refs, &after_refs);
                            crate::repo::record_ref_updates(
                                &db,
                                &repo_id,
                                &user.id,
                                &updates,
                                Some(git.clone()),
                                Some(bare.as_path()),
                            )
                            .await;
                            crate::webhook::dispatch::notify_push(
                                &db,
                                &repo_id,
                                &owner_slug,
                                &repo_name,
                                &user.username,
                                &user.id,
                                &updates,
                                &env_name,
                            )
                            .await;
                            crate::pull::synchronize_after_push(
                                &db,
                                &repos_dir,
                                &repo_id,
                                &owner_slug,
                                &repo_name,
                                &user.username,
                                &user.id,
                                &updates,
                                &env_name,
                            )
                            .await;
                            let actions_enabled = crate::actions::env_actions_enabled();
                            let git_mirror = git.clone();
                            crate::actions::notify_push_actions(
                                &db,
                                git,
                                &repos_dir,
                                &repo_id,
                                &owner_slug,
                                &repo_name,
                                Some(&user.id),
                                &updates,
                                actions_enabled,
                            )
                            .await;
                            crate::mirror::notify_mirror_after_local_mutation(
                                db.clone(),
                                git_mirror,
                                repos_dir.clone(),
                                repo_id.clone(),
                            );
                        }
                    }
                    let _ = handle.exit_status_request(channel, code as u32).await;
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                });
                Ok(())
            }
        }
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_failure(channel)?;
        Ok(())
    }
}

/// Diff `list_refs` snapshots into `(before, after, refname)` triples for notify hooks.
fn ref_updates_from_lists(
    before: &[oxidean_git::GitRef],
    after: &[oxidean_git::GitRef],
) -> Vec<(String, String, String)> {
    use std::collections::HashMap;
    let before_map: HashMap<&str, &str> = before
        .iter()
        .map(|r| (r.name.as_str(), r.oid.as_str()))
        .collect();
    let mut out = Vec::new();
    for r in after {
        let prev = before_map.get(r.name.as_str()).copied().unwrap_or("0");
        if prev != r.oid {
            out.push((prev.to_string(), r.oid.clone(), r.name.clone()));
        }
    }
    // Deleted refs: present before, missing after.
    let after_names: std::collections::HashSet<&str> =
        after.iter().map(|r| r.name.as_str()).collect();
    for r in before {
        if !after_names.contains(r.name.as_str()) {
            out.push((r.oid.clone(), "0".repeat(40), r.name.clone()));
        }
    }
    out
}

/// True when `OXIDEAN_SSH_ENABLED` is `1`/`true`/`yes` (case-insensitive).
pub fn ssh_enabled_from_env() -> bool {
    std::env::var("OXIDEAN_SSH_ENABLED")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

pub fn ssh_port_from_env() -> u16 {
    std::env::var("OXIDEAN_SSH_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2222)
}

/// Bind and serve SSH until stopped. Returns bound address.
pub async fn spawn_listener(
    state: SshState,
    bind: SocketAddr,
) -> Result<(SocketAddr, oneshot::Sender<()>), String> {
    let host_dir = host_keys::host_key_dir();
    let host_key = host_keys::load_or_generate(&host_dir).await?;

    let config = russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![host_key],
        methods: MethodSet::from(&[MethodKind::PublicKey][..]),
        ..Default::default()
    };
    let config = Arc::new(config);
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| format!("SSH bind {bind}: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("SSH local_addr: {e}"))?;

    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut server = SshServer { state };
        let run = server.run_on_socket(config, &listener);
        let handle = run.handle();
        tokio::select! {
            _ = run => {}
            _ = stop_rx => {
                handle.shutdown("shutdown".into());
            }
        }
    });

    tokio::task::yield_now().await;
    tracing::info!(%local, "Git SSH listener ready");
    Ok((local, stop_tx))
}

/// Start SSH from env when enabled (production path).
pub async fn maybe_spawn_from_env(db: Database, repos_dir: PathBuf) -> Option<oneshot::Sender<()>> {
    if !ssh_enabled_from_env() {
        tracing::info!("OXIDEAN_SSH_ENABLED not set; SSH listener skipped");
        return None;
    }
    let port = ssh_port_from_env();
    let bind: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .expect("SSH bind parse");
    let state = SshState {
        db,
        repos_dir,
        auth_limiter: Arc::new(Mutex::new(SshAuthLimiter::new())),
    };
    match spawn_listener(state, bind).await {
        Ok((_addr, stop)) => Some(stop),
        Err(e) => {
            tracing::error!(error = %e, "failed to start SSH listener");
            None
        }
    }
}
