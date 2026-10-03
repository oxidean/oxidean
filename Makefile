# One target per .PHONY line group — checkmake does not parse backslash-continued
# .PHONY lists, so keep each declaration on a single physical line.
.PHONY: help makefile-lint
.PHONY: dev rpc-gen rpc-sync-check check-stack-presets sync-stack-presets
.PHONY: up down logs test smoke smoke-git-https smoke-git-ssh
.PHONY: smoke-git-lfs
.PHONY: smoke-packages
.PHONY: smoke-actions
.PHONY: smoke-protection
.PHONY: smoke-protocol-ci smoke-compose-ci
.PHONY: cloud-plan cloud-docs cloud-production-autodeploy-check
.PHONY: up-mysql up-sqlite down-mysql down-sqlite smoke-mysql smoke-sqlite
.PHONY: up-dev-auth down-dev-auth up-with-dev-auth down-with-dev-auth test-e2e-stack seed-actions-demo test-e2e-actions
.PHONY: db-migrate db-switch-dialect db-matrix
.PHONY: coverage-web coverage-rust coverage-weighted coverage-contract
.PHONY: route-coverage-check browser-coverage-check ui-coverage-change-contract
.PHONY: route-coverage-check-pr browser-coverage-check-pr
.PHONY: web-lint web-format-check test-web-browser

COMPOSE ?= docker compose
COMPOSE_FILE ?= docker-compose.yml

help:
	@echo "Oxidean targets:"
	@echo "  make makefile-lint  - parse Makefile + checkmake (CI early gate)"
	@echo "  make dev            - local API + web (Vite proxy; D-10)"
	@echo "  make rpc-gen        - regenerate packages/api-client from Rust"
	@echo "  make rpc-sync-check - fail if generated client is out of sync"
	@echo "  make check-stack-presets - validate stack-presets catalog + size budget"
	@echo "  make sync-stack-presets - stamp/refresh stack-presets metadata"
	@echo "  make up             - docker compose up (Traefik on :80)"
	@echo "  make up-mysql       - compose up with the MySQL profile (D-07)"
	@echo "  make up-sqlite      - compose up with SQLite file in ./var (D-19)"
	@echo "  make up-dev-auth    - Mailpit + OIDC mock + Resend/WorkOS stubs (docs/dev-auth.md)"
	@echo "  make up-with-dev-auth - make up + attach API to stubs (SMTP→Mailpit)"
	@echo "  make test-e2e-stack - full Vitest e2e vs API + Mailpit/OIDC/stubs"
	@echo "  make down           - docker compose down"
	@echo "  make down-mysql     - docker compose down (mysql overlay)"
	@echo "  make down-sqlite    - docker compose down (sqlite overlay)"
	@echo "  make down-dev-auth  - stop local auth/email stubs"
	@echo "  make down-with-dev-auth - tear down full stack from up-with-dev-auth"
	@echo "  make logs           - follow compose logs"
	@echo "  make test           - cargo nextest + JS Vitest (unit/integration/e2e)"
	@echo "  make coverage-web   - Vitest unit+integration coverage (json-summary/lcov)"
	@echo "  make coverage-rust  - Rust lib/test coverage via cargo-llvm-cov (optional)"
	@echo "  make coverage-weighted - D-QH-02 weighted gate (25/40/35, floor 0.70)"
	@echo "  make coverage-contract - aggregator contract self-test"
	@echo "  make route-coverage-check / browser-coverage-check - page + high-risk UI coverage gates"
	@echo "  make web-lint / web-format-check / test-web-browser - oxlint, oxfmt, Chromium DOM races"
	@echo "  make smoke-actions  - Actions/runner Compose smoke (ACT-04/05; skip-ok without Docker)"
	@echo "  make smoke          - compose bring-up smoke (PLAT-01)"
	@echo "  make smoke-protection - ORG-06 helper + HTTPS/SSH protected-push denial (D-PKG-03)"
	@echo "  make smoke-git-https - Traefik .git → API + git ls-remote smoke (GIT-02)"
	@echo "  make smoke-git-ssh   - Compose TCP SSH + git ls-remote/push smoke (GIT-03)"
	@echo "  make smoke-git-lfs   - Traefik .git/info/lfs batch routing smoke (GIT-12)"
	@echo "  make smoke-packages  - Traefik /v2|/npm|/generic → API smoke (PKG-01..03)"
	@echo "  make smoke-protocol-ci - Compose up + smoke-git-* + smoke-packages + smoke-protection (D-QH-04; fail-closed)"
	@echo "  make smoke-compose-ci - Compose dialect bring-up smoke for CI (D-CI-01; DIALECT=postgres|sqlite|mysql)"
	@echo "  make smoke-mysql    - bring-up smoke asserting dialect=mysql"
	@echo "  make smoke-sqlite   - bring-up smoke asserting dialect=sqlite"
	@echo "  make cloud-plan     - railway config plan (Oxidean Cloud IaC; no apply)"
	@echo "  make cloud-docs     - print pointers to cloud deploy docs"
	@echo "  make cloud-production-autodeploy-check - assert production has no GitHub autodeploy triggers"
	@echo "  make db-migrate     - apply migrations for DATABASE_URL"
	@echo "  make db-switch-dialect - migrate an EMPTY target DB to a new dialect"
	@echo "  make db-matrix      - run the dialect probe test against DATABASE_URL"
	@echo ""
	@echo "Sample DATABASE_URLs:"
	@echo "  postgres://oxidean:oxidean@localhost:5432/oxidean"
	@echo "  mysql://oxidean:oxidean@127.0.0.1:3306/oxidean"
	@echo "  sqlite:./var/oxidean.db"

