//! Run-state machine — claim/heartbeat multiplexing, single-flight exec,
//! cancel-set semantics, route-aware classification, `terminate` funnel.
//!
//! Invariants (functional-spec / reliability-design):
//!   * `running` holds AT MOST one action; it is inserted BEFORE spawn.
//!   * `settled` inserts AFTER the report lands; exec-entry and
//!     child-completion both consult it to suppress duplicate reports.
//!   * Cancel ids close out exactly once whether claimed or not.
//!   * `terminate` is the sole exit funnel — every exit path runs the
//!     kill/report/custody sequence through it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;

use super::client::{ActionClaim, C3Error, ClaimAnswer, ConnectorClient, ReportStatus};
use super::custody::{self, SecretString, TargetRow};
use super::exec::{self, ActionOutcome};
use crate::commands::{CliError, Out};

/// Heartbeat cadence (BR4.1).
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
/// Default long-poll window (`--wait-seconds` default).
pub const WAIT_SECONDS_DEFAULT: u64 = 30;
/// Bounded report-retry ceiling for transport-class failures.
const REPORT_ATTEMPTS: u32 = 4;

// ---------------------------------------------------------------------------
// Error classification — ONE table, route-aware (dispatch totality is a
// tested invariant: all 10 wire codes + Transport + Unknown on all routes).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Pair,
    Claim,
    Report,
    Heartbeat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Kill in-flight, NO report (verifier untrusted), delete creds+row,
    /// exit 1. Carries the wire code for the message.
    AuthDeath(&'static str),
    /// Kill, report `cancelled:revoked`, delete creds+row, exit 0.
    TerminalRevoked,
    /// Claim/heartbeat `not_found`: kill, report-attempt (drops 404),
    /// delete creds+row, exit 0.
    TerminalNotFound,
    /// Kill, report `cancelled:bad_input`, KEEP creds, exit 1.
    BadInputExit,
    /// Report-route `not_found` — the row is dead; drop, continue.
    DropAction,
    /// Transport-class: exponential backoff, loop survives.
    Backoff,
    /// `rate_limited` — sleep the hint verbatim, not a transport failure.
    Sleep(u64),
    /// Pair-route terminal refusal — exit 1 with a human message.
    PairReject(&'static str),
    /// `rate_limited` on pair — sleep the hint, then ONE retry.
    PairRetryOnce(u64),
    /// Pair transport/internal/loop-only-code — exit 1.
    PairFail(String),
}

/// The single dispatch table. Pair-only codes on loop routes collapse to
/// Backoff (internal-class); loop-only codes on the pair route collapse to
/// PairFail — never a "clean" exit.
pub fn classify(e: &C3Error, route: Route) -> Disposition {
    match route {
        Route::Pair => match e {
            C3Error::PairingExpired => Disposition::PairReject(
                "pairing code expired — mint a fresh one in the Oxidean UI and retry",
            ),
            C3Error::PairingUsed => Disposition::PairReject(
                "pairing code already used — mint a fresh one in the Oxidean UI",
            ),
            C3Error::Forbidden => Disposition::PairReject(
                "requested capabilities exceed the pairing grant — narrow --caps and retry",
            ),
            C3Error::Disabled => {
                Disposition::PairReject("oxpilot is disabled on this forge — ask the operator")
            }
            C3Error::RateLimited { retry_after } => Disposition::PairRetryOnce(*retry_after),
            C3Error::Transport(m) => Disposition::PairFail(format!(
                "pair request failed: {m} — check connectivity and retry"
            )),
            // unauthorized/revoked/not_found/bad_input/internal/unknown on
            // pair → internal-class exit 1 (never confused with a refusal).
            other => Disposition::PairFail(format!("pairing failed ({other}) — retry")),
        },
        Route::Report => match e {
            C3Error::NotFound => Disposition::DropAction,
            other => classify_loop(other),
        },
        Route::Claim | Route::Heartbeat => classify_loop(e),
    }
}

fn classify_loop(e: &C3Error) -> Disposition {
    match e {
        C3Error::Unauthorized => Disposition::AuthDeath("unauthorized"),
        C3Error::Forbidden => Disposition::AuthDeath("forbidden"),
        C3Error::Revoked => Disposition::TerminalRevoked,
        C3Error::NotFound => Disposition::TerminalNotFound,
        C3Error::BadInput => Disposition::BadInputExit,
        C3Error::RateLimited { retry_after } => Disposition::Sleep(*retry_after),
        // Loop-survivable: transport, disabled, internal, unknown, and
        // pair-only codes leaking into the loop.
        C3Error::Disabled
        | C3Error::Internal
        | C3Error::Transport(_)
        | C3Error::Unknown(_)
        | C3Error::PairingExpired
        | C3Error::PairingUsed => Disposition::Backoff,
    }
}

// ---------------------------------------------------------------------------
// Terminal reasons — everything the funnel handles.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TermReason {
    /// SIGINT / SIGTERM — kill, report `cancelled:<sig>`, KEEP creds, exit 0.
    Signal(&'static str),
    /// Revoked (claim/report `revoked` or heartbeat `revoked:true`) — kill,
    /// report `cancelled:revoked`, delete creds+row, exit 0.
    Revoked,
    /// Claim/heartbeat `not_found` — same shape as Revoked, marker
    /// `cancelled:revoked` (the design's terminal close-out vocabulary).
    NotFound,
    /// `unauthorized`/`forbidden` on loop routes — kill, NO report
    /// (verifier untrusted), delete creds+row, exit 1.
    AuthDeath(&'static str),
    /// `bad_input` on loop routes — kill, report `cancelled:bad_input`,
    /// KEEP creds, exit 1.
    BadInput,
}

impl TermReason {
    /// Report marker sent for each drained in-flight action; `None` for
    /// AuthDeath — a rejected verifier must not close out rows.
    fn report_marker(&self) -> Option<String> {
        match self {
            Self::Signal(s) => Some(format!("cancelled:{s}")),
            Self::Revoked => Some("cancelled:revoked".to_string()),
            Self::NotFound => Some("cancelled:revoked".to_string()),
            Self::BadInput => Some("cancelled:bad_input".to_string()),
            Self::AuthDeath(_) => None,
        }
    }

    fn delete_creds(&self) -> bool {
        matches!(self, Self::Revoked | Self::NotFound | Self::AuthDeath(_))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Signal(s) => s,
            Self::Revoked => "revoked",
            Self::NotFound => "not_found",
            Self::AuthDeath(c) => c,
            Self::BadInput => "bad_input",
        }
    }
}

/// Backoff math, pure: `min(60, 2^(streak-1) × (0.8 + frac×0.4))` —
/// jitter BEFORE the clamp (NFR3.x). `frac` in `[0,1)`.
fn backoff_secs(fail_streak: u32, frac: f64) -> f64 {
    let exp = fail_streak.saturating_sub(1).min(9);
    let base = (1u64 << exp) as f64;
    (base * (0.8 + frac * 0.4)).min(60.0).max(0.05)
}

/// Exit-pinned outcome — Ok(out) for exit 0, Err(cli) for exit 1.
pub struct LoopExit(pub Result<Out, CliError>);

impl std::fmt::Debug for LoopExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Ok(_) => f.write_str("LoopExit(clean)"),
            Err(e) => f.write_str(&format!("LoopExit(fail: {e:?})")),
        }
    }
}

