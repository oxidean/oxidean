/**
 * Route coverage manifest (G-11.1-15 / Phase 11.1-08).
 *
 * Every user-facing apps/web src/routes page (.tsrx) must appear here with at least
 * one of: happy-dom render test, stack-browser suite, or documented skip.
 * Outlet-only layouts and `__root` are marked `layoutOnly` and excluded from
 * the required set by `scripts/route-coverage-check.sh`.
 *
 * Paths are repo-relative from the Oxidean root.
 */

export type RouteCoverageKind = "happy-dom" | "stack-browser" | "skip";

export type RouteCoverageEvidence =
  | { kind: "happy-dom"; test: string }
  | { kind: "stack-browser"; test: string }
  | { kind: "skip"; rationale: string };

export type RouteCoverageEntry = {
  /** Path relative to `apps/web/src/routes/` */
  route: string;
  /** When true, excluded from the required coverage set (shell / Outlet-only). */
  layoutOnly?: boolean;
  coverage: RouteCoverageEvidence[];
};

const AUTH_UI = "apps/web/e2e/stack-browser/auth-ui.stack.browser.test.tsx";
const FORGE_REPO = "apps/web/e2e/stack-browser/forge-repo.stack.browser.test.tsx";
const FORGE_ISSUES = "apps/web/e2e/stack-browser/forge-issues-releases.stack.browser.test.tsx";
const FORGE_SSH_ORGS = "apps/web/e2e/stack-browser/forge-packages-ssh-orgs.stack.browser.test.tsx";
const FORGE_ADMIN = "apps/web/e2e/stack-browser/forge-admin.stack.browser.test.tsx";

