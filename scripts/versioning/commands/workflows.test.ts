import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * Workflow-shape assertions for the C4 trigger matrix and C5 latest.json
 * gating — the workflows can't run locally, so the contract is pinned as
 * text/structure checks on the YAML.
 */
const ROOT = join(import.meta.dir, "../../..");
const wf = (name: string) =>
  readFileSync(join(ROOT, ".github/workflows", name), "utf-8");

describe("ox-cli.yml retarget", () => {
  const yml = wf("ox-cli.yml");

  test("stable lane triggers on ox-v* tags, not generic v*", () => {
    expect(yml).toContain('tags: ["ox-v*.*.*"]');
    expect(yml).not.toContain('tags: ["v*.*.*"]');
    expect(yml).not.toContain("refs/tags/v'");
    expect(yml).toContain("refs/tags/ox-v");
  });

  test("oxidean-core is in the path filters (FR5.1)", () => {
    expect(yml).toContain("crates/oxidean-core/**");
  });

  test("package version strips the ox-v prefix", () => {
    expect(yml).toContain('GITHUB_REF_NAME#ox-v');
    expect(yml).not.toContain("GITHUB_REF_NAME#v}");
  });

  test("latest.json is written on stable cuts only, never on prereleases", () => {
    expect(yml).toContain("write latest.json (stable cuts only)");
    expect(yml).toContain('"$version" == *-*');
    expect(yml).toContain("schema_version: 1");
  });

  test("no rolling ox-dist feed — the tag release carries the matrix", () => {
    expect(yml).not.toContain("releases/download/ox-dist");
    expect(yml).not.toContain("gh release upload ox-dist");
    expect(yml).not.toContain("gh release create ox-dist");
    expect(yml).not.toContain("publish-dist");
    // latest.json + SOURCE_COMMIT ship as assets on the tag's own release.
    expect(yml).toContain('releases/download/${{ github.ref_name }}');
    const release = yml.slice(yml.indexOf("  release:"));
    expect(release).toContain("assets/SOURCE_COMMIT");
    expect(release).toContain("gh release create");
  });

  test("release title is the tag itself — no component prefix (#151)", () => {
    // The tag embeds the component name (ox-v0.1.0), so a prefix would
    // read as `ox ox-v0.1.0`.
    expect(yml).toContain('--title "${{ github.ref_name }}"');
    expect(yml).not.toContain('args=(--title "ox ');
  });
});

describe("images.yml image lane", () => {
  const yml = wf("images.yml");

  test("component tag lanes map to their service image", () => {
    // oxidean-api-v*→api, web-v*/oxidean-web-v*→web, oxidean-runner-v*→runner.
    expect(yml).toContain(
      'tags: ["oxidean-api-v*.*.*", "web-v*.*.*", "oxidean-web-v*.*.*", "oxidean-runner-v*.*.*"]',
    );
    expect(yml).toContain("oxidean-api-v*) svc=api");
    expect(yml).toContain("web-v*) svc=web");
    expect(yml).toContain("oxidean-web-v*) svc=web");
    expect(yml).toContain("oxidean-runner-v*) svc=runner");
  });

  test("component prefix is stripped for the image tag", () => {
    expect(yml).toContain('semver="${tagname#oxidean-api-v}"');
    expect(yml).toContain('semver="${tagname#web-v}"');
    expect(yml).toContain('semver="${tagname#oxidean-web-v}"');
    expect(yml).toContain('semver="${tagname#oxidean-runner-v}"');
  });

  test("all four services publish as an all-or-nothing sha tag set", () => {
    // Matrix legs build push:false; a separate publish job pushes after all
    // are green — a sha-<sha> tag set on GHCR is always complete.
    expect(yml).toContain('ALL=\'["api","web","runner","gateway"]\'');
    expect(yml).toContain("push: false");
    expect(yml).toContain("needs: [meta, build]");
    expect(yml).toContain("sha-${sha}");
  });

  test("prerelease tags never get :latest", () => {
    expect(yml).toContain('"$semver" != *-*');
  });

  test("staging-cli-feed only runs for ox-v* ox-cli completions", () => {
    // PR / path-filter main ox-cli runs have no release assets — gating on
    // head_branch prevents the pipefail empty-grep crash that failed Images
    // on every successful PR ox-cli run (workflow_run).
    expect(yml).toContain("staging-cli-feed:");
    expect(yml).toContain(
      "startsWith(github.event.workflow_run.head_branch, 'ox-v')",
    );
    expect(yml).toContain("scripts/ox-dist-tag.sh tag");
  });
});

