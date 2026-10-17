//! oxidean-runner — self-hosted Actions runner for Oxidean's JSON protocol.
//!
//! Flow: load-or-register → declare labels → poll fetch_task → execute →
//! report status/log chunks.

use std::time::Duration;

use oxidean_runner::client::Client;
use oxidean_runner::config::Config;
use oxidean_runner::exec;
use oxidean_runner::state::{self, RunnerState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("oxidean-runner: {e}");
            std::process::exit(2);
        }
    };
    tracing::info!(
        origin = %config.origin,
        name = %config.name,
        labels = ?config.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        "oxidean-runner starting"
    );

    let label_names: Vec<String> = config.labels.iter().map(|l| l.name.clone()).collect();

    // Load persisted registration or register fresh.
    let mut client = Client::new(&config.origin, None);
    let st = match state::load(&config.state_path) {
        Some(s) => {
            tracing::info!(runner_id = %s.runner_id, "loaded runner state");
            s
        }
        None => {
            let reg = match &config.registration_token {
                Some(t) => t.clone(),
                None => {
                    eprintln!(
                        "oxidean-runner: no state at {} and OXIDEAN_RUNNER_REGISTRATION_TOKEN unset",
                        config.state_path.display()
                    );
                    std::process::exit(2);
                }
            };
            match client.register(&config.name, &label_names, &reg).await {
                Ok((runner_id, token)) => {
                    let s = RunnerState {
                        runner_id: runner_id.clone(),
                        token,
                        name: Some(config.name.clone()),
                    };
                    if let Err(e) = state::save(&config.state_path, &s) {
                        eprintln!(
                            "oxidean-runner: failed to persist state to {}: {e}",
                            config.state_path.display()
                        );
                        std::process::exit(1);
                    }
                    tracing::info!(runner_id = %runner_id, "registered runner");
                    s
                }
                Err(e) => {
                    eprintln!("oxidean-runner: registration failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    };
    client.set_token(st.token.clone());

    if let Err(e) = client.declare(&label_names).await {
        tracing::warn!("declare failed (continuing): {e}");
    }

    // Graceful drain: SIGTERM/SIGINT stop claiming new work but let the
    // in-flight job finish — the process exits after `execute` returns so a
    // running job is never killed mid-step by a rolling deploy.
    let draining = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let draining = draining.clone();
        tokio::spawn(async move {
            let term = async {
                #[cfg(unix)]
                {
                    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    {
                        Ok(mut s) => {
                            s.recv().await;
                        }
                        Err(_) => std::future::pending::<()>().await,
                    }
                }
                #[cfg(not(unix))]
                std::future::pending::<()>().await
            };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = term => {},
            }
            draining.store(true, std::sync::atomic::Ordering::SeqCst);
            tracing::info!("drain signal received — finishing current job, then exiting");
        });
    }

    loop {
        if draining.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("drained — runner exiting");
            return;
        }
        // Race the fetch against a drain check — a 30s long-poll shouldn't
        // hold SIGTERM/SIGINT hostage for its full hold.
        let fetch = client.fetch_task(config.wait_ms);
        tokio::pin!(fetch);
        let fetched = loop {
            tokio::select! {
                r = &mut fetch => break Some(r),
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    if draining.load(std::sync::atomic::Ordering::SeqCst) {
                        break None;
                    }
                }
            }
        };
        let Some(result) = fetched else {
            continue;
        };
        match result {
            Ok(Some(task)) => {
                let job = task.job_id.clone().unwrap_or_default();
                tracing::info!(job_id = %job, "claimed job");
                let outcome = exec::execute(&client, &task, &config).await;
                tracing::info!(job_id = %job, state = %outcome, "job finished");
            }
            // Long-poll timed out server-side — re-poll immediately, no
            // local cadence. With wait_ms=0 the server answered instantly,
            // so the poll_ms cadence below still applies.
            Ok(None) if config.wait_ms > 0 => {}
            Ok(None) => tokio::time::sleep(Duration::from_millis(config.poll_ms)).await,
            Err(e) => {
                tracing::warn!("fetch_task failed: {e}");
                tokio::time::sleep(Duration::from_millis(config.poll_ms)).await;
            }
        }
    }
}
