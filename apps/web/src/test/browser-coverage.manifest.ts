/**
 * Browser (Chromium component) coverage manifest — follow-up to #41–#43 / PR #54.
 *
 * Every high-risk interactive `.tsrx` under `apps/web/src` (Checkbox, RadioGroup,
 * form.Subscribe, Select*, Switch, Dialog/AlertDialog/DropdownMenu portals) must
 * appear here with proof that a real-DOM render test (or stack-browser
 * click-through) covers it. Happy-dom alone does not count.
 *
 * Paths are repo-relative from the Oxidean root. Surface paths are relative to
 * `apps/web/src/`.
 *
 * Adding or changing high-risk UI:
 * 1. Author the `.tsrx`.
 * 2. Add `*.browser.test.tsx` that mounts + interacts + asserts no overlay/DOM race
 *    (or attribute an existing stack-browser suite that exercises the control).
 * 3. Append a row here; run `make browser-coverage-check` / `make browser-coverage-check-pr`.
 * 4. Do not leave new/changed surfaces as skip-only — CI change-aware mode rejects that.
 */

export type BrowserCoverageKind = "browser" | "stack-browser" | "skip";

export type BrowserCoverageEvidence =
  | {
      kind: "browser";
      /** Repo-relative `*.browser.test.tsx` path */
      test: string;
      /**
       * Marker that must appear in the test source (import path fragment or
       * export name) proving this surface is the mount subject.
       */
      subject: string;
    }
  | {
      kind: "stack-browser";
      test: string;
      /** Marker that must appear in the stack-browser suite source. */
      subject: string;
    }
  | { kind: "skip"; rationale: string };

export type BrowserCoverageEntry = {
  /** Path relative to `apps/web/src/` */
  surface: string;
  coverage: BrowserCoverageEvidence[];
};

const PAT_BROWSER = "apps/web/src/components/settings/pat-mint.browser.test.tsx";
const UI_BROWSER = "apps/web/src/components/ui/ui-controls.browser.test.tsx";
const SSH_BROWSER = "apps/web/src/components/settings/ssh-key-add-form.browser.test.tsx";
const MIRROR_BROWSER = "apps/web/src/components/repo/mirror-settings-panel.browser.test.tsx";
const WEBHOOK_FORM_BROWSER = "apps/web/src/components/repo/webhook-form.browser.test.tsx";

const AUTH_UI = "apps/web/e2e/stack-browser/auth-ui.stack.browser.test.tsx";
const FORGE_ADMIN = "apps/web/e2e/stack-browser/forge-admin.stack.browser.test.tsx";
const FORGE_SSH = "apps/web/e2e/stack-browser/forge-packages-ssh-orgs.stack.browser.test.tsx";
const FORGE_MIRROR = "apps/web/e2e/stack-browser/forge-mirror.stack.browser.test.tsx";
const NEW_REPO = "apps/web/e2e/stack-browser/new-repo-template.stack.browser.test.tsx";
const FORGE_ISSUES = "apps/web/e2e/stack-browser/forge-issues-releases.stack.browser.test.tsx";
const FORGE_BRANCHES = "apps/web/e2e/stack-browser/forge-branches.stack.browser.test.tsx";
const CHROME_MENUS = "apps/web/e2e/stack-browser/chrome-menus.stack.browser.test.tsx";
const PAT_STACK = "apps/web/e2e/stack-browser/pat-mint.stack.browser.test.tsx";
const SETTINGS_AVATAR = "apps/web/e2e/stack-browser/settings-profile-avatar.stack.browser.test.tsx";