describe("release.yml generic lane", () => {
  const yml = wf("release.yml");

  test("matches every component tag; ox owns its own lane", () => {
    expect(yml).toContain('tags: ["*-v*.*.*"]');
    expect(yml).toContain("!startsWith(github.ref_name, 'ox-v')");
  });

  test("prerelease tags mark the Release prerelease", () => {
    expect(yml).toContain("--prerelease");
    expect(yml).toMatch(/\^?\[0-9\].*-/) // semver `-` gate
  });

  test("release title is the tag itself — no component prefix (#151)", () => {
    // Tags embed the component name (wire-v0.1.0); a prefix read as
    // `wire wire-v0.1.0`.
    expect(yml).toContain('--title "${{ github.ref_name }}"');
    expect(yml).not.toContain("steps.tag.outputs.component");
  });

  test("dispatch lane takes {component, channel} and never GITHUB_TOKEN-pushes tags", () => {
    expect(yml).toContain("component:");
    expect(yml).toContain("channel:");
    expect(yml).toContain("secrets.RELEASE_TOKEN");
  });
});

describe("release-beta.yml channel lane", () => {
  const yml = wf("release-beta.yml");

  test("fires on every push to main — main is the beta channel", () => {
    expect(yml).toContain("push:");
    expect(yml).toContain("branches: [main]");
    expect(yml).toContain("concurrency:");
  });

  test("tag-only cut: plan + release-tags on the beta channel, never bump/PR", () => {
    expect(yml).toContain("cli.ts plan --channel beta --json");
    expect(yml).toContain("cli.ts release-tags --channel beta");
    expect(yml).not.toContain("cli.ts bump");
    expect(yml).not.toContain("gh pr create");
  });

  test("tag pushes ride RELEASE_TOKEN so downstream lanes fire", () => {
    expect(yml).toContain("secrets.RELEASE_TOKEN");
    expect(yml).not.toContain("github.token");
  });

  test("fork-gated and pins the pushed SHA", () => {
    expect(yml).toContain("github.repository == 'oxidean/oxidean'");
    expect(yml).toContain("ref: ${{ github.sha }}");
  });
});

describe("release tag pushes", () => {
  // Per-tag `git push` loops race GitHub's ref backend and intermittently come
  // back `remote: fatal error in commit_refs` — run 39315353804 pushed 6/8
  // beta tags before the rejection, and `set -e` orphaned the rest. Every cut
  // lane must push its tag set through the single-transaction helper instead.
  for (const name of ["release-beta.yml", "release.yml", "production-deploy.yml"]) {
    test(`${name} pushes tags atomically via push-tags.sh`, () => {
      const yml = wf(name);
      expect(yml).toContain("scripts/push-tags.sh");
      expect(yml).not.toContain('git push origin "$t"');
      expect(yml).not.toContain("for t in $NEW");
    });
  }

  test("push-tags.sh sends at most three tags per push", () => {
    // GitHub creates no push events when more than three tags are pushed at
    // once — the 2026-10-24 stable cut pushed ~38 in one atomic transaction,
    // so the ox-cli / release / images tag lanes never ran and production
    // promoted against a release feed that did not exist. Atomicity stays
    // per batch (the commit_refs race fix) but no batch may exceed 3.
    const helper = readFileSync(join(ROOT, "scripts/push-tags.sh"), "utf-8");
    expect(helper).toContain("--atomic");
    expect(helper).toContain("${#batch[@]} < 3");
  });
});