type LResult<T> = Result<T, LoopExit>;

// ---------------------------------------------------------------------------
// Signals — SIGINT + SIGTERM as persistent unix streams; SIGINT is a
// once-pinned ctrl_c future on non-unix.
// ---------------------------------------------------------------------------

#[cfg(unix)]
type SigStream = tokio::signal::unix::Signal;
#[cfg(not(unix))]
type SigStream = ();

/// A persistent SIGINT receiver — held across every `select!` so a
/// Ctrl-C landing mid-handler (between iterations) is never broadcast
/// to zero subscribers and silently eaten. `ctrl_c()`-per-iteration
/// loses that race.
#[cfg(unix)]
type SigInt = tokio::signal::unix::Signal;
#[cfg(not(unix))]
type SigInt = std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>;

#[cfg(unix)]
async fn next_signal(i: &mut SigInt, s: &mut SigStream) -> TermReason {
    tokio::select! {
        _ = i.recv() => TermReason::Signal("sigint"),
        _ = s.recv() => TermReason::Signal("sigterm"),
    }
}

#[cfg(not(unix))]
async fn next_signal(i: &mut SigInt, _: &mut SigStream) -> TermReason {
    let _ = i.as_mut().await;
    TermReason::Signal("sigint")
}

// ---------------------------------------------------------------------------
// RunState.
// ---------------------------------------------------------------------------

/// What `running` holds — `Armed` is the insert-before-spawn sentinel
/// (spawn itself can't yield, so a cancel can only ever observe `Live`;
/// `Armed` exists so the map write provably precedes the spawn call).
enum Running {
    Armed,
    Live { pid: u32 },
}