export const browserCoverageManifest: BrowserCoverageEntry[] = [
  // --- primitives ---
  {
    surface: "components/ui/checkbox.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-controls.browser-harness" }],
  },
  {
    surface: "components/ui/radio-group.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-controls.browser-harness" }],
  },
  {
    surface: "components/ui/select.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-harness-select" }],
  },
  {
    surface: "components/ui/switch.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-harness-switch" }],
  },
  {
    surface: "components/ui/dialog.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_BRANCHES, subject: "branch" }],
  },
  {
    surface: "components/ui/alert-dialog.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_BRANCHES, subject: "delete" }],
  },
  {
    surface: "components/ui/dropdown-menu.tsrx",
    coverage: [{ kind: "stack-browser", test: CHROME_MENUS, subject: "menu" }],
  },

  // --- PAT mint ---
  {
    surface: "components/settings/pat-classic-form.tsrx",
    coverage: [
      { kind: "browser", test: PAT_BROWSER, subject: "pat-classic-form" },
      { kind: "stack-browser", test: PAT_STACK, subject: "expectPatMintClickThroughFlow" },
    ],
  },
  {
    surface: "components/settings/pat-fg-form.tsrx",
    coverage: [
      { kind: "browser", test: PAT_BROWSER, subject: "pat-fg-form" },
      { kind: "stack-browser", test: PAT_STACK, subject: "expectPatMintClickThroughFlow" },
    ],
  },
  {
    surface: "components/settings/pat-fg-repo-picker.tsrx",
    coverage: [{ kind: "browser", test: PAT_BROWSER, subject: "fg-repo-item" }],
  },
  {
    surface: "components/settings/pat-expiry-field.tsrx",
    coverage: [
      { kind: "browser", test: PAT_BROWSER, subject: "pat-expiry-option" },
      { kind: "stack-browser", test: PAT_STACK, subject: "expectPatMintClickThroughFlow" },
    ],
  },

  // --- settings forms ---
  {
    surface: "components/settings/ssh-key-add-form.tsrx",
    coverage: [
      { kind: "browser", test: SSH_BROWSER, subject: "ssh-key-add-form" },
      { kind: "stack-browser", test: FORGE_SSH, subject: "SSH" },
    ],
  },
  {
    surface: "components/settings/gpg-key-add-form.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Subscribe wraps submit only; stack-browser SSH/settings chrome covers adjacent settings. Promote to *.browser.test.tsx if GPG gains toggles.",
      },
    ],
  },

  // --- repo / chrome ---
  {
    surface: "components/repo/mirror-settings-panel.tsrx",
    coverage: [
      { kind: "browser", test: MIRROR_BROWSER, subject: "mirror-settings-panel" },
      { kind: "stack-browser", test: FORGE_MIRROR, subject: "expectMirrorAuthToggleFlow" },
    ],
  },
  {
    surface: "components/repo/webhook-form.tsrx",
    coverage: [
      {
        kind: "browser",
        test: WEBHOOK_FORM_BROWSER,
        subject: "webhook-form",
      },
    ],
  },
  {
    surface: "components/repo/webhooks-panel.tsrx",
    coverage: [
      {
        kind: "browser",
        test: WEBHOOK_FORM_BROWSER,
        subject: "webhooks-panel",
      },
    ],
  },
  {
    surface: "components/repo/collaborators-panel.tsrx",
    coverage: [
      {
        kind: "browser",
        test: "apps/web/src/components/repo/collaborators-panel.browser.test.tsx",
        subject: "collaborators-panel",
      },
    ],
  },
  {
    surface: "components/repo/ref-select.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Branch/tag Select used across forge chrome; covered indirectly by forge-repo/branches flows. Dedicated browser Select pick when next edited.",
      },
    ],
  },
  {
    surface: "components/repo/lfs-settings-panel.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Switch/settings panel; promote to *.browser.test.tsx on next LFS settings edit.",
      },
    ],
  },
  {
    surface: "components/repo/actions-settings-panel.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Actions enable Switch; promote on next actions-settings edit.",
      },
    ],
  },
  {
    surface: "components/repo/template-repo-settings-panel.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale: "Template Switch panel; promote on next template-settings edit.",
      },
    ],
  },
  {
    surface: "components/repo/clone-box.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Clone URL Select; forge-repo stack-browser covers repo home chrome. Dedicated Select pick when clone-box is next touched.",
      },
    ],
  },
  {
    surface: "components/admin/byte-quota-field.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Admin quota Select; forge-admin opens /admin/packages chrome. Add browser Select pick when quota field is next edited.",
      },
    ],
  },
  {
    surface: "components/chrome.tsrx",
    coverage: [{ kind: "stack-browser", test: CHROME_MENUS, subject: "chrome" }],
  },

  // --- routes ---
  {
    surface: "routes/login.tsrx",
    coverage: [{ kind: "stack-browser", test: AUTH_UI, subject: "login" }],
  },
  {
    surface: "routes/signup.tsrx",
    coverage: [{ kind: "stack-browser", test: AUTH_UI, subject: "signup" }],
  },
  {
    surface: "routes/verify.tsrx",
    coverage: [
      {
        kind: "browser",
        test: "apps/web/src/routes/verify.browser.test.tsx",
        subject: "VerifyPage",
      },
    ],
  },
  {
    surface: "routes/setup.index.tsrx",
    coverage: [
      {
        kind: "browser",
        test: "apps/web/src/routes/setup.index.browser.test.tsx",
        subject: "SetupPage",
      },
    ],
  },
  {
    surface: "routes/setup.credentials.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Credentials step covered by happy-dom setup.credentials.integration; promote on next edit.",
      },
    ],
  },
  {
    surface: "routes/new.tsrx",
    coverage: [{ kind: "stack-browser", test: NEW_REPO, subject: "template" }],
  },
  {
    surface: "routes/admin/auth.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_ADMIN, subject: "admin/auth" }],
  },
  {
    surface: "routes/admin/users.tsrx",
    coverage: [
      {
        kind: "browser",
        test: "apps/web/src/routes/admin/users.browser.test.tsx",
        subject: "admin-users-page",
      },
    ],
  },
  {
    surface: "routes/admin/templates.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Admin templates Switch/Select; forge-admin does not open /admin/templates yet. Add browser or stack-browser on next templates edit.",
      },
    ],
  },
  {
    surface: "routes/$owner.settings.index.tsrx",
    coverage: [
      {
        kind: "browser",
        test: "apps/web/src/routes/$owner.settings.index.browser.test.tsx",
        subject: "OrgSettingsPage",
      },
    ],
  },
  {
    surface: "routes/$owner.settings.members.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_SSH, subject: "members" }],
  },
  {
    surface: "routes/$owner.$repo.releases.new.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_ISSUES, subject: "release" }],
  },
  {
    surface: "routes/settings/tokens.index.tsrx",
    coverage: [{ kind: "stack-browser", test: SETTINGS_AVATAR, subject: "tokens" }],
  },
];