describe("production-deploy.yml stable cut", () => {
  const yml = wf("production-deploy.yml");

  test("promote creates the stable release (changelog + bump + tags)", () => {
    expect(yml).toContain("cli.ts plan --channel stable");
    expect(yml).toContain("cli.ts changelog --plan");
    expect(yml).toContain("cli.ts bump --plan");
    expect(yml).toContain("cli.ts release-tags --plan");
    expect(yml).toContain("chore(release):");
  });

  test("first cut on a tagless repo is the 0.1.0 baseline (--all)", () => {
    expect(yml).toContain("release-tags --all");
  });

  test("cut writes as the release App (Main ruleset bypass) and deploys the bump commit", () => {
    // main is PR-only; the bump commit lands via the App installation token —
    // never a human PAT, never GITHUB_TOKEN (which would also skip the lanes).
    expect(yml).toContain("actions/create-github-app-token");
    expect(yml).toContain("vars.RELEASE_APP_ID");
    expect(yml).toContain("secrets.RELEASE_APP_PRIVATE_KEY");
    expect(yml).not.toContain("secrets.RELEASE_TOKEN");
    expect(yml).toContain("contents: read");
    expect(yml).not.toContain("contents: write");
    expect(yml).toContain("deploy_sha");
  });

  test("fresh ox-v* tag waits for its asset matrix before deploys", () => {
    expect(yml).toContain("ox-cli");
    expect(yml).toMatch(/wait.*ox|ox.*wait/i);
  });

  test("the ox wait gate polls the release's assets, not workflow-run status", () => {
    // Run 40127322081 false-passed: the newest ox-cli.yml run for the bump
    // sha was a branch-push run that concludes success without ever
    // publishing (the release job is refs/tags/ox-v gated). The gate must
    // see ox-targets.txt + SOURCE_COMMIT + latest.json on the tag's release.
    expect(yml).toContain("releases/tags/");
    expect(yml).toContain("ox_tag");
    expect(yml).toContain("ox-targets.txt");
    expect(yml).toContain("latest.json");
    // head_sha-keyed ox-cli run selection was the false-positive — the
    // commit-level CI check keeps its own head_sha query (correct there).
    expect(yml).not.toContain('endswith("ox-cli.yml")');
  });

  test("promote preflights the resolved CLI feed before touching Railway", () => {
    // A resolved ox tag whose release was never published used to crash-loop
    // the api entrypoint for ~5min after the upsert. The deploy script must
    // refuse first — covers retries where the cut step is skipped.
    const script = readFileSync(
      join(ROOT, "scripts/railway-production-deploy.sh"),
      "utf-8",
    );
    expect(script).toContain("curl -fsSI");
    expect(script).toContain("ox-targets.txt");
  });
});