export const routeCoverageManifest: RouteCoverageEntry[] = [
  // --- shells / Outlet-only layouts (excluded from required set) ---
  { route: "__root.tsrx", layoutOnly: true, coverage: [] },
  { route: "$owner.tsrx", layoutOnly: true, coverage: [] },
  { route: "$owner.$repo.issues.tsrx", layoutOnly: true, coverage: [] },
  { route: "$owner.$repo.releases.tsrx", layoutOnly: true, coverage: [] },
  { route: "setup.tsrx", layoutOnly: true, coverage: [] },

  // --- auth + marketing (11.1-04) ---
  {
    route: "login.tsrx",
    coverage: [
      { kind: "happy-dom", test: "apps/web/src/routes/login.integration.test.ts" },
      { kind: "stack-browser", test: AUTH_UI },
    ],
  },
  {
    route: "signup.tsrx",
    coverage: [
      { kind: "happy-dom", test: "apps/web/src/routes/signup.integration.test.ts" },
      { kind: "stack-browser", test: AUTH_UI },
    ],
  },
  {
    route: "verify.tsrx",
    coverage: [{ kind: "happy-dom", test: "apps/web/src/routes/verify.integration.test.ts" }],
  },
  {
    route: "reset-password.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/reset-password.integration.test.ts",
      },
    ],
  },
  {
    route: "status.tsrx",
    coverage: [
      { kind: "happy-dom", test: "apps/web/src/routes/status.integration.test.ts" },
      { kind: "stack-browser", test: AUTH_UI },
    ],
  },
  {
    route: "index.tsrx",
    coverage: [
      { kind: "stack-browser", test: AUTH_UI },
      {
        kind: "skip",
        rationale:
          "happy-dom covers tree-gate helpers + SignedInHome module, not full page mount; browser covers signed-in home via auth.me dedupe",
      },
    ],
  },

  // --- setup wizard ---
  {
    route: "setup.index.tsrx",
    coverage: [{ kind: "happy-dom", test: "apps/web/src/routes/setup.integration.test.ts" }],
  },
  {
    route: "setup.credentials.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/setup.credentials.integration.test.ts",
      },
    ],
  },

  // --- user settings ---
  {
    route: "settings/general.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/general.integration.test.ts",
      },
    ],
  },
  {
    route: "settings/profile.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/profile.integration.test.ts",
      },
    ],
  },
  {
    route: "settings/emails.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/emails.integration.test.ts",
      },
    ],
  },
  {
    route: "settings/ssh-keys.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/ssh-keys.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_SSH_ORGS },
    ],
  },
  {
    route: "settings/tokens.tsrx",
    layoutOnly: true,
    coverage: [],
  },
  {
    route: "settings/tokens.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/tokens.integration.test.ts",
      },
    ],
  },
  {
    route: "settings/tokens.new.tsrx",
    layoutOnly: true,
    coverage: [],
  },
  {
    route: "settings/tokens.new.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/settings/tokens.integration.test.ts",
      },
    ],
  },
  {
    route: "settings/tokens.new.fine-grained.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Fine-grained token wizard deferred; classic tokens happy-dom + packages scope tests cover PAT surface",
      },
    ],
  },

  // --- create flows ---
  {
    route: "new.tsrx",
    coverage: [
      { kind: "happy-dom", test: "apps/web/src/routes/new.integration.test.ts" },
      {
        kind: "stack-browser",
        test: "apps/web/e2e/stack-browser/new-repo-template.stack.browser.test.tsx",
      },
    ],
  },
  {
    route: "orgs.new.tsrx",
    coverage: [{ kind: "happy-dom", test: "apps/web/src/routes/orgs.new.integration.test.ts" }],
  },
  {
    route: "dashboard.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Hard-404 beforeLoad only (notFound); no UI to mount — covered by dashboard.integration.test.ts contract",
      },
    ],
  },
  {
    route: "invites.$token.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/invites.$token.integration.test.ts",
      },
    ],
  },

  // --- admin (G-11.1-15 / 11.1-07) ---
  {
    route: "admin/lfs.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/lfs.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_ADMIN },
    ],
  },
  {
    route: "admin/users.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/users.integration.test.ts",
      },
    ],
  },
  {
    route: "admin/packages.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/packages.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_ADMIN },
    ],
  },
  {
    route: "admin/auth.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/auth.integration.test.ts",
      },
    ],
  },
  {
    route: "admin/runners.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Phase 19 admin registration-token UI; happy-dom deferred — covered by actions_secrets/dispatch_policy nextest + manual Admin runners smoke",
      },
    ],
  },
  {
    route: "admin/templates.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/templates.integration.test.ts",
      },
    ],
  },
  {
    route: "admin/mcp.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/admin/mcp.integration.test.ts",
      },
    ],
  },

  // --- forge repo chrome + code browse (11.1-04) ---
  {
    route: "$owner.$repo.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_REPO },
    ],
  },
  {
    route: "$owner.$repo.index.tsrx",
    coverage: [
      { kind: "stack-browser", test: FORGE_REPO },
      {
        kind: "skip",
        rationale:
          "Code home exercised via forge-repo stack-browser + layout chrome happy-dom; dedicated index mount deferred",
      },
    ],
  },
  {
    route: "$owner.$repo.packages.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_REPO }],
  },
  {
    route: "$owner.$repo.actions.tsrx",
    layoutOnly: true,
    coverage: [],
  },
  {
    route: "$owner.$repo.actions.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.actions.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.actions.$run.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.actions.$run.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.settings.actions.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Phase 19 repo Actions enable/secrets settings panel; happy-dom deferred — RPC covered by actions_secrets nextest",
      },
    ],
  },
  {
    route: "$owner.$repo.tree.$.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.blob.$.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.blame.$.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Blame view deferred behind blob/tree happy-dom; add mount when blame UX changes",
      },
    ],
  },
  {
    route: "$owner.$repo.commits.$.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Commits list not in D-QH-03 matrix; deferred stack-browser",
      },
    ],
  },
  {
    route: "$owner.$repo.commit.$sha.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Commit detail not in D-QH-03 matrix; deferred stack-browser",
      },
    ],
  },
  {
    route: "$owner.$repo.compare.$.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Compare view deferred; no happy-dom mount yet",
      },
    ],
  },
  {
    route: "$owner.$repo.branches.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Branches list deferred behind refs chrome on code home",
      },
    ],
  },
  {
    route: "$owner.$repo.tags.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Tags list deferred; release create covers tag selection path",
      },
    ],
  },
  {
    route: "$owner.$repo.settings.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.settings.integration.test.ts",
      },
      {
        kind: "stack-browser",
        test: "apps/web/e2e/stack-browser/forge-mirror.stack.browser.test.tsx",
      },
    ],
  },
  {
    route: "$owner.$repo.fork.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.fork.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.activity.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.social-lists.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.stargazers.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.social-lists.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.watchers.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.social-lists.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.forks.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.social-lists.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.search.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.search.integration.test.ts",
      },
    ],
  },

  // --- pulls (Phase 12) ---
  {
    route: "$owner.$repo.pulls.tsrx",
    layoutOnly: true,
    coverage: [],
  },
  {
    route: "$owner.$repo.pulls.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.pulls.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.pulls.new.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.pulls.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.pull.$n.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.pulls.integration.test.ts",
      },
    ],
  },

  // --- issues ---
  {
    route: "$owner.$repo.issues.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.$repo.issues.new.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_ISSUES },
    ],
  },
  {
    route: "$owner.$repo.issues.$n.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_ISSUES },
    ],
  },
  {
    route: "$owner.$repo.issues.labels.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Repo labels settings UI deferred; issue label attach covered in issues happy-dom",
      },
    ],
  },

  // --- releases ---
  {
    route: "$owner.$repo.releases.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.$repo.releases.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_ISSUES },
    ],
  },
  {
    route: "$owner.$repo.releases.new.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_ISSUES }],
  },
  {
    route: "$owner.$repo.releases.$tag.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_ISSUES }],
  },

  // --- owner / org ---
  {
    route: "$owner.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.layout.integration.test.ts",
      },
    ],
  },
  {
    route: "$owner.packages.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.packages.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_REPO },
    ],
  },
  {
    route: "$owner.settings.tsrx",
    layoutOnly: true,
    coverage: [],
  },
  {
    route: "$owner.settings.index.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.settings.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_SSH_ORGS },
    ],
  },
  {
    route: "$owner.settings.members.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/$owner.settings.integration.test.ts",
      },
      { kind: "stack-browser", test: FORGE_SSH_ORGS },
    ],
  },
  {
    route: "$owner.settings.labels.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Org-wide labels settings deferred behind repo issue labels",
      },
    ],
  },

  // --- explore / notifications / global search (Phases 17 / 21) ---
  {
    route: "search.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/search.integration.test.ts",
      },
    ],
  },
  {
    route: "explore.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/explore.integration.test.ts",
      },
    ],
  },
  {
    route: "notifications.tsrx",
    coverage: [
      {
        kind: "happy-dom",
        test: "apps/web/src/routes/notifications.integration.test.ts",
      },
    ],
  },
];