dev:
	@echo "Starting API + web (rpc-gen once)..."
	@$(MAKE) rpc-gen
	@echo "Run in two terminals:"
	@echo "  OXIDEAN_ENV=development API_BIND=127.0.0.1:8080 cargo run -p oxidean-api --bin oxidean-api"
	@echo "  bun run --filter @oxidean/web dev"

rpc-gen:
	cargo run -q -p oxidean-api --bin rpc-gen

rpc-sync-check:
	@./scripts/check-rpc-sync.sh

makefile-lint:
	@./scripts/check-makefile.sh

check-stack-presets:
	@./scripts/check-stack-presets.sh

sync-stack-presets:
	@bun scripts/sync-stack-presets.mjs --stamp

up:
	$(COMPOSE) -f $(COMPOSE_FILE) up --build -d

up-mysql:
	$(COMPOSE) -f docker-compose.yml -f docker-compose.mysql.yml --profile mysql up --build -d

up-sqlite:
	mkdir -p var
	@host="$$(./scripts/sqlite-host-dir.sh)"; \
	printf 'OXIDEAN_SQLITE_HOST_DIR=%s\n' "$$host" > .env.sqlite; \
	$(COMPOSE) --env-file .env.sqlite -f docker-compose.yml -f docker-compose.sqlite.yml up --build -d

down:
	$(COMPOSE) -f $(COMPOSE_FILE) down --remove-orphans

down-mysql:
	$(COMPOSE) -f docker-compose.yml -f docker-compose.mysql.yml down --remove-orphans

down-sqlite:
	$(COMPOSE) -f docker-compose.yml -f docker-compose.sqlite.yml down --remove-orphans

up-dev-auth:
	@bash -c 'source ./scripts/docker-wsl-creds.sh; $(COMPOSE) -f docker-compose.dev-auth.yml --profile dev-auth up --build -d'
	@echo "==> Mailpit UI  http://127.0.0.1:8025"
	@echo "==> OIDC mock   http://127.0.0.1:9090/default"
	@echo "==> HTTP stubs  http://127.0.0.1:9092"
	@echo "==> Env template: cp docs/dev-auth.env.example .env.dev-auth"
	@echo "==> Compose API: make up-with-dev-auth (SMTP→Mailpit)"
	@echo "==> Docs: docs/dev-auth.md"

# Main Traefik stack + Mailpit/stubs; API SMTP defaults to smtp://mailpit:1025.
up-with-dev-auth:
	@bash -c 'source ./scripts/docker-wsl-creds.sh; \
	  export OXIDEAN_HOST_GATEWAY_IP="$$(./scripts/dev-auth/host-gateway-ip.sh)"; \
	  $(COMPOSE) -f docker-compose.yml \
	    -f docker-compose.dev-auth.yml \
	    -f docker-compose.dev-auth-attach.yml \
	    --profile dev-auth --profile actions up --build -d'
	@./scripts/dev-auth/promote-smtp-settings.sh
	@echo "==> App        http://localhost"
	@echo "==> Mailpit UI http://127.0.0.1:8025"
	@echo "==> OIDC mock  http://127.0.0.1:9090/default"
	@echo "==> HTTP stubs http://127.0.0.1:9092"
	@echo "==> Runner     actions profile (host-exec, label ubuntu-latest)"
	@echo "==> Optional env: docs/dev-auth.env.compose.example"
	@echo "==> Docs: docs/dev-auth.md"

down-dev-auth:
	@bash -c 'source ./scripts/docker-wsl-creds.sh; $(COMPOSE) -f docker-compose.dev-auth.yml --profile dev-auth down --remove-orphans'

down-with-dev-auth:
	@bash -c 'source ./scripts/docker-wsl-creds.sh; \
	  $(COMPOSE) -f docker-compose.yml \
	    -f docker-compose.dev-auth.yml \
	    -f docker-compose.dev-auth-attach.yml \
	    --profile dev-auth --profile actions down --remove-orphans'

