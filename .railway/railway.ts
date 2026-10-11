/**
 * Oxidean Cloud — Railway TypeScript IaC (D-CLOUD-01…07).
 *
 * Same Dockerfiles as Compose. Managed Postgres. File-configured Caddy gateway
 * (no Docker socket). Secrets use preserve() — never commit production tokens.
 *
 * Operators: `railway config plan` then apply only with explicit approval.
 * See .railway/README.md and docs/DEPLOYMENT.md.
 */
import {
  defineRailway,
  github,
  group,
  postgres,
  preserve,
  project,
  service,
  volume,
} from "railway/iac";

/** GitHub source for DOCKERFILE builds (repo-root context). */
const REPO = "oxidean/oxidean";

export default defineRailway((ctx) => {
  const db = postgres("postgres");

  // Single forge volume covering Compose /var/* paths (D-CLOUD-04).
  // Subdirs: repos, lfs, packages, release-assets, uploads, ssh.
  const forgeData = volume("forge-data", { sizeMB: 20480 });

  // Wait for CI only on staging (autodeploy from main). Preview/production
  // keep GitHub connected for promote-by-SHA, but Autodeploy must be disabled
  // in the dashboard after apply — IaC cannot express "no deploymentTriggers".
  // Verify: make cloud-production-autodeploy-check
  const stagingWaitForCi = ctx.isEnvironment("staging");

  const api = service("api", {
    source: github(REPO, { checkSuites: stagingWaitForCi || null }),
    build: {
      builder: "DOCKERFILE",
      dockerfilePath: "crates/oxidean-api/Dockerfile",
      watchPatterns: [
        "crates/oxidean-api/**",
        "crates/oxidean-core/**",
        "crates/oxidean-db/**",
        "crates/oxidean-git/**",
        "packages/api-client/**",
      ],
    },
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
      OXIDEAN_SSH_HOST_KEY_DIR: "/var/ssh",
      OXIDEAN_SSH_ENABLED: "true",
      // Default SSH port (clients expect 22). Railway still publishes a random
      // public TCP proxy port → this application port.
      OXIDEAN_SSH_PORT: "22",
      // Track gateway public domain (PR Environments get unique hosts; do not preserve).
      OXIDEAN_PUBLIC_ORIGIN: "https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      OXIDEAN_CORS_ORIGINS: "https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}",
      OXIDEAN_ADMIN_EMAIL: preserve(),
      OXIDEAN_ADMIN_PASSWORD: preserve(),
      OXIDEAN_RESEND_API_KEY: preserve(),
      OXIDEAN_SMTP_URL: preserve(),
      OXIDEAN_MAIL_FROM: preserve(),
      // AES-256-GCM for Actions secrets, mirror credentials, webhook secrets (D-ACT-17).
      // Required — set a unique value per environment in the dashboard (never commit).
      OXIDEAN_ACTIONS_SECRETS_KEY: preserve(),
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
    source: github(REPO, { checkSuites: stagingWaitForCi || null }),
    build: {
      builder: "DOCKERFILE",
      dockerfilePath: "apps/web/Dockerfile",
      watchPatterns: ["apps/web/**", "packages/**"],
    },
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
    source: github(REPO, { checkSuites: stagingWaitForCi || null }),
    build: {
      builder: "DOCKERFILE",
      dockerfilePath: "docker/oxidean-runner/Dockerfile",
      watchPatterns: [
        "docker/oxidean-runner/**",
        "crates/oxidean-runner/**",
        "Cargo.lock",
      ],
    },
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
    source: github(REPO, { checkSuites: stagingWaitForCi || null }),
    build: {
      builder: "DOCKERFILE",
      dockerfilePath: "deploy/cloud/Dockerfile",
      watchPatterns: ["deploy/cloud/**"],
    },
    // Public HTTP(S) edge — attach custom domain in Railway dashboard.
    env: {
      API_HOST: api.env.RAILWAY_PRIVATE_DOMAIN,
      API_PORT: "8080",
      WEB_HOST: web.env.RAILWAY_PRIVATE_DOMAIN,
      WEB_PORT: "3000",
    },
  });

  const forge = group("Oxidean Cloud", [db, forgeData, api, web, runner, gateway]);

  return project("oxidean-cloud", {
    resources: [forge],
  });
});