describe("pr-env-teardown.sh GitHub legs", () => {
  const script = readFileSync(
    join(ROOT, "scripts/pr-env-teardown.sh"),
    "utf-8",
  );

  test("the deployments fallback lists with an explicit GET", () => {
    // gh api defaults to POST once -f params exist — without -X GET the
    // "list" hit create-deployment (403/422) and the sweep reported it as a
    // missing read permission in run 40125953928.
    expect(script).toContain('gh api -X GET "repos/${REPO}/deployments"');
  });

  test("deployment inactivation prefers the job token's deployments:write", () => {
    // GH_ADMIN_TOKEN resolves App → RELEASE_TOKEN → github.token; the PAT
    // may lack deployments:write while the job's GITHUB_TOKEN is granted it
    // (inactivate 403s in runs 40147871008/40150179728). Statuses prefer it.
    expect(script).toContain(
      'GH_TOKEN="${GH_TOKEN:-$admin_token}" gh api -X POST "repos/${REPO}/deployments/${dep}/statuses"',
    );
  });

  test("GHCR version deletes ride the package-token chain, never bare GITHUB_TOKEN", () => {
    // GITHUB_TOKEN can publish packages but never delete them — deletes need
    // a package-delete-capable token (release App Packages grant at the org
    // installation or a PAT with delete:packages), falling back to the admin
    // chain then the job token.
    expect(script).toContain(
      'pkg_token="${GH_PKG_TOKEN:-${GH_ADMIN_TOKEN:-}}"',
    );
    expect(script).toContain(
      'GH_TOKEN="$pkg_token" gh api -X DELETE "orgs/${ORG}/packages',
    );
  });

  test("GHCR version listing prefers the job token, not the delete chain", () => {
    // These packages are connected to this repo (images.yml pushes them with
    // GITHUB_TOKEN), so the job token reads their versions without package
    // scopes — run 40153046588 failed this leg only because it listed via
    // the admin chain (RELEASE_TOKEN can't read packages).
    expect(script).toContain('list_token="${GH_TOKEN:-$pkg_token}"');
    expect(script).toContain('pr_version_ids "$list_token" "$pkg"');
  });

  test("the teardown mints are per-leg, and Packages mints against the org", () => {
    // A single atomic mint let one missing grant 422 every leg at once; the
    // split keeps the env leg alive when Packages isn't granted — and org
    // packages need an org-installation token (owner:), not a repo token.
    for (const name of ["pr-env-sweep.yml", "images.yml"]) {
      const yml = wf(name);
      expect(yml).toContain("id: apptok_env");
      expect(yml).toContain("id: apptok_pkg");
      expect(yml).toContain("permission-administration: write");
      expect(yml).toContain("permission-packages: write");
      expect(yml).toContain("owner: ${{ github.repository_owner }}");
      expect(yml).toContain(
        "GH_PKG_TOKEN: ${{ steps.apptok_pkg.outputs.token || secrets.RELEASE_TOKEN }}",
      );
      expect(yml).toContain(
        "GH_ADMIN_TOKEN: ${{ steps.apptok_env.outputs.token || secrets.RELEASE_TOKEN }}",
      );
    }
  });

  test("privileged chains never fall back to the job token", () => {
    // github.token can publish but never delete packages, and there is no
    // `environments` job permission at all — so it only ever reproduced the
    // 403 a leg already degrades. It stays for reads + deployments:write.
    expect(script).toContain('admin_token="${GH_ADMIN_TOKEN:-}"');
    for (const name of ["pr-env-sweep.yml", "images.yml"]) {
      expect(wf(name)).not.toContain("|| github.token }}");
    }
  });

  test("scope walls degrade to warnings + a step-summary leak report", () => {
    // Missing grants are permanent until the App/PAT gains them — failing
    // the run every time trained everyone to ignore a permanently red job
    // (runs 40147871008/40150179728/40153046588). Real errors still fail.
    expect(script).toContain("is_scope_denial");
    expect(script).toContain("GITHUB_STEP_SUMMARY");
    expect(script).toContain("OXIDEAN_TEARDOWN_STRICT");
  });
});

describe("ci.yml web-browser suite", () => {
  const yml = wf("ci.yml");

  test("browser files shard across fresh Chromium sessions", () => {
    // One long-lived Chromium session serves the whole serial suite; on
    // shared CI hardware it drops the connection late in the run
    // (VITEST_BROWSER_CONNECTION_CLOSED, ~file 37/39 on consecutive
    // attempts). Sharding keeps every session far below the accumulated-
    // state/lifetime cliff — never collapse this back to one invocation.
    expect(yml).toContain("for shard in 1 2 3 4 5");
    expect(yml).toContain('--shard="${shard}/5"');
    expect(yml).not.toContain("run: make test-web-browser");
  });

  test("browser-session retry keys on the connection signature only", () => {
    // Assertion failures exit immediately — retrying them would mask the
    // DOM races this suite exists to catch. Only a dropped Chromium session
    // retries, once, with playwright browser debug logs for forensics.
    expect(yml).toContain("VITEST_BROWSER_CONNECTION_CLOSED");
    expect(yml).toContain("DEBUG=pw:browser");
  });
});