test-e2e-stack:
	./scripts/dev-auth/run-stack-e2e.sh

# Seed the ci-demo repo + workflow and poll the Actions run to green.
# Works against local compose (default http://localhost) or any origin:
#   OXIDEAN_ORIGIN=https://... OXIDEAN_SEED_USER=... OXIDEAN_SEED_PASSWORD=... make seed-actions-demo
seed-actions-demo:
	./scripts/dev-auth/seed-actions-demo.sh

# Full pipeline e2e: API (sqlite) + host runner + seed — no Docker required.
test-e2e-actions:
	./scripts/e2e-actions-pipeline.sh

logs:
	$(COMPOSE) -f $(COMPOSE_FILE) logs -f

test:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cargo nextest run --workspace; \
	else \
		echo "cargo-nextest not found; falling back to cargo test (install: cargo install cargo-nextest --locked)"; \
		cargo test --workspace; \
	fi
	bun run test

smoke:
	@EXPECT_DIALECT=postgres ./scripts/compose-smoke.sh

# ORG-06 / D-PKG-03: fresh Compose up, assert helper binary, deny HTTPS push to
# reviews-required protected branch (enforce_admins). When SSH TCP 2222 is up and
# SMOKE_SKIP_LS_REMOTE is unset, also deny the same push over SSH. Wipes volumes.
# Docker-missing skips exit 0 locally; CI=true / SMOKE_REQUIRE_STACK=1 fails closed.
smoke-protection:
	@./scripts/compose-smoke-protection.sh

# Requires stack already up (`make up`). Public repo at SMOKE_GIT_OWNER/SMOKE_GIT_REPO;
# optional SMOKE_PAT=oxidean_pat_… for push. See scripts/smoke-git-https.sh.
# Docker-missing skips exit 0 locally; CI=true / SMOKE_REQUIRE_STACK=1 fails closed.
smoke-git-https:
	@./scripts/smoke-git-https.sh

# Compose TCP 2222 + ls-remote/push over scp-style remotes (D-SSH-02 / D-SSH-07).
# Requires stack with SSH listener. See scripts/smoke-git-ssh.sh.
smoke-git-ssh:
	@./scripts/smoke-git-ssh.sh

# Requires stack already up (`make up`). Asserts .git/info/lfs is not SPA HTML.
# See scripts/smoke-git-lfs.sh.
smoke-git-lfs:
	@./scripts/smoke-git-lfs.sh

# Requires stack already up (`make up`). Traefik PathPrefix /v2|/npm|/generic → api.
# See scripts/smoke-packages.sh.
smoke-packages:
	@./scripts/smoke-packages.sh

# D-QH-04: bring up Compose, run protocol smokes fail-closed, tear down.
# Ends with smoke-protection (ORG-06/D-PKG-03; DEBT-01) which re-ups a fresh
# stack, wiping volumes. Default skips client ls-remote/LFS push (no seeded
# repo); set SMOKE_SKIP_*=0 + fixtures for full.
smoke-protocol-ci:
	@./scripts/ci-smoke-protocol.sh

# D-CI-01…04: fail-closed Compose bring-up for one dialect (default postgres).
# Usage: make smoke-compose-ci DIALECT=sqlite
smoke-compose-ci:
	@./scripts/ci-compose-smoke.sh "$(or $(DIALECT),postgres)"

# D-CLOUD-07: preview Railway IaC diff only — never auto-apply from Make/CI.
cloud-plan:
	@if ! command -v railway >/dev/null 2>&1; then \
		echo "railway CLI not found. Install CLI ≥ 5.42.1, run railway link, then retry."; \
		echo "Docs: .railway/README.md and docs/DEPLOYMENT.md"; \
		exit 1; \
	fi
	@echo "==> railway config plan (review only — apply requires explicit human approval)"
	@railway config plan

cloud-production-autodeploy-check:
	@bash scripts/railway-production-autodeploy-check.sh

cloud-docs:
	@echo "Oxidean Cloud:"
	@echo "  IaC:       .railway/railway.ts"
	@echo "  Gateway:   deploy/cloud/Caddyfile"
	@echo "  Operator:  docs/DEPLOYMENT.md (Oxidean Cloud section)"
	@echo "  Plan only: make cloud-plan"
	@echo "  Apply:     scripts/railway-apply.sh --environment <env> --apply  # human-approved only"

smoke-mysql:
	@COMPOSE_FILES="-f docker-compose.yml -f docker-compose.mysql.yml" COMPOSE_PROFILES=mysql EXPECT_DIALECT=mysql ./scripts/compose-smoke.sh

