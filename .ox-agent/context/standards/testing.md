# Testing

Posture: **test-after** — implement the change, then write and run the tests
its change-type minimum requires; full CI must be green before merge. An
in-repo `tdd` skill exists for deliberate red-first work but is not the
mandated posture.

## Minimums by change type

| Change type | Minimum |
|-------------|---------|
| Pure helper | Unit test |
| UI + Query / session | happy-dom integration test |
| High-risk interactive UI (Checkbox / RadioGroup / Select / Switch / portals / `form.Subscribe`) | `*.browser.test.tsx` + `browser-coverage` manifest row |
| Auth / RPC contract | Rust integration test and/or stack e2e |
| RPC schema | `make rpc-sync-check` clean |
| Dialect-touching DB code | `dialect_*` test (CI runs `dialect_probe` only — full parity is local-capable, recorded debt) |

## Tooling and gates

- Rust: cargo-nextest (`.config/nextest.toml`); web: Vitest 5 (unit /
  integration / browser / e2e-stack projects); Playwright real Chromium;
  wiremock for outbound HTTP; protocol smokes run real `git`, `ssh`,
  `git-lfs` against a live Compose stack (fail-closed under
  `SMOKE_REQUIRE_STACK=1`).
- CI enforces a weighted composite coverage gate at floor **0.70**
  (web unit 0.25 + web integration 0.40 + e2e checklist 0.35). Rust coverage
  is collected, not gated — recorded debt.
- Route- and browser-coverage manifests are change-aware: fresh skip-only
  rows fail CI; zero `#[ignore]` / `it.skip` / `test.skip` in the tree.
- `make test` is only nextest + web unit/integration — a green local run
  proves far less than green CI; browser, stack e2e, and smokes are
  separate targets.
- CI/Docker changes are verified locally with docker + `act` before
  shipping; prefer Docker (`make up`, `make smoke`) over bare-metal
  debugging.

Details: [`docs/TESTING.md`](../../docs/TESTING.md).
