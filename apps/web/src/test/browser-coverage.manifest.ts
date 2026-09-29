/**
 * Browser (Chromium component) coverage manifest — follow-up to #41–#43 / PR #54.
 *
 * Every high-risk interactive `.tsrx` under `apps/web/src` (Checkbox, RadioGroup,
 * or `form.Subscribe`) must appear here with proof that a real-DOM render test
 * (or stack-browser click-through) covers it. Happy-dom alone does not count.
 *
 * Paths are repo-relative from the Oxidean root. Surface paths are relative to
 * `apps/web/src/`.
 *
 * Adding new high-risk UI:
 * 1. Author the `.tsrx`.
 * 2. Add `*.browser.test.tsx` that mounts + interacts + asserts no overlay/DOM race
 *    (or attribute an existing stack-browser suite that exercises the control).
 * 3. Append a row here; run `make browser-coverage-check`.
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
      /** Optional marker that must appear in the stack-browser suite source. */
      subject?: string;
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

const AUTH_UI = "apps/web/e2e/stack-browser/auth-ui.stack.browser.test.tsx";
const FORGE_ADMIN = "apps/web/e2e/stack-browser/forge-admin.stack.browser.test.tsx";
const FORGE_SSH = "apps/web/e2e/stack-browser/forge-packages-ssh-orgs.stack.browser.test.tsx";
const FORGE_MIRROR = "apps/web/e2e/stack-browser/forge-mirror.stack.browser.test.tsx";
const NEW_REPO = "apps/web/e2e/stack-browser/new-repo-template.stack.browser.test.tsx";
const FORGE_ISSUES = "apps/web/e2e/stack-browser/forge-issues-releases.stack.browser.test.tsx";
const PAT_STACK = "apps/web/e2e/stack-browser/pat-mint.stack.browser.test.tsx";

export const browserCoverageManifest: BrowserCoverageEntry[] = [
  // --- primitives (keepMounted Indicators) ---
  {
    surface: "components/ui/checkbox.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-controls.browser-harness" }],
  },
  {
    surface: "components/ui/radio-group.tsrx",
    coverage: [{ kind: "browser", test: UI_BROWSER, subject: "ui-controls.browser-harness" }],
  },

  // --- PAT mint (motivating crash class) ---
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
          "Subscribe wraps submit only (no Checkbox/Radio); stack-browser SSH/settings chrome covers adjacent settings. Promote to *.browser.test.tsx if GPG gains toggles.",
      },
    ],
  },

  // --- repo settings ---
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
        kind: "skip",
        rationale:
          "Checkbox/events live behind repo settings webhooks panel; no dedicated Chromium mount yet. Add *.browser.test.tsx when webhook UI is next touched.",
      },
    ],
  },

  // --- routes with Checkbox / Radio / Subscribe ---
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
        kind: "skip",
        rationale:
          "Checkbox is secondary; happy-dom mount + auth stack covers verify. Promote to browser mount if verify gains Base UI Indicator toggles beside panels.",
      },
    ],
  },
  {
    surface: "routes/setup.index.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Setup wizard Radio/Checkbox covered by happy-dom setup.integration; full Chromium click-through not yet required. Add *.browser.test.tsx when setup UI is next changed.",
      },
    ],
  },
  {
    surface: "routes/setup.credentials.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Credentials step Subscribe/checkbox covered by happy-dom setup.credentials.integration; promote on next edit.",
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
    surface: "routes/$owner.settings.index.tsrx",
    coverage: [
      {
        kind: "skip",
        rationale:
          "Org general settings Select/visibility; stack-browser org settings covers chrome. Add browser mount when org visibility radios are next touched.",
      },
    ],
  },
  {
    surface: "routes/$owner.$repo.releases.new.tsrx",
    coverage: [{ kind: "stack-browser", test: FORGE_ISSUES, subject: "release" }],
  },
];