smoke-sqlite:
	mkdir -p var
	@host="$$(./scripts/sqlite-host-dir.sh)"; \
	OXIDEAN_SQLITE_HOST_DIR="$$host" COMPOSE_FILES="-f docker-compose.yml -f docker-compose.sqlite.yml" EXPECT_DIALECT=sqlite ./scripts/compose-smoke.sh; \
	if echo "$$host" | grep -Eq '^[A-Za-z]:/'; then \
	  src="$$(wslpath "$$host")/oxidean.db"; \
	  if [ -f "$$src" ]; then cp -f "$$src" ./var/oxidean.db; echo "==> mirrored $$src -> ./var/oxidean.db"; fi; \
	fi

db-migrate:
	cargo run -q -p oxidean-db --bin migrate

db-switch-dialect:
	./scripts/db-switch-dialect.sh $(ARGS)

db-matrix:
	cargo test -p oxidean-db --test dialect_probe -- --nocapture

# D-QH-02 coverage collect + weighted gate (see docs/TESTING.md).
# Collect continues when Vitest exits non-zero so summaries are still emitted
# (reportOnFailure); web-octane remains the hard test pass/fail job.
coverage-web:
	@mkdir -p var/coverage
	@bun run --filter @oxidean/web test:coverage:unit \
		|| echo "==> unit coverage finished with test failures (summaries may still exist)"
	@bun run --filter @oxidean/web test:coverage:integration \
		|| echo "==> integration coverage finished with test failures (summaries may still exist)"
	@test -f apps/web/coverage/unit/coverage-summary.json
	@test -f apps/web/coverage/integration/coverage-summary.json
	@cp -f apps/web/coverage/unit/coverage-summary.json var/coverage/web-unit-summary.json
	@cp -f apps/web/coverage/integration/coverage-summary.json var/coverage/web-integration-summary.json
	@echo "==> web coverage summaries in var/coverage/"

# Prefer cargo-llvm-cov; nextest keeps process-per-test isolation when installed.
# CI installs both via taiki-e/install-action (tool: cargo-llvm-cov,nextest) plus
# the llvm-tools-preview component on the Rust toolchain.
coverage-rust:
	@mkdir -p var/coverage
	@if command -v cargo-llvm-cov >/dev/null 2>&1; then \
		if command -v cargo-nextest >/dev/null 2>&1; then \
			cargo llvm-cov nextest --workspace --lcov --output-path var/coverage/rust-lcov.info; \
		else \
			cargo llvm-cov --workspace --lcov --output-path var/coverage/rust-lcov.info; \
		fi; \
		cargo llvm-cov report --json --output-path var/coverage/rust-summary.json; \
		echo "==> rust coverage in var/coverage/rust-*.{info,json}"; \
	else \
		echo "==> cargo-llvm-cov not installed; skipping Rust coverage collect"; \
		echo "    install: cargo install cargo-llvm-cov --locked"; \
		echo "    CI: taiki-e/install-action tool: cargo-llvm-cov,nextest"; \
		echo "skipped" > var/coverage/rust-skipped.txt; \
	fi

coverage-contract:
	@./scripts/coverage-weighted.contract.sh

route-coverage-check:
	@./scripts/route-coverage-check.sh

# Change-aware (PR-equivalent): fail skip-only for routes touched vs origin/main.
route-coverage-check-pr:
	@UI_COVERAGE_BASE=$${UI_COVERAGE_BASE:-origin/main} ./scripts/route-coverage-check.sh

browser-coverage-check:
	@./scripts/browser-coverage-check.sh

# Change-aware (PR-equivalent): fail skip-only for high-risk UI touched vs origin/main.
browser-coverage-check-pr:
	@UI_COVERAGE_BASE=$${UI_COVERAGE_BASE:-origin/main} ./scripts/browser-coverage-check.sh

ui-coverage-change-contract:
	@./scripts/ui-coverage-change.contract.sh

web-lint:
	bun run --filter @oxidean/web lint
	bun run scripts/check-octane-dom-races.ts
	bun run scripts/check-internal-anchors.ts
	bun run scripts/check-loader-deps.ts

web-format-check:
	bun run --filter @oxidean/web format:check

test-web-browser:
	bun run --filter @oxidean/web test:browser

coverage-weighted: coverage-web
	@mkdir -p var/coverage
	@$(MAKE) --no-print-directory coverage-rust
	@./scripts/coverage-e2e-checklist.sh | tee var/coverage/e2e-checklist.txt
	@./scripts/coverage-weighted.sh \
		--unit-json var/coverage/web-unit-summary.json \
		--integration-json var/coverage/web-integration-summary.json \
		--e2e-checklist | tee var/coverage/weighted.txt

# See scripts/smoke-actions.sh.
smoke-actions:
	@./scripts/smoke-actions.sh