pub struct RunOpts {
    pub client: ConnectorClient,
    pub verifier: SecretString,
    pub row: TargetRow,
    pub cfg_dir: PathBuf,
    pub canonical_root: PathBuf,
    pub canonical_cfg: PathBuf,
    pub secrets: Vec<String>,
    pub wait_seconds: u64,
    pub exec_timeout_secs: u64,
    pub verbose: bool,
}

pub struct RunState {
    client: ConnectorClient,
    verifier: SecretString,
    row: TargetRow,
    cfg_dir: PathBuf,
    canonical_root: PathBuf,
    canonical_cfg: PathBuf,
    running: HashMap<String, Running>,
    settled: HashSet<String>,
    fail_streak: u32,
    rng: u64,
    secrets: Vec<String>,
    wait_seconds: u64,
    exec_timeout: Duration,
    hb: tokio::time::Interval,
    next_poll: Instant,
    sigint: SigInt,
    sigterm: SigStream,
    verbose: bool,
}

impl RunState {
    pub fn new(opts: RunOpts) -> Result<Self, CliError> {
        #[cfg(unix)]
        let sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|e| CliError::Transport(format!("signal setup failed: {e}")))?;
        #[cfg(not(unix))]
        let sigterm = ();
        #[cfg(unix)]
        let sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map_err(|e| CliError::Transport(format!("signal setup failed: {e}")))?;
        #[cfg(not(unix))]
        let sigint: SigInt = Box::pin(tokio::signal::ctrl_c());
        let mut hb = tokio::time::interval(HEARTBEAT_INTERVAL);
        // Delay, not Burst: ticks missed inside long handler awaits must
        // not storm the server on resume.
        hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() ^ ((d.subsec_nanos() as u64) << 21))
            .unwrap_or(0x9e37_79b9_7f4a_7c15)
            | 1;
        Ok(Self {
            client: opts.client,
            verifier: opts.verifier,
            row: opts.row,
            cfg_dir: opts.cfg_dir,
            canonical_root: opts.canonical_root,
            canonical_cfg: opts.canonical_cfg,
            running: HashMap::new(),
            settled: HashSet::new(),
            fail_streak: 0,
            rng: seed,
            secrets: opts.secrets,
            wait_seconds: opts.wait_seconds,
            exec_timeout: Duration::from_secs(opts.exec_timeout_secs),
            hb,
            next_poll: Instant::now(),
            sigint,
            sigterm,
            verbose: opts.verbose,
        })
    }

    fn log(&self, msg: &str) {
        if self.verbose {
            eprintln!("ox pilot: {msg}");
        }
    }

    /// `min(60, 2^n × (1±0.2))` — jitter BEFORE clamp; streak resets on
    /// any successful call. `frac ∈ [0,1)` is the jitter source.
    fn backoff(&mut self) -> Duration {
        self.fail_streak = self.fail_streak.saturating_add(1);
        let frac = {
            let mut x = self.rng;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.rng = x;
            (x >> 11) as f64 / (1u64 << 53) as f64
        };
        Duration::from_secs_f64(backoff_secs(self.fail_streak, frac))
    }

    fn claim_future(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ClaimAnswer, C3Error>> + Send>>
    {
        let client = self.client.clone();
        let verifier = self.verifier.clone();
        let target_id = self.row.target_id.clone();
        let wait = self.wait_seconds;
        let caps = self.row.granted_caps.clone();
        Box::pin(async move { client.claim(&verifier, &target_id, wait, &caps).await })
    }

    /// The outer select: heartbeat | claim-wait | signal. Arms evaluate
    /// through destructured field borrows so `self` stays usable in bodies.
    pub async fn run_loop(mut self) -> Result<Out, CliError> {
        let mut claim_fut: Option<
            std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<ClaimAnswer, C3Error>> + Send>,
            >,
        > = None;
        enum Ev {
            Heartbeat,
            ArmClaim,
            Claim(Result<ClaimAnswer, C3Error>),
            Signal(TermReason),
        }
        loop {
            let ev = {
                let Self {
                    hb,
                    next_poll,
                    sigint,
                    sigterm,
                    ..
                } = &mut self;
                tokio::select! {
                    _ = hb.tick() => Ev::Heartbeat,
                    _ = tokio::time::sleep_until(*next_poll), if claim_fut.is_none() => Ev::ArmClaim,
                    r = async { claim_fut.as_mut().expect("armed").as_mut().await }, if claim_fut.is_some() => Ev::Claim(r),
                    s = next_signal(sigint, sigterm) => Ev::Signal(s),
                }
            };
            let res: LResult<()> = match ev {
                Ev::Heartbeat => self.on_heartbeat().await,
                Ev::ArmClaim => {
                    claim_fut = Some(self.claim_future());
                    Ok(())
                }
                Ev::Claim(r) => {
                    // The completed future must drop before the next arm —
                    // re-polling a finished Future inside select! panics.
                    claim_fut = None;
                    self.on_claim(r).await
                }
                Ev::Signal(t) => Err(self.terminate(t).await),
            };
            match res {
                Ok(()) => {}
                Err(LoopExit(out)) => return out,
            }
        }
    }

    async fn on_heartbeat(&mut self) -> LResult<()> {
        match self
            .client
            .heartbeat(&self.verifier, &self.row.target_id)
            .await
        {
            Ok(hb) => {
                self.fail_streak = 0;
                self.apply_cancels(hb.cancel).await?;
                if hb.revoked {
                    return Err(self.terminate(TermReason::Revoked).await);
                }
                Ok(())
            }
            Err(e) => self.loop_error(&e, Route::Heartbeat).await,
        }
    }

    async fn on_claim(&mut self, r: Result<ClaimAnswer, C3Error>) -> LResult<()> {
        match r {
            Err(e) => self.loop_error(&e, Route::Claim).await,
            Ok(ClaimAnswer::Timeout { retry_after }) => {
                self.fail_streak = 0;
                self.next_poll = Instant::now() + Duration::from_secs(retry_after.max(1).min(3600));
                Ok(())
            }
            Ok(ClaimAnswer::Batch {
                actions,
                cancel,
                retry_after,
            }) => {
                self.fail_streak = 0;
                // Cancel set applies BEFORE the next action starts (BR5.1).
                self.apply_cancels(cancel).await?;
                for a in &actions {
                    self.exec_action(a).await?;
                }
                // 1s re-poll floor: 204s, absent hints, and cancel-only
                // batches all bottom out at 1s (BR5.4/BR8.1). The 1h
                // ceiling keeps hostile hints inside Instant's range.
                self.next_poll = Instant::now() + Duration::from_secs(retry_after.max(1).min(3600));
                Ok(())
            }
        }
    }

    /// Shared claim/heartbeat error arm — every route's non-Ok answer
    /// funnels through the single dispatch table.
    async fn loop_error(&mut self, e: &C3Error, route: Route) -> LResult<()> {
        match classify(e, route) {
            Disposition::AuthDeath(code) => Err(self.terminate(TermReason::AuthDeath(code)).await),
            Disposition::TerminalRevoked => Err(self.terminate(TermReason::Revoked).await),
            Disposition::TerminalNotFound => Err(self.terminate(TermReason::NotFound).await),
            Disposition::BadInputExit => Err(self.terminate(TermReason::BadInput).await),
            Disposition::Backoff => {
                let d = self.backoff();
                self.log(&format!("{route:?} failed ({e}) — retry in {d:.0?}"));
                self.next_poll = Instant::now() + d;
                Ok(())
            }
            Disposition::Sleep(secs) => {
                self.fail_streak = 0;
                self.next_poll = Instant::now() + Duration::from_secs(secs.max(1).min(3600));
                Ok(())
            }
            // Loop routes never produce pair/report-only dispositions.
            Disposition::DropAction
            | Disposition::PairReject(_)
            | Disposition::PairRetryOnce(_)
            | Disposition::PairFail(_) => Ok(()),
        }
    }

    /// Idempotent report — transport-class retries bounded (then the
    /// server re-announce or crash-recovery posture takes over); terminal
    /// dispositions propagate through `terminate`.
    async fn post_report(
        &mut self,
        action_id: &str,
        status: ReportStatus,
        exit_code: Option<i64>,
        output: String,
    ) -> LResult<bool> {
        let output = exec::enforce_output_bound(output);
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self
                .client
                .report(&self.verifier, action_id, status, exit_code, &output)
                .await
            {
                Ok(()) => {
                    self.fail_streak = 0;
                    return Ok(true);
                }
                Err(e) => match classify(&e, Route::Report) {
                    Disposition::DropAction => return Ok(true),
                    Disposition::AuthDeath(code) => {
                        return Err(self.terminate(TermReason::AuthDeath(code)).await);
                    }
                    Disposition::TerminalRevoked => {
                        return Err(self.terminate(TermReason::Revoked).await);
                    }
                    Disposition::TerminalNotFound => {
                        return Err(self.terminate(TermReason::NotFound).await);
                    }
                    Disposition::BadInputExit => {
                        return Err(self.terminate(TermReason::BadInput).await);
                    }
                    Disposition::Sleep(secs) => {
                        if attempt >= REPORT_ATTEMPTS {
                            return Ok(false);
                        }
                        tokio::time::sleep(Duration::from_secs(secs.max(1).min(3600))).await;
                    }
                    Disposition::Backoff => {
                        if attempt >= REPORT_ATTEMPTS {
                            return Ok(false);
                        }
                        let d = self.backoff();
                        self.log(&format!(
                            "report {action_id} failed ({e}) — retry in {d:.0?}"
                        ));
                        tokio::time::sleep(d).await;
                    }
                    Disposition::PairReject(_)
                    | Disposition::PairRetryOnce(_)
                    | Disposition::PairFail(_) => return Ok(false),
                },
            }
        }
    }

    /// Cancel-set semantics (BR5.1/BR5.2): running → kill + `mid-flight`;
    /// unclaimed/unknown → `pre-start`. Settled ids are skipped (exactly-once);
    /// `settled` inserts only after the report lands — a transport-failed
    /// close-out retries on the next server re-announce.
    async fn apply_cancels(&mut self, ids: Vec<String>) -> LResult<()> {
        for id in ids {
            if self.settled.contains(&id) {
                continue;
            }
            let marker = match self.running.remove(&id) {
                Some(Running::Live { pid }) => {
                    exec::kill_process_tree(pid);
                    "cancelled:mid-flight"
                }
                Some(Running::Armed) => "cancelled:mid-flight",
                None => "cancelled:pre-start",
            };
            if self
                .post_report(&id, ReportStatus::Failed, None, marker.to_string())
                .await?
            {
                self.settle(id);
            }
        }
        Ok(())
    }

    /// Insert into `settled` under a hard cap — a long-lived connector
    /// must not grow the dedupe set by lifetime action throughput. The
    /// window (4096) far exceeds the server's ≤50 re-announcement batch,
    /// so exactly-once suppression is preserved for every live id.
    fn settle(&mut self, id: String) {
        const SETTLED_CAP: usize = 4096;
        if self.settled.len() >= SETTLED_CAP {
            if let Some(ev) = self.settled.iter().next().cloned() {
                self.settled.remove(&ev);
            }
        }
        self.settled.insert(id);
    }

    /// Report a failed outcome and settle on success.
    async fn report_failed(&mut self, id: &str, output: String) -> LResult<()> {
        if self
            .post_report(id, ReportStatus::Failed, None, output)
            .await?
        {
            self.settle(id.to_string());
        }
        Ok(())
    }

    async fn report_outcome(&mut self, id: &str, o: ActionOutcome) -> LResult<()> {
        if self
            .post_report(id, o.status, o.exit_code, o.output)
            .await?
        {
            self.settle(id.to_string());
        }
        Ok(())
    }

    /// One claimed action: settled-suppress → cap check → kind dispatch.
    async fn exec_action(&mut self, a: &ActionClaim) -> LResult<()> {
        let id = a.action_id.clone();
        if self.settled.contains(&id) {
            return Ok(());
        }
        if !self.row.granted_caps.iter().any(|c| c == &a.kind) {
            return self
                .report_failed(&id, format!("capability_refused:{}", a.kind))
                .await;
        }
        match a.kind.as_str() {
            "exec_shell" => self.exec_shell(a).await,
            "fs_read" => self.exec_fs(a, false).await,
            "fs_write" => self.exec_fs(a, true).await,
            // `granted_caps` ⊆ the closed set — unreachable; still refuse.
            other => {
                self.report_failed(&id, format!("capability_refused:{other}"))
                    .await
            }
        }
    }

    /// fs_* run inside spawn_blocking — not killable mid-op (BR5.2's
    /// granularity is BETWEEN ops). `running` is never populated for an
    /// fs op, so a cancel announced after the op (whose report may not
    /// have landed yet) is closed out `cancelled:pre-start` — the marker
    /// understates "ran to completion" by design.
    async fn exec_fs(&mut self, a: &ActionClaim, write: bool) -> LResult<()> {
        let id = a.action_id.clone();
        let root = self.canonical_root.clone();
        let cfg = self.canonical_cfg.clone();
        let secrets = self.secrets.clone();
        let outcome = if write {
            let args: exec::FsWriteArgs = match exec::parse_args(&a.args) {
                Ok(v) => v,
                Err(reason) => return self.report_failed(&id, reason).await,
            };
            tokio::task::spawn_blocking(move || {
                exec::fs_write(&root, &cfg, &args.path, &args.content_b64, &secrets)
            })
            .await
        } else {
            let args: exec::FsReadArgs = match exec::parse_args(&a.args) {
                Ok(v) => v,
                Err(reason) => return self.report_failed(&id, reason).await,
            };
            tokio::task::spawn_blocking(move || exec::fs_read(&root, &cfg, &args.path, &secrets))
                .await
        };
        match outcome {
            Ok(o) => self.report_outcome(&id, o).await,
            Err(e) => self.report_failed(&id, format!("io:task {e}")).await,
        }
    }

    /// exec_shell — nested select: child completion | exec timeout |
    /// heartbeat tick | signal. Heartbeat cancellation reaches the child
    /// via `apply_cancels` → `kill_process_tree` → the wait arm resolves
    /// and `settled` suppresses the second report.
    async fn exec_shell(&mut self, a: &ActionClaim) -> LResult<()> {
        let id = a.action_id.clone();
        let args: exec::ExecShellArgs = match exec::parse_args(&a.args) {
            Ok(v) => v,
            Err(reason) => return self.report_failed(&id, reason).await,
        };
        // Insert BEFORE spawn — the cancel path must always see the entry.
        self.running.insert(id.clone(), Running::Armed);
        let mut child = match exec::spawn_shell(&args.command, &self.canonical_root) {
            Ok(c) => c,
            Err(e) => {
                self.running.remove(&id);
                return self.report_failed(&id, format!("spawn_error:{e}")).await;
            }
        };
        self.running
            .insert(id.clone(), Running::Live { pid: child.pid() });
        self.log(&format!("exec_shell {id} pid {}", child.pid()));

        let deadline = tokio::time::sleep(self.exec_timeout);
        tokio::pin!(deadline);
        enum ExecEv {
            Child(Result<(Option<i32>, String), tokio::task::JoinError>),
            Timeout,
            Heartbeat,
            Signal(TermReason),
        }
        enum End {
            Done(Option<i32>, String),
            Timeout,
        }
        let end: End = loop {
            let ev = {
                let Self {
                    hb,
                    sigint,
                    sigterm,
                    ..
                } = &mut self;
                tokio::select! {
                    res = child.wait_mut() => ExecEv::Child(res),
                    _ = &mut deadline => ExecEv::Timeout,
                    _ = hb.tick() => ExecEv::Heartbeat,
                    s = next_signal(sigint, sigterm) => ExecEv::Signal(s),
                }
            };
            match ev {
                ExecEv::Child(Ok((code, tail))) => break End::Done(code, tail),
                ExecEv::Child(Err(e)) => break End::Done(None, format!("join:{e}")),
                ExecEv::Timeout => break End::Timeout,
                ExecEv::Heartbeat => self.on_heartbeat().await?,
                ExecEv::Signal(t) => return Err(self.terminate(t).await),
            }
        };
        match end {
            End::Timeout => {
                if let Some(Running::Live { pid }) = self.running.get(&id) {
                    exec::kill_process_tree(*pid);
                }
                self.running.remove(&id);
                // Same suppression as End::Done — a heartbeat cancel that
                // already reported must not double-report on timeout.
                if self.settled.contains(&id) {
                    return Ok(());
                }
                let tail = exec::scrub(&child.tail_snapshot(), &self.secrets);
                let marker = format!("timeout:{}s", self.exec_timeout.as_secs());
                let output = if tail.is_empty() {
                    marker
                } else {
                    format!("{marker}\n{tail}")
                };
                self.report_failed(&id, output).await
            }
            End::Done(code, tail) => {
                self.running.remove(&id);
                // Suppress the second report when a mid-flight cancel
                // already closed the row out (settled was inserted there).
                if self.settled.contains(&id) {
                    return Ok(());
                }
                let scrubbed = exec::scrub(&tail, &self.secrets);
                self.report_outcome(
                    &id,
                    ActionOutcome {
                        status: if code == Some(0) {
                            ReportStatus::Done
                        } else {
                            ReportStatus::Failed
                        },
                        exit_code: code.map(i64::from),
                        output: scrubbed,
                    },
                )
                .await
            }
        }
    }

    /// The sole exit funnel: kill in-flight → report per cred-liveness →
    /// custody delete per the matrix → the pinned exit code.
    async fn terminate(&mut self, reason: TermReason) -> LoopExit {
        let drained: Vec<(String, Running)> = self.running.drain().collect();
        for (_, r) in &drained {
            if let Running::Live { pid } = r {
                exec::kill_process_tree(*pid);
            }
        }
        if let Some(marker) = reason.report_marker() {
            for (id, _) in &drained {
                let _ = self
                    .client
                    .report(&self.verifier, id, ReportStatus::Failed, None, &marker)
                    .await;
            }
        }
        if reason.delete_creds() {
            custody::delete_verifier(&self.cfg_dir, &self.row.instance, &self.row.connector_id);
            if let Err(e) = custody::delete_target(&self.cfg_dir, &self.row.target_id) {
                eprintln!("ox pilot: warning — could not remove target row: {e}");
            }
        }
        let label = &self.row.device_label;
        let target = &self.row.target_id;
        match reason {
            TermReason::Signal(s) => LoopExit(Ok(Out::Report {
                text: format!("received {s} — killed in-flight work; credentials kept"),
                json: json!({"terminated": s, "target_id": target}),
            })),
            TermReason::Revoked => LoopExit(Ok(Out::Report {
                text: format!("target {target} ({label}) was revoked — credentials removed"),
                json: json!({"terminated": "revoked", "target_id": target}),
            })),
            TermReason::NotFound => LoopExit(Ok(Out::Report {
                text: format!("target {target} ({label}) no longer exists — credentials removed"),
                json: json!({"terminated": "not_found", "target_id": target}),
            })),
            TermReason::AuthDeath(code) => LoopExit(Err(CliError::Transport(format!(
                "{code} — verifier rejected; local credentials deleted; re-pair to reconnect"
            )))),
            TermReason::BadInput => LoopExit(Err(CliError::Transport(
                "server rejected a connector request (bad_input) — credentials kept; \
                 upgrade ox or report this"
                    .to_string(),
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pilot::client::{ActionClaim, ConnectorClient};

    fn test_state(server_uri: &str) -> RunState {
        let jail = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        RunState::new(RunOpts {
            client: ConnectorClient::new(server_uri, false),
            verifier: SecretString::new("verifier-under-test-123".to_string()),
            row: TargetRow {
                instance: "https://forge.example".to_string(),
                connector_id: "conn-1".to_string(),
                target_id: "t-1".to_string(),
                device_label: "test".to_string(),
                granted_caps: vec!["exec_shell".into(), "fs_read".into(), "fs_write".into()],
                requested_caps: vec!["exec_shell".into()],
                paired_at: "2026-01-01T00:00:00Z".to_string(),
            },
            cfg_dir: cfg.path().to_path_buf(),
            canonical_root: std::fs::canonicalize(jail.path()).unwrap(),
            canonical_cfg: std::fs::canonicalize(cfg.path()).unwrap(),
            secrets: vec![],
            wait_seconds: 1,
            exec_timeout_secs: 30,
            verbose: false,
        })
        .unwrap()
    }

    /// The FULL dispatch table: every C3Error variant on every route —
    /// 10 wire codes + Transport + Unknown × 4 routes.
    #[test]
    fn dispatch_table_totality() {
        use Disposition as D;
        let loop_cases: &[(C3Error, D)] = &[
            (C3Error::Unauthorized, D::AuthDeath("unauthorized")),
            (C3Error::Forbidden, D::AuthDeath("forbidden")),
            (C3Error::Revoked, D::TerminalRevoked),
            (C3Error::NotFound, D::TerminalNotFound),
            (C3Error::BadInput, D::BadInputExit),
            (C3Error::RateLimited { retry_after: 3 }, D::Sleep(3)),
            (C3Error::Disabled, D::Backoff),
            (C3Error::Internal, D::Backoff),
            (C3Error::Transport("x".into()), D::Backoff),
            (C3Error::Unknown("u".into()), D::Backoff),
            (C3Error::PairingExpired, D::Backoff),
            (C3Error::PairingUsed, D::Backoff),
        ];
        for route in [Route::Claim, Route::Heartbeat] {
            for (e, want) in loop_cases {
                assert_eq!(&classify(e, route), want, "{route:?} {e:?}");
            }
        }
        // Report: not_found drops, everything else matches loop table.
        for (e, want) in loop_cases {
            let want = if *e == C3Error::NotFound {
                &D::DropAction
            } else {
                want
            };
            assert_eq!(&classify(e, Route::Report), want, "report {e:?}");
        }
        // Pair: refusals, one retry, everything else PairFail.
        assert!(matches!(
            classify(&C3Error::PairingExpired, Route::Pair),
            D::PairReject(_)
        ));
        assert!(matches!(
            classify(&C3Error::PairingUsed, Route::Pair),
            D::PairReject(_)
        ));
        assert!(matches!(
            classify(&C3Error::Forbidden, Route::Pair),
            D::PairReject(_)
        ));
        assert!(matches!(
            classify(&C3Error::Disabled, Route::Pair),
            D::PairReject(_)
        ));
        assert_eq!(
            classify(&C3Error::RateLimited { retry_after: 5 }, Route::Pair),
            D::PairRetryOnce(5)
        );
        for e in [
            C3Error::Unauthorized,
            C3Error::Revoked,
            C3Error::NotFound,
            C3Error::BadInput,
            C3Error::Internal,
            C3Error::Transport("x".into()),
            C3Error::Unknown("u".into()),
        ] {
            assert!(matches!(classify(&e, Route::Pair), D::PairFail(_)), "{e:?}");
        }
    }

    #[test]
    fn backoff_bounds() {
        // streak 1: base 1s × [0.8, 1.2)
        assert!((backoff_secs(1, 0.0) - 0.8).abs() < 1e-9);
        assert!((backoff_secs(1, 0.999) - 1.1996).abs() < 1e-3);
        // streak 4: base 8s
        assert!((backoff_secs(4, 0.5) - 8.0).abs() < 1e-9);
        // huge streak clamps at 60 (after jitter).
        assert!((backoff_secs(10, 0.999) - 60.0).abs() < 1e-9);
        assert!((backoff_secs(9, 0.5) - 60.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn settled_id_suppresses_execution() {
        let server = wiremock::MockServer::start().await;
        let mut st = test_state(&server.uri());
        st.settled.insert("a-1".to_string());
        st.exec_action(&ActionClaim {
            action_id: "a-1".to_string(),
            kind: "exec_shell".to_string(),
            args: "{\"command\":\"echo hi\"}".to_string(),
            grant_id: None,
        })
        .await
        .unwrap();
        assert!(st.running.is_empty());
        let reqs = server.received_requests().await.unwrap_or_default();
        assert!(reqs.is_empty(), "settled id must not execute or report");
    }

    #[tokio::test]
    async fn unknown_capability_refused_before_execution() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(crate::pilot::client::REPORT_PATH))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "action_id": "a-9",
                "status": "failed",
                "output": "capability_refused:exec_admin",
            })))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut st = test_state(&server.uri());
        st.exec_action(&ActionClaim {
            action_id: "a-9".to_string(),
            kind: "exec_admin".to_string(),
            args: "{}".to_string(),
            grant_id: None,
        })
        .await
        .unwrap();
        assert!(st.settled.contains("a-9"));
        server.verify().await;
    }

    #[tokio::test]
    async fn apply_cancels_pre_start_marks_and_settles() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(crate::pilot::client::REPORT_PATH))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "action_id": "never-claimed",
                "status": "failed",
                "output": "cancelled:pre-start",
            })))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut st = test_state(&server.uri());
        st.apply_cancels(vec!["never-claimed".to_string()])
            .await
            .unwrap();
        assert!(st.settled.contains("never-claimed"));
        server.verify().await;
    }

    #[tokio::test]
    async fn apply_cancels_skips_settled_exactly_once() {
        let server = wiremock::MockServer::start().await;
        let mut st = test_state(&server.uri());
        st.settled.insert("a-1".to_string());
        st.apply_cancels(vec!["a-1".to_string()]).await.unwrap();
        let reqs = server.received_requests().await.unwrap_or_default();
        assert!(reqs.is_empty(), "settled cancel must not re-report");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn apply_cancels_kills_live_child() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(crate::pilot::client::REPORT_PATH))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "action_id": "a-run",
                "status": "failed",
                "output": "cancelled:mid-flight",
            })))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut st = test_state(&server.uri());
        let mut child = exec::spawn_shell("sleep 600", &st.canonical_root).unwrap();
        st.running
            .insert("a-run".to_string(), Running::Live { pid: child.pid() });
        st.apply_cancels(vec!["a-run".to_string()]).await.unwrap();
        assert!(st.settled.contains("a-run"));
        assert!(st.running.is_empty());
        // The process group is dead — the wait handle resolves promptly.
        let res = tokio::time::timeout(Duration::from_secs(5), child.wait_mut())
            .await
            .expect("killed child never reaped")
            .unwrap();
        assert!(res.0 != Some(0), "expected non-zero/dead exit, got {res:?}");
        server.verify().await;
    }

    #[tokio::test]
    async fn report_404_drops_without_retry() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(crate::pilot::client::REPORT_PATH))
            .respond_with(
                wiremock::ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"error": "not_found"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut st = test_state(&server.uri());
        let landed = st
            .post_report("a-gone", ReportStatus::Done, Some(0), "x".to_string())
            .await
            .unwrap();
        assert!(landed, "404 report counts as closed");
        server.verify().await;
    }
}
