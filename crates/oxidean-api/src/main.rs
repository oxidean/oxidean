use oxidean_api::auth::seed;
use oxidean_api::build_cors;
use oxidean_db::Database;
use oxidean_git::assert_git_version;
use tracing_subscriber::EnvFilter;

fn main() {
    // Deep RPC dispatch chains (~25 nested coroutine polls) overflow tokio's
    // 2 MiB worker default in debug builds — seen on vcs.changeset_create.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 << 20)
        .build()
        .expect("tokio runtime")
        .block_on(run());
}

async fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    // D-33 / GIT-09: refuse to serve if system git is missing or older than 2.5.0.
    if let Err(e) = assert_git_version((2, 5, 0)) {
        eprintln!("git version gate failed: {e}");
        std::process::exit(1);
    }

    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    let cors_origins = oxidean_api::public_origin::resolve_cors_origins(
        std::env::var("OXIDEAN_CORS_ORIGINS").ok().as_deref(),
    );
    let cors = match build_cors(&env_name, cors_origins.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cors config error: {e}");
            std::process::exit(1);
        }
    };

    let database_url = std::env::var("DATABASE_URL").ok().filter(|v| !v.is_empty());
    let db = match &database_url {
        Some(url) => {
            let dialect = match oxidean_db::resolve_dialect_from_env(url) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("database config error: {e}");
                    std::process::exit(1);
                }
            };
            let db = match Database::from_env().await {
                Ok(db) => db,
                Err(e) => {
                    eprintln!("database connect failed: {e}");
                    std::process::exit(1);
                }
            };
            tracing::info!("database dialect: {}", dialect.as_str());
            db
        }
        None => {
            tracing::warn!("DATABASE_URL not set; running without a database pool");
            Database::skipped()
        }
    };

    if database_url.is_some() {
        let auto_migrate = std::env::var("OXIDEAN_AUTO_MIGRATE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);
        if auto_migrate {
            if let Err(e) = db.migrate().await {
                eprintln!("migration failed: {e}");
                std::process::exit(1);
            }
            tracing::info!("database migrations applied");
        } else {
            tracing::info!("OXIDEAN_AUTO_MIGRATE=false; run `make db-migrate` to apply migrations");
        }

        // Optional first-admin seed (T-04-13). Phase 6 owns the interactive wizard —
        // this path only runs when both env vars are set and the users table is empty.
        // D-04: maybe_seed_admin sets email_verified_at after create_user.
        if let Err(e) = seed::maybe_seed_admin(&db).await {
            eprintln!("admin seed failed: {e}");
            std::process::exit(1);
        }
    }

    let repos_dir = std::env::var("OXIDEAN_REPOS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("var/repos"));
    let _ssh_stop = oxidean_api::ssh::maybe_spawn_from_env(db.clone(), repos_dir).await;

    let bind = std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|e| {
            eprintln!("bind {bind}: {e}");
            std::process::exit(1);
        });
    tracing::info!("oxidean-api listening on {bind}");

    let email = if database_url.is_some() {
        match db.get_auth_settings().await {
            Ok(settings) => oxidean_api::email::build_email_sender_for_settings(&settings),
            Err(e) => {
                tracing::warn!(error = %e, "auth settings unavailable at boot; using ENV email sender");
                oxidean_api::email::build_email_sender_from_env()
            }
        }
    } else {
        oxidean_api::email::build_email_sender_from_env()
    };
    let state = oxidean_api::AppState::new(db, email, env_name);
    // D-PKG-04: overwrite protection hooks on existing bares (upgrade safety net).
    let sweep = oxidean_api::protection::sweep_protection_hooks(&state.repos_dir).await;
    if sweep.errors > 0 {
        tracing::warn!(
            scanned = sweep.scanned,
            installed = sweep.installed,
            errors = sweep.errors,
            skipped = sweep.skipped,
            "protection hook boot sweep completed with errors; continuing listen"
        );
    }
    let job_cfg = oxidean_api::jobs::JobConfig::from_env();
    oxidean_api::jobs::spawn_background_jobs(
        state.db.clone(),
        state.git.clone(),
        state.repos_dir.clone(),
        state.lfs_dir.clone(),
        state.packages_dir.clone(),
        job_cfg,
        state.vcs.clone(),
    );
    let app = oxidean_api::router_with_state(state, cors);
    axum::serve(listener, app).await.expect("server error");
}
