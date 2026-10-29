/**
 * Oxidean Cloud — Railway TypeScript IaC (D-CLOUD-01…07).
 *
 * Services pull GHCR images built by .github/workflows/images.yml — Railway
 * never builds from the repo (oxidean#124). Managed Postgres. File-configured
 * Caddy gateway (no Docker socket). Secrets use preserve() — never commit
 * production tokens.
 *
 * Each environment seeds its channel tag — :production on production,
 * :staging on staging, :main elsewhere (preview + PR-env bases). Deploy
 * tooling (scripts/railway-image-deploy.sh) retags the channel to the
 * environment's live image on every deploy, so a `config apply` redeploys
 * the exact bits the environment already ran — no source drift.
 *
 * Operators: `railway config plan` then apply only with explicit approval.
 * See .railway/README.md and docs/DEPLOYMENT.md.
 */
import {
  defineRailway,
  group,
  image,
  postgres,
  preserve,
  project,
  service,
  volume,
} from "railway/iac";

/** GHCR image base — images.yml publishes one repo per service. */
const IMAGES = "ghcr.io/oxidean";

export default defineRailway((ctx) => {
  const db = postgres("postgres");

  // Single forge volume covering Compose /var/* paths (D-CLOUD-04).
  // Subdirs: repos, lfs, packages, release-assets, uploads, ssh.
  const forgeData = volume("forge-data", { sizeMB: 20480 });

  // Channel tag seeded per environment — deploy tooling retags it to the
  // live sha-* image on every deploy, so an apply redeploys identical bits.
  const channel = ctx.isEnvironment("production")
    ? "production"
    : ctx.isEnvironment("staging")
      ? "staging"
      : "main";

  const api = service("api", {
    source: image(`${IMAGES}/oxidean-api:${channel}`),
    healthcheck: "/health",
    // All environments migrate on API boot (including production promote). A
    // failed migration exits before listen so Railway keeps the previous replica.
    volumeMounts: {
      "/var": forgeData,
    },
    env: {
      DATABASE_URL: db.env.DATABASE_URL,
      // Set per environment in the dashboard (preview / staging / production).
      OXIDEAN_ENV: preserve(),
      OXIDEAN_DB_DIALECT: "postgres",
      OXIDEAN_AUTO_MIGRATE: "true",
      OXIDEAN_ALLOW_SIGNUP: "true",
      // Branch-protection update-hook helper (same path as Compose / API image).
      OXIDEAN_PROTECTION_HELPER: "/usr/local/bin/oxidean-protection-hook",
      // Align Railway healthcheck PORT with the API listen address.
      PORT: "8080",
      API_BIND: "0.0.0.0:8080",
      OXIDEAN_REPOS_DIR: "/var/repos",
      OXIDEAN_LFS_DIR: "/var/lfs",
      OXIDEAN_PACKAGES_DIR: "/var/packages",
      OXIDEAN_RELEASE_ASSETS_DIR: "/var/release-assets",
      OXIDEAN_VCS_OBJECTS_DIR: "/var/vcs-objects",
      OXIDEAN_SSH_HOST_KEY_DIR: "/var/ssh",
      OXIDEAN_SSH_ENABLED: "true",
      // Default SSH port (clients expect 22). Railway still publishes a random
      // public TCP proxy port → this application port.
      OXIDEAN_SSH_PORT: "22",
      // Track gateway public domain (PR Environments get unique hosts; do not preserve).
      OXIDEAN_PUBLIC_ORIGIN: "https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      OXIDEAN_CORS_ORIGINS: "https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      // OXIDEAN_CLI_DIST_URL is owned by deploy tooling
      // (scripts/railway-staging-api-redeploy.sh and
      // scripts/railway-production-deploy.sh) — a runtime env var the api
      // container's entrypoint fetches at boot, pinned to the `ox-v*` tag
      // release matching the deployed commit (empty = host-only). Not seeded
      // here: a static URL would pin every environment to one feed.
      // See docs/CLI.md § Cloud / self-host.
      OXIDEAN_ADMIN_EMAIL: preserve(),
      OXIDEAN_ADMIN_PASSWORD: preserve(),
      OXIDEAN_RESEND_API_KEY: preserve(),
      OXIDEAN_SMTP_URL: preserve(),
      OXIDEAN_MAIL_FROM: preserve(),
      // AES-256-GCM for Actions secrets, mirror credentials, webhook secrets (D-ACT-17).
      // Required — set a unique value per environment in the dashboard (never commit).
      OXIDEAN_ACTIONS_SECRETS_KEY: preserve(),
      // Oxpilot flag defaults ON via IaC (preview, staging, production — and
      // PR environments, which fork preview's variables). Provider/model stay
      // per-environment dashboard choices (flag off by default in code).
      OXIDEAN_OXPILOT_ENABLED: "1",
      OXIDEAN_OXPILOT_PROVIDER: preserve(),
      OXIDEAN_OXPILOT_MODEL: preserve(),
      // Reusable runner registration token — must equal the `runner` service's
      // OXIDEAN_RUNNER_REGISTRATION_TOKEN so the bundled runner can register.
      // Set per environment in the dashboard (never commit); production should
      // prefer per-runner DB tokens over this env path.
      OXIDEAN_RUNNER_REGISTRATION_TOKEN: preserve(),
      // Optional deterministic web-flow commit-signing key (OpenSSH PEM or
      // base64 PEM). PR Environments inherit preview's vars; unset envs
      // auto-generate on first seeded repo create (production|cloud fail closed).
      OXIDEAN_WEB_FLOW_PRIVATE_KEY: preserve(),
      OXIDEAN_SSH_HOST: "${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      WORKOS_API_KEY: preserve(),
      WORKOS_CLIENT_ID: preserve(),
      OXIDEAN_OIDC_ISSUER: preserve(),
      OXIDEAN_OIDC_CLIENT_ID: preserve(),
      OXIDEAN_OIDC_CLIENT_SECRET: preserve(),
    },
    // Git-over-SSH TCP publish (D-CLOUD-08).
    tcp: [22],
  });

  const web = service("web", {
    source: image(`${IMAGES}/oxidean-web:${channel}`),
    // Private behind gateway; SPA `/` can 403 for bare probes. Rely on gateway + api checks.
    env: {
      // Pin listen port so gateway WEB_PORT=3000 stays correct on private net.
      PORT: "3000",
      // SSR / server-fn RPCs must hit the API on the private network (not
      // 127.0.0.1:8080). Browser clients still use same-origin via gateway.
      OXIDEAN_API_ORIGIN: "http://${{api.RAILWAY_PRIVATE_DOMAIN}}:8080",
      // Public site origin — track gateway domain (PR Environments need this).
      OXIDEAN_PUBLIC_ORIGIN: "https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      // Railway's gateway sanitizes X-Forwarded-* before traffic reaches this
      // tier — trust the edge's chain instead of rewriting it.
      OXIDEAN_WEB_BEHIND_PROXY: "1",
      // CloneBox SSH advertise (SSR’d into the page; match api listen/advertise).
      OXIDEAN_SSH_HOST: "${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      OXIDEAN_SSH_PORT: "22",
    },
  });

  // Actions runner — native oxidean-runner against the JSON protocol.
  // Host execution only: Railway has no Docker socket, so labels are
  // host-mode. Registration state + workspaces persist on runner-data.
  const runnerData = volume("runner-data", { sizeMB: 5120 });
  const runner = service("runner", {
    source: image(`${IMAGES}/oxidean-runner:${channel}`),
    volumeMounts: {
      "/data": runnerData,
    },
    env: {
      // Runner → API over the private network; clone URLs also resolve here
      // (api serves smart HTTP on :8080).
      OXIDEAN_PUBLIC_ORIGIN: "http://${{api.RAILWAY_PRIVATE_DOMAIN}}:8080",
      // Must equal the api service's OXIDEAN_RUNNER_REGISTRATION_TOKEN —
      // set per environment in the dashboard.
      OXIDEAN_RUNNER_REGISTRATION_TOKEN: preserve(),
      OXIDEAN_RUNNER_NAME: "railway-runner",
      // Host execution only — Railway has no docker.sock for docker:// jobs.
      OXIDEAN_RUNNER_LABELS: "ubuntu-latest,self-hosted",
      // Optional PAT for cloning private repositories (http.extraHeader auth).
      OXIDEAN_RUNNER_GIT_TOKEN: preserve(),
    },
  });

  const gateway = service("gateway", {
    source: image(`${IMAGES}/oxidean-gateway:${channel}`),
    // Public HTTP(S) edge — attach custom domain in Railway dashboard.
    env: {
      API_HOST: api.env.RAILWAY_PRIVATE_DOMAIN,
      API_PORT: "8080",
      WEB_HOST: web.env.RAILWAY_PRIVATE_DOMAIN,
      WEB_PORT: "3000",
    },
  });

  // Production does not run the actions-runner service — the promote/rollback
  // set is api/web/gateway (scripts/railway-production-deploy.sh), so the
  // runner (and its runner-data volume, which only mounts onto it) is omitted
  // from the production manifest entirely.
  const forge = ctx.isEnvironment("production")
    ? group("Oxidean Cloud", [db, forgeData, api, web, gateway])
    : group("Oxidean Cloud", [db, forgeData, api, web, runner, gateway]);

  return project("oxidean-cloud", {
    resources: [forge],
  });
});
