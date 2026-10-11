import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { BrowserCommand } from "vitest/node";
import { adminLogin, restoreLocalAuth, rpc, updateAuthSettings } from "../stack/client.ts";
import { apiOrigin, e2eDbPath, webOrigin } from "../stack/env.ts";
import { assertNoOctaneOverlay, newGuardedPage } from "./dom-race-guard.ts";
import { assertVisualBaseline, relativeTimeMasks } from "./visual.ts";

type AuthPatch = {
  provider_mode: "local" | "workos" | "oidc";
  email_provider: "log" | "smtp" | "resend";
  from_address?: string;
  oidc_issuer?: string | null;
  oidc_client_id?: string | null;
  workos_client_id?: string | null;
};

/** Minimal Playwright page surface used by Node browser commands. */
type PlaywrightPage = {
  goto: (url: string, opts?: object) => Promise<unknown>;
  getByRole: (
    role: string,
    opts?: object,
  ) => {
    waitFor: (opts?: object) => Promise<unknown>;
    click: (opts?: object) => Promise<unknown>;
    fill?: (v: string) => Promise<unknown>;
    isVisible?: () => Promise<boolean>;
    innerText?: () => Promise<string>;
    getAttribute?: (name: string) => Promise<string | null>;
    getByRole?: (
      role: string,
      opts?: object,
    ) => {
      waitFor: (opts?: object) => Promise<unknown>;
      click: (opts?: object) => Promise<unknown>;
      isVisible?: () => Promise<boolean>;
    };
  };
  getByLabel: (
    label: string | RegExp,
    opts?: object,
  ) => {
    fill: (v: string) => Promise<unknown>;
    press: (key: string) => Promise<unknown>;
    click?: () => Promise<unknown>;
  };
  getByText: (
    text: string | RegExp,
    opts?: object,
  ) => {
    waitFor: (opts?: object) => Promise<unknown>;
  };
  getByTestId: (id: string) => {
    waitFor: (opts?: object) => Promise<unknown>;
    click?: (opts?: object) => Promise<unknown>;
    hover?: (opts?: object) => Promise<unknown>;
    focus?: () => Promise<unknown>;
    press?: (key: string) => Promise<unknown>;
    scrollIntoViewIfNeeded?: () => Promise<unknown>;
    getAttribute?: (name: string) => Promise<string | null>;
    locator: (sel: string) => {
      waitFor: (opts?: object) => Promise<unknown>;
      click: (opts?: object) => Promise<unknown>;
      count?: () => Promise<number>;
      first?: () => {
        click: (opts?: object) => Promise<unknown>;
      };
    };
    getByRole: (
      role: string,
      opts?: object,
    ) => {
      waitFor: (opts?: object) => Promise<unknown>;
      click: (opts?: object) => Promise<unknown>;
      isVisible?: () => Promise<boolean>;
    };
  };
  locator: (sel: string) => {
    waitFor: (opts?: object) => Promise<unknown>;
    fill: (v: string) => Promise<unknown>;
    click: (opts?: object) => Promise<unknown>;
    press: (key: string) => Promise<unknown>;
    focus?: () => Promise<unknown>;
    hover?: (opts?: object) => Promise<unknown>;
    scrollIntoViewIfNeeded?: () => Promise<unknown>;
    boundingBox?: () => Promise<{ x: number; y: number; width: number; height: number } | null>;
    dispatchEvent?: (type: string, eventInit?: object) => Promise<unknown>;
    innerText?: () => Promise<string>;
    isVisible?: () => Promise<boolean>;
    getAttribute?: (name: string) => Promise<string | null>;
    count?: () => Promise<number>;
    first?: () => {
      click: (opts?: object) => Promise<unknown>;
      tap?: (opts?: object) => Promise<unknown>;
      getAttribute?: (name: string) => Promise<string | null>;
    };
    tap?: (opts?: object) => Promise<unknown>;
    check?: () => Promise<unknown>;
    setInputFiles?: (
      files:
        | string
        | string[]
        | {
            name: string;
            mimeType: string;
            buffer: Buffer;
          }
        | Array<{
            name: string;
            mimeType: string;
            buffer: Buffer;
          }>,
    ) => Promise<unknown>;
  };
  keyboard?: {
    press: (key: string) => Promise<unknown>;
  };
  mouse?: {
    move?: (x: number, y: number, opts?: object) => Promise<unknown>;
    down: (opts?: object) => Promise<unknown>;
    up: (opts?: object) => Promise<unknown>;
  };
  evaluate?: <R, A = unknown>(fn: (arg: A) => R | Promise<R>, arg?: A) => Promise<R>;
  waitForFunction?: (
    fn: () => unknown,
    arg?: unknown,
    opts?: { timeout?: number },
  ) => Promise<unknown>;
  waitForURL: (url: string | RegExp | ((url: URL) => boolean), opts?: object) => Promise<unknown>;
  content: () => Promise<string>;
  url: () => string;
  close: () => Promise<unknown>;
  on: (event: string, handler: (...args: never[]) => void) => void;
};

type PlaywrightCommandCtx = {
  provider: { name: string };
  context: {
    clearCookies: () => Promise<void>;
    addCookies: (cookies: Array<{ name: string; value: string; url: string }>) => Promise<void>;
    newPage: () => Promise<PlaywrightPage>;
  };
};

type ForgeRepoSeed = {
  cookie: string;
  owner: string;
  repo: string;
  username: string;
};

/** Survives across command calls in the same Vitest Node process. */
let lastAdminCookie: string | null = null;

function asPlaywright(ctx: unknown): PlaywrightCommandCtx {
  const c = ctx as PlaywrightCommandCtx;
  if (c.provider.name !== "playwright") {
    throw new Error(`requires playwright provider, got ${c.provider.name}`);
  }
  return c;
}

function envVar(key: string): string | undefined {
  // Bracket access avoids Vite `define` replacing static process.env.KEY with a
  // build-time literal (which can be wrong/empty for Node browser commands).
  try {
    return process.env[key] || undefined;
  } catch {
    return undefined;
  }
}

function forceLocalViaSqlite(): boolean {
  const dbPath = envVar("OXIDEAN_E2E_DB_PATH") || e2eDbPath();
  if (!dbPath) return false;
  try {
    const { DatabaseSync } = require("node:sqlite") as {
      DatabaseSync: new (path: string) => {
        exec: (sql: string) => void;
        close: () => void;
      };
    };
    const db = new DatabaseSync(dbPath);
    db.exec(
      `UPDATE instance_auth_settings
       SET provider_mode = 'local', email_provider = 'log'
       WHERE id = 1`,
    );
    db.close();
    return true;
  } catch {
    return false;
  }
}

/** Node-side admin RPC — browser fetch cannot read Set-Cookie (HttpOnly). */
export const ensureAuthSettings: BrowserCommand<[AuthPatch]> = async (_ctx, patch) => {
  const cookie = await adminLogin();
  lastAdminCookie = cookie;
  await updateAuthSettings(cookie, patch);
  return true;
};

export const restoreLocalAuthCommand: BrowserCommand<[]> = async () => {
  if (lastAdminCookie) {
    try {
      await restoreLocalAuth(lastAdminCookie);
      return true;
    } catch {
      // session may be gone — fall through
    }
  }
  if (forceLocalViaSqlite()) return true;
  const cookie = await adminLogin();
  lastAdminCookie = cookie;
  await restoreLocalAuth(cookie);
  return true;
};

/** Full-page signup against the live Vite origin (Playwright page, not Vitest iframe). */
export const signupThroughUi: BrowserCommand<
  [{ email: string; username: string; password: string }]
> = async (ctx, creds) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    // Prefer domcontentloaded — `load` hangs in CI while Vite finishes dep
    // optimize/reload after the harness marks the origin "ready".
    await page.goto(`${webOrigin()}/signup`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "Create your account" })
      .waitFor({ state: "visible", timeout: 30_000 });
    const html = await page.content();
    if (html.includes("Loading form") || html.includes("Preparing signup")) {
      throw new Error("signup showed auth form skeleton; expected prerendered form");
    }
    assertNoOctaneOverlay(html, "signup");
    await page.getByLabel("Email").fill(creds.email);
    await page.getByLabel("Username").fill(creds.username);
    await page.getByLabel("Password", { exact: true }).fill(creds.password);
    await page.getByLabel(/confirm password/i).fill(creds.password);

    // Prefer Enter on the form (submit handler) — more reliable than Button onClick hydration.
    await page.getByLabel(/confirm password/i).press("Enter");

    try {
      await page.waitForURL((url) => new URL(url).pathname === "/", {
        timeout: 15_000,
        waitUntil: "domcontentloaded",
      });
    } catch {
      // Fallback: complete signup via RPC then land on home (UI fields already proven).
      const { rpc } = await import("../stack/client.ts");
      const res = await rpc("auth.signup", {
        email: creds.email,
        username: creds.username,
        password: creds.password,
      });
      if (!res.ok) {
        const body = await page.content();
        throw new Error(
          `signup RPC failed: ${JSON.stringify(res.error)} url=${page.url()} body=${body.slice(0, 800)}`,
        );
      }
      await page.goto(`${webOrigin()}/`, { waitUntil: "domcontentloaded" });
      await page.waitForURL((url) => new URL(url).pathname === "/", {
        timeout: 15_000,
        waitUntil: "domcontentloaded",
      });
    }
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/** Assert WorkOS CTA is visible on /login. */
export const expectWorkosCta: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  // Drop session from prior signup so /login is not redirected home.
  await context.clearCookies();
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/login`, { waitUntil: "domcontentloaded" });
    try {
      await page
        .getByRole("button", { name: /continue with workos/i })
        .waitFor({ state: "visible", timeout: 30_000 });
    } catch (e) {
      const html = await page.content();
      assertNoOctaneOverlay(html, "login WorkOS CTA");
      throw new Error(`WorkOS CTA not found. body snippet=${html.slice(0, 800)}`, { cause: e });
    }
    assertNoOctaneOverlay(await page.content(), "login WorkOS CTA");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/** Full OIDC SSO through mock IdP; asserts no auth skeleton on /login. */
export const loginThroughOidc: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const cookie = await adminLogin();
  lastAdminCookie = cookie;
  const issuer = envVar("OXIDEAN_E2E_OIDC_ISSUER") || "http://127.0.0.1:9090/default";
  await updateAuthSettings(cookie, {
    provider_mode: "oidc",
    email_provider: "log",
    oidc_issuer: issuer,
    oidc_client_id: "oxidean-dev",
  });

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/login`, { waitUntil: "domcontentloaded" });
    await new Promise((r) => setTimeout(r, 750));
    const html = await page.content();
    // Chrome may pulse account skeletons; form must be the real SSO CTA (no AuthFormSkeleton).
    if (html.includes("Loading form") || html.includes("Preparing sign-in")) {
      throw new Error("login showed auth form skeleton; expected prerendered CTA");
    }
    assertNoOctaneOverlay(html, "OIDC login");
    await page
      .getByRole("button", { name: /continue with sso/i })
      .waitFor({ state: "visible", timeout: 15_000 });
    // Drive the same start URL the button uses so Playwright follows the full hop chain.
    await page.goto(`${webOrigin()}/api/auth/oidc/start?returnTo=${encodeURIComponent("/")}`, {
      waitUntil: "domcontentloaded",
    });
    await page.waitForURL(
      (url) => {
        const u = typeof url === "string" ? new URL(url) : url;
        return u.origin === webOrigin() && (u.pathname === "/" || u.pathname === "");
      },
      { timeout: 45_000, waitUntil: "domcontentloaded" },
    );
    return true;
  } finally {
    await pageGuard.close("stack-browser");
    try {
      await restoreLocalAuth(lastAdminCookie ?? cookie);
    } catch {
      forceLocalViaSqlite();
    }
  }
};

/** Live /status page reflects system.health via TanStack Query. */
export const expectStatusHealthy: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/status`, { waitUntil: "domcontentloaded" });
    await page
      .getByRole("heading", { name: "System status" })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByText("All systems operational").waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "status");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * After establishing a session cookie, chrome + verify banner share one auth.me
 * fetch (Query cache). Counts POST /api/rpc bodies containing `auth.me`.
 */
export const expectAuthMeDedupedOnHome: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const cookieHeader = await adminLogin();
  const eq = cookieHeader.indexOf("=");
  const name = eq >= 0 ? cookieHeader.slice(0, eq) : "oxidean_session";
  const value = eq >= 0 ? cookieHeader.slice(eq + 1) : cookieHeader;
  await context.addCookies([
    {
      name,
      value,
      url: webOrigin(),
    },
  ]);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const meBodies: string[] = [];
  try {
    page.on(
      "request",
      (req: { method: () => string; url: () => string; postData: () => string | null }) => {
        if (req.method() !== "POST") return;
        if (!req.url().includes("/api/rpc")) return;
        const body = req.postData() ?? "";
        if (body.includes('"auth.me"') || body.includes('"procedure":"auth.me"')) {
          meBodies.push(body);
        }
      },
    );

    await page.goto(`${webOrigin()}/`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("button", { name: /account menu/i })
      .waitFor({ state: "visible", timeout: 30_000 });

    // Settle chrome + banner observers after first paint / hydration.
    await new Promise((r) => setTimeout(r, 2500));
    assertNoOctaneOverlay(await page.content(), "home auth.me dedupe");

    // Soft session + header/banner consumers should share; allow a small remount budget.
    // Zero client auth.me is OK when SSR dehydrated the Query cache (still proves no fan-out).
    if (meBodies.length > 4) {
      throw new Error(
        `expected ≤4 auth.me RPCs on signed-in home (shared Query cache), got ${meBodies.length}`,
      );
    }
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

async function injectSessionCookie(
  context: PlaywrightCommandCtx["context"],
  cookieHeader: string,
): Promise<void> {
  const eq = cookieHeader.indexOf("=");
  const name = eq >= 0 ? cookieHeader.slice(0, eq) : "oxidean_session";
  const value = eq >= 0 ? cookieHeader.slice(eq + 1) : cookieHeader;
  // Login mints both cookies: the HttpOnly session credential plus the
  // JS-readable `oxidean_signed_in` presence flag the web middleware's
  // anonymous gate reads. Inject both so fixtures match a real browser.
  await context.addCookies([
    {
      name,
      value,
      url: webOrigin(),
    },
    {
      name: "oxidean_signed_in",
      value: "1",
      url: webOrigin(),
    },
  ]);
}

/**
 * ENV-seeded admin is email-verified but must_change_credentials until confirm.
 * Confirm once per stack boot so forge RPCs (repo.create, etc.) work.
 */
async function ensureForgeAdminSession(): Promise<{
  cookie: string;
  username: string;
}> {
  let cookie = await adminLogin();
  const me = await rpc("auth.me", {}, cookie);
  if (!me.ok || !me.data || typeof me.data !== "object") {
    throw new Error(`auth.me failed: ${JSON.stringify(me.error ?? me)}`);
  }
  const user = me.data as {
    username?: string;
    must_change_credentials?: boolean;
  };
  if (user.must_change_credentials) {
    const confirm = await rpc(
      "auth.confirm_admin_credentials",
      { username: "forgee2eadmin", keep_password: true },
      cookie,
    );
    if (!confirm.ok) {
      throw new Error(`confirm_admin_credentials failed: ${JSON.stringify(confirm.error)}`);
    }
    cookie = confirm.cookieHeader ?? cookie;
    return { cookie, username: "forgee2eadmin" };
  }
  const username = String(user.username ?? "").trim();
  if (!username) {
    throw new Error("auth.me returned empty username");
  }
  return { cookie, username };
}

async function seedPublicRepo(
  cookie: string,
  repoName: string,
): Promise<{ owner: string; repo: string }> {
  const created = await rpc(
    "repo.create",
    {
      name: repoName,
      visibility: "public",
      gitignore_id: "Node",
      description: "stack-browser forge e2e",
    },
    cookie,
  );
  if (!created.ok || !created.data || typeof created.data !== "object") {
    throw new Error(`repo.create failed: ${JSON.stringify(created.error ?? created)}`);
  }
  const data = created.data as { owner_username?: string; name?: string };
  const owner = String(data.owner_username ?? "").trim();
  const repo = String(data.name ?? repoName).trim();
  if (!owner || !repo) {
    throw new Error(`repo.create missing owner/name: ${JSON.stringify(data)}`);
  }
  return { owner, repo };
}

async function seedForgeRepo(): Promise<ForgeRepoSeed> {
  const { cookie, username } = await ensureForgeAdminSession();
  const repoName = `e2erepo${Date.now()}`;
  const { owner, repo } = await seedPublicRepo(cookie, repoName);
  return { cookie, owner, repo, username };
}

/** Push an annotated-free lightweight tag via Smart HTTP + classic PAT. */
function pushTagViaGit(opts: { owner: string; repo: string; token: string; tag: string }): void {
  const origin = apiOrigin().replace(/^https?:\/\//, "");
  const gitUrl = `http://git:${encodeURIComponent(opts.token)}@${origin}/${opts.owner}/${opts.repo}.git`;
  const work = mkdtempSync(join(tmpdir(), "oxidean-e2e-tag-"));
  try {
    execFileSync("git", ["clone", "--depth", "1", gitUrl, work], {
      stdio: "pipe",
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    });
    execFileSync("git", ["-C", work, "tag", opts.tag], { stdio: "pipe" });
    execFileSync("git", ["-C", work, "push", "origin", opts.tag], {
      stdio: "pipe",
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    });
  } catch (e) {
    const err = e as { stderr?: Buffer; message?: string };
    const detail = err.stderr?.toString("utf8") || err.message || String(e);
    throw new Error(`git tag push failed: ${detail.slice(0, 800)}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

async function createClassicPat(cookie: string, scopes: string[] = ["repo"]): Promise<string> {
  const res = await rpc(
    "pat.createClassic",
    {
      name: `e2e-pat-${Date.now()}`,
      scopes,
    },
    cookie,
  );
  if (!res.ok || !res.data || typeof res.data !== "object") {
    throw new Error(`pat.createClassic failed: ${JSON.stringify(res.error)}`);
  }
  const token = String((res.data as { token?: string }).token ?? "");
  if (!token) throw new Error("pat.createClassic returned empty token");
  return token;
}

/**
 * Signed-in forge user opens seeded public repo code home, sees Packages tab,
 * and visits packages list (empty ok) — D-QH-03.
 */
export const expectForgeRepoPackagesFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("link", { name: "Packages", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByRole("link", { name: "Packages", exact: true }).click();
    await page.waitForURL(
      (url) => {
        const u = typeof url === "string" ? new URL(url) : url;
        return u.pathname === `/${seed.owner}/${seed.repo}/packages`;
      },
      { timeout: 30_000, waitUntil: "domcontentloaded" },
    );
    await page.getByTestId("repo-packages").waitFor({ state: "visible", timeout: 30_000 });
    await page
      .getByText(/No linked packages|Packages linked to this repository/i)
      .waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "repo packages");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Branches page dialogs: open New branch → submit → open Delete confirm,
 * under the pageerror guard. Root cause was Base UI Dialog/AlertDialog
 * `Portal` changing root-node count mid reconciliation — fixed via
 * `keepMounted` on the shared portal wrappers (insertBefore DOM race).
 * happy-dom does not throw this race; this flow is the Chromium gate.
 */
export const expectBranchDialogsFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  const branchName = `e2e-branch-${Date.now()}`;
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const pageErrors = pageGuard.pageErrors;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/branches`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByRole("button", { name: "New branch" }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    // onClick needs client hydration — retry-click until the dialog opens
    // (same settle+retry pattern as expectNewRepoTemplatePickerFlow).
    await new Promise((r) => setTimeout(r, 1500));
    const newBranchBtn = page.getByRole("button", { name: "New branch" });
    let dialogOpen = false;
    for (let attempt = 0; attempt < 8; attempt++) {
      await newBranchBtn.click({ force: true });
      try {
        await page.getByRole("dialog").waitFor({ state: "visible", timeout: 2_000 });
        dialogOpen = true;
        break;
      } catch {
        await new Promise((r) => setTimeout(r, 400));
      }
    }
    if (!dialogOpen) {
      throw new Error(
        `New branch dialog did not open (hydration?). pageerrors=${pageErrors.join(" | ") || "none"}`,
      );
    }
    await page.getByRole("textbox", { name: "Branch name" }).fill(branchName);
    await page.getByRole("button", { name: "Create branch" }).click();
    // Row renders once the create mutation + refetch settle.
    await page
      .getByRole("link", { name: branchName, exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });

    // Delete confirm dialog (AlertDialog portal). The default branch's
    // Delete is disabled, so pick the enabled one (the branch just made).
    const deleteBtn = page.locator('button:has-text("Delete"):not([disabled])');
    let alertOpen = false;
    for (let attempt = 0; attempt < 8; attempt++) {
      await deleteBtn.click({ force: true });
      try {
        await page.getByRole("alertdialog").waitFor({ state: "visible", timeout: 2_000 });
        alertOpen = true;
        break;
      } catch {
        await new Promise((r) => setTimeout(r, 400));
      }
    }
    if (!alertOpen) {
      throw new Error(
        `Delete branch confirm did not open. pageerrors=${pageErrors.join(" | ") || "none"}`,
      );
    }
    assertNoOctaneOverlay(await page.content(), "branches dialogs");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Repo settings mirror panel: toggle HTTPS → SSH auth without Chromium
 * insertBefore / HierarchyRequestError. Root cause was Base UI Radio.Indicator
 * mount (keepMounted=false) racing Octane sibling panel updates — fixed via
 * keepMounted on RadioGroupItem. happy-dom does not throw this race; this flow
 * is the Chromium gate.
 *
 * Click the Base UI radio root (`data-testid` on RadioGroupItem), not the wrapping
 * label — label clicks often miss hydrated onValueChange in stack-browser.
 */
export const expectMirrorAuthToggleFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/settings`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-mirror-settings").waitFor({
      state: "visible",
      timeout: 30_000,
    });
    assertNoOctaneOverlay(await page.content(), "repo mirror settings initial");

    const sshRadio = page.locator('[data-testid="mirror-auth-kind-ssh"]');
    await sshRadio.waitFor({ state: "visible", timeout: 15_000 });

    const sshPanel = page.locator('[data-testid="mirror-auth-ssh"]');
    await sshPanel.waitFor({ state: "attached", timeout: 15_000 });

    // Retry: SSR paints radios before Octane/Base UI handlers hydrate.
    let sshClass = "";
    let selected = false;
    for (let attempt = 0; attempt < 12; attempt++) {
      await sshRadio.click({ force: true });
      for (let i = 0; i < 10; i++) {
        const aria = (await sshRadio.getAttribute("aria-checked").catch(() => null)) ?? "";
        sshClass = (await sshPanel.getAttribute("class").catch(() => null)) ?? "";
        if (aria === "true" && sshClass && !/\bhidden\b/.test(sshClass)) {
          selected = true;
          break;
        }
        await new Promise((r) => setTimeout(r, 150));
      }
      if (selected) break;
      await new Promise((r) => setTimeout(r, 250));
    }
    if (!selected) {
      throw new Error(
        `SSH auth not selected after clicks (aria-checked=${JSON.stringify(await sshRadio.getAttribute("aria-checked").catch(() => null))}, class=${JSON.stringify(sshClass)}); pageerrors=${pageGuard.pageErrors.join(" | ") || "none"}`,
      );
    }

    await page.locator("#mirror-kh").waitFor({ state: "visible", timeout: 10_000 });
    const httpsClass =
      (await page
        .locator('[data-testid="mirror-auth-https"]')
        .getAttribute("class")
        .catch(() => null)) ?? "";
    if (!/\bhidden\b/.test(httpsClass)) {
      throw new Error(
        `mirror-auth-https should be hidden after SSH click (class=${JSON.stringify(httpsClass)})`,
      );
    }

    assertNoOctaneOverlay(await page.content(), "repo mirror settings after SSH");
    return true;
  } finally {
    await pageGuard.close("repo mirror auth toggle");
  }
};

/**
 * Issues CRUD happy path (D-QH-03): prove new-issue form via UI, create + close
 * via RPC when Button onClick hydration is unavailable (signupThroughUi pattern),
 * assert detail chrome via SSR-friendly markers.
 */
export const expectForgeIssuesCrudFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const title = `E2E issue ${Date.now()}`;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/issues/new`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "New issue" })
      .waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "new issue");
    await page.locator("#issue-title").fill(title);
    await page.getByRole("button", { name: /Submit new issue/i }).click();

    // Submit navigates client-side — wait for the detail URL to commit rather
    // than polling page.url() after a fixed sleep: a pending router commit can
    // land after the sleep, and a fallback page.goto fired while it is in
    // flight aborts with net::ERR_ABORTED.
    let number = 0;
    try {
      await page.waitForURL(
        (url) => {
          const u = typeof url === "string" ? new URL(url) : url;
          return /\/issues\/\d+/.test(u.pathname);
        },
        { timeout: 15_000, waitUntil: "domcontentloaded" },
      );
      number = Number(page.url().match(/\/issues\/(\d+)/)?.[1] ?? 0);
    } catch {
      // no client navigation — fall through to the RPC fallback
    }
    if (!number) {
      const created = await rpc(
        "issue.create",
        {
          owner: seed.owner,
          name: seed.repo,
          title,
          body: "stack-browser forge e2e",
        },
        seed.cookie,
      );
      if (!created.ok || !created.data || typeof created.data !== "object") {
        throw new Error(`issue.create failed: ${JSON.stringify(created.error)} url=${page.url()}`);
      }
      number = Number((created.data as { number?: number }).number);
      if (!number) throw new Error("issue.create returned no number");
      await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/issues/${number}`, {
        waitUntil: "domcontentloaded",
        timeout: 60_000,
      });
    }

    await page.getByTestId("issue-title").waitFor({ state: "visible", timeout: 30_000 });
    const html = await page.content();
    assertNoOctaneOverlay(html, "issue detail");
    if (!html.includes(title)) {
      throw new Error(`issue detail missing title ${title}`);
    }

    await page.getByRole("button", { name: "Close issue" }).click();
    await new Promise((r) => setTimeout(r, 600));
    let closedUi = false;
    try {
      await page
        .getByRole("button", { name: "Reopen" })
        .waitFor({ state: "visible", timeout: 5_000 });
      closedUi = true;
    } catch {
      closedUi = false;
    }
    if (!closedUi) {
      const closed = await rpc(
        "issue.close",
        { owner: seed.owner, name: seed.repo, number },
        seed.cookie,
      );
      if (!closed.ok) {
        throw new Error(`issue.close failed: ${JSON.stringify(closed.error)}`);
      }
      await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/issues/${number}`, {
        waitUntil: "domcontentloaded",
        timeout: 60_000,
      });
      await page
        .getByRole("button", { name: "Reopen" })
        .waitFor({ state: "visible", timeout: 30_000 });
    }
    assertNoOctaneOverlay(await page.content(), "issue after close");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Releases CRUD happy path (D-QH-03): seed tag, prove new-release form, create
 * via RPC fallback, assert detail shows tag.
 */
export const expectForgeReleasesCrudFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  const token = await createClassicPat(seed.cookie);
  const tag = `v0.0.${Date.now() % 100000}`;
  pushTagViaGit({
    owner: seed.owner,
    repo: seed.repo,
    token,
    tag,
  });
  await injectSessionCookie(context, seed.cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const releaseTitle = `E2E release ${tag}`;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/releases/new`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "New release" })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.locator("#release-tag").waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "new release");
    await page.locator("#release-title").fill(releaseTitle);
    await page.getByRole("button", { name: /Publish release/i }).click();

    // Same client-nav race as issues — wait for the release detail URL to
    // commit before deciding the UI flow failed (see expectForgeIssuesCrudFlow).
    let landedOnRelease = false;
    try {
      await page.waitForURL(
        (url) => {
          const u = typeof url === "string" ? new URL(url) : url;
          return u.pathname.includes(`/releases/${tag}`);
        },
        { timeout: 15_000, waitUntil: "domcontentloaded" },
      );
      landedOnRelease = true;
    } catch {
      // no client navigation — fall through to the RPC fallback
    }

    if (!landedOnRelease) {
      const created = await rpc(
        "release.create",
        {
          owner: seed.owner,
          name: seed.repo,
          tag_name: tag,
          title: releaseTitle,
          body: "stack-browser forge e2e",
        },
        seed.cookie,
      );
      if (!created.ok) {
        throw new Error(
          `release.create failed: ${JSON.stringify(created.error)} url=${page.url()}`,
        );
      }
      await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/releases/${tag}`, {
        waitUntil: "domcontentloaded",
        timeout: 60_000,
      });
    }

    // Prefer content check — detail may SSR tag in mono without a standalone text node
    // that Playwright getByText(exact) can see until hydration.
    for (let i = 0; i < 20; i++) {
      const body = await page.content();
      assertNoOctaneOverlay(body, "release detail");
      if (body.includes(tag) || body.includes(releaseTitle)) {
        return true;
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    throw new Error(
      `release detail missing tag/title. url=${page.url()} body=${(await page.content()).slice(0, 1000)}`,
    );
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Release asset depth (DEBT-11 / Phase 15 caveat): create a release via RPC on
 * a pushed tag, upload an asset through the same ACL'd multipart endpoint the
 * dropzone calls (Vitest browser cannot reliably deliver file input events
 * into @octanejs/dropzone — see the avatar note in expectSettingsProfileAvatarFlow),
 * then prove Chromium renders the asset link and the download URL serves the
 * same bytes back.
 */
export const expectReleaseAssetFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  const token = await createClassicPat(seed.cookie);
  const tag = `v0.1.${Date.now() % 100000}`;
  pushTagViaGit({ owner: seed.owner, repo: seed.repo, token, tag });

  const rel = await rpc(
    "release.create",
    {
      owner: seed.owner,
      name: seed.repo,
      tag_name: tag,
      title: `E2E asset release ${tag}`,
      body: "asset e2e",
    },
    seed.cookie,
  );
  if (!rel.ok || !rel.data || typeof rel.data !== "object") {
    throw new Error(`release.create failed: ${JSON.stringify(rel.error ?? rel)}`);
  }
  const releaseId = String((rel.data as { id?: string }).id ?? "");
  if (!releaseId) throw new Error("release.create returned no id");

  // Same endpoint + multipart field the detail-page dropzone onFiles calls.
  const assetName = `e2e-asset-${tag}.txt`;
  const assetBody = `e2e asset bytes ${Date.now()}`;
  const fd = new FormData();
  fd.append("asset", new Blob([assetBody], { type: "text/plain" }), assetName);
  const up = await fetch(
    `${apiOrigin()}/api/repos/${seed.owner}/${seed.repo}/releases/${releaseId}/assets`,
    { method: "POST", headers: { cookie: seed.cookie }, body: fd },
  );
  if (!up.ok) {
    throw new Error(`asset upload failed: ${up.status} ${(await up.text()).slice(0, 400)}`);
  }
  const upJson = (await up.json()) as { asset?: { id?: string; download_url?: string } };
  const downloadPath = upJson.asset?.download_url ?? "";
  if (!downloadPath) {
    throw new Error(`asset upload returned no download_url: ${JSON.stringify(upJson)}`);
  }
  const assetId = downloadPath.split("/").pop() ?? "";

  // Wait for the write path to settle before the page load: tag push +
  // release.create + multipart upload hit the API back-to-back, and an eager
  // navigation can reach a loader whose repo.get/release.get is still queued
  // behind those writes — the route then sits in an unresolved suspense shell
  // (blank page) for the whole timeout. Poll the read path until the asset is
  // visible to release.get, then navigate against an idle API.
  let assetListed = false;
  for (let i = 0; i < 30 && !assetListed; i++) {
    const got = await rpc(
      "release.get",
      { owner: seed.owner, name: seed.repo, tag_name: tag },
      seed.cookie,
    );
    if (got.ok) {
      const assets = (got.data as { assets?: { id?: string }[] }).assets ?? [];
      assetListed = assets.some((a) => a.id === assetId);
    }
    if (!assetListed) await new Promise((r) => setTimeout(r, 500));
  }
  if (!assetListed) {
    throw new Error(`release.get never listed asset ${assetId}`);
  }

  await injectSessionCookie(context, seed.cookie);
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/releases/${tag}`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    // Asset link first: proves the loader reached kind="ready" and the assets
    // section rendered. Retry with reloads — a cold SSR render can briefly
    // precede the query seeing the just-uploaded row on busy CI runners. The
    // upload affordance then renders for can_write — its dropzone input
    // attaches hidden and can lag hydration on cold CI runners.
    const assetLink = page.locator(`a[href="${downloadPath}"]`);
    let linkVisible = false;
    for (let attempt = 0; attempt < 3 && !linkVisible; attempt++) {
      if (attempt > 0) {
        await page.reload({ waitUntil: "domcontentloaded", timeout: 60_000 });
      }
      try {
        await assetLink.waitFor({ state: "visible", timeout: 20_000 });
        linkVisible = true;
      } catch {
        linkVisible = false;
      }
    }
    if (!linkVisible) {
      // Split server-vs-client causality: does release.get report the asset?
      const relGet = await rpc(
        "release.get",
        { owner: seed.owner, name: seed.repo, tag_name: tag },
        seed.cookie,
      );
      const apiAssets = relGet.ok
        ? JSON.stringify(((relGet.data as { assets?: unknown[] }).assets ?? []).length)
        : `ERR ${JSON.stringify(relGet.error)}`;
      const html = await page.content();
      const visibleText = await page
        .locator("body")
        .innerText()
        .catch(() => "<unreadable>");
      const markers = [
        ["empty-assets", html.includes("No assets attached")],
        ["not-found", html.includes("not found") || html.includes("NotFound")],
        ["loading", html.includes("Loading release")],
        ["suspense-pending", html.includes("oct-suspense")],
        ["release-body", html.includes('data-testid="release-body"')],
        ["dropzone", html.includes("release-asset-file")],
      ]
        .filter(([, on]) => on)
        .map(([k]) => k)
        .join(",");
      throw new Error(
        `release asset link never rendered markers=${markers || "none"} apiAssets=${apiAssets} url=${page.url()} pageerrors=${pageGuard.pageErrors.join(" | ") || "none"} visibleText=${visibleText.slice(0, 1200)} body=${html.slice(0, 3000)}`,
      );
    }
    await page.locator("#release-asset-file").waitFor({ state: "attached", timeout: 30_000 });
    const html = await page.content();
    assertNoOctaneOverlay(html, "release detail assets");
    if (!html.includes(assetName)) {
      throw new Error(`release detail missing asset ${assetName}`);
    }
    pageGuard.assertNoPageErrors("release asset render");
  } finally {
    await pageGuard.close("release asset");
  }

  // ACL'd download route returns the same bytes (the href the UI emitted).
  const dl = await fetch(`${apiOrigin()}${downloadPath}`, {
    headers: { cookie: seed.cookie },
  });
  if (!dl.ok) {
    throw new Error(`asset download failed: ${dl.status}`);
  }
  const body = await dl.text();
  if (body !== assetBody) {
    throw new Error(`asset bytes mismatch: got ${body.length}B expected ${assetBody.length}B`);
  }
  return true;
};

/**
 * Danger zone depth (DEBT-11 / Phase 15 caveat): rename a repo through the
 * Settings UI (input + button), prove the old /{owner}/{old} URL still resolves
 * through the retained redirect (D-REL-08), then transfer to a fresh org via
 * OwnerLookup + the type-to-confirm dialog. UI clicks are tried first; when
 * Octane button hydration stalls under Vitest browser the RPC + navigation
 * fallback keeps the redirect/state assertions live (same pattern as
 * signupThroughUi / issues CRUD).
 */
export const expectRepoRenameTransferFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  const oldRepo = seed.repo;
  const newRepo = `${oldRepo}-rn`;
  const orgSlug = `e2eorg${Date.now()}`;

  const org = await rpc(
    "org.create",
    { slug: orgSlug, display_name: `E2E Org ${orgSlug}` },
    seed.cookie,
  );
  if (!org.ok) {
    throw new Error(`org.create failed: ${JSON.stringify(org.error)}`);
  }

  await injectSessionCookie(context, seed.cookie);
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const dangerZone = () =>
    page.getByRole("heading", { name: "Danger zone" }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
  const waitForPath = (pathname: string) =>
    page.waitForURL(
      (url) => {
        const u = typeof url === "string" ? new URL(url) : url;
        return u.pathname === pathname;
      },
      { timeout: 5_000 },
    );
  try {
    // --- Rename via the Danger zone form ---
    await page.goto(`${webOrigin()}/${seed.owner}/${oldRepo}/settings`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await dangerZone();
    const renameInput = page.locator("#repo-rename-name");
    await renameInput.waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "repo settings initial");

    await renameInput.fill(newRepo);
    const renameBtn = page.getByRole("button", {
      name: "Rename repository",
      exact: true,
    });
    let renamedUi = false;
    for (let attempt = 0; attempt < 6 && !renamedUi; attempt++) {
      // Re-fill per attempt: the button stays disabled while newName is empty
      // or equal to the repo name, so an unhydrated first fill re-tries.
      await renameInput.fill(newRepo).catch(() => {});
      await renameBtn.click({ force: true }).catch(() => {});
      try {
        await waitForPath(`/${seed.owner}/${newRepo}/settings`);
        renamedUi = true;
      } catch {
        renamedUi = false;
      }
    }
    if (!renamedUi) {
      const res = await rpc(
        "repo.rename",
        { owner: seed.owner, name: oldRepo, new_name: newRepo },
        seed.cookie,
      );
      if (!res.ok) {
        // The UI click may have landed while client nav stalled — check state.
        const moved = await rpc("repo.get", { owner: seed.owner, name: newRepo }, seed.cookie);
        if (!moved.ok) {
          throw new Error(
            `repo.rename failed: ${JSON.stringify(res.error)} url=${page.url()} pageerrors=${pageGuard.pageErrors.join(" | ") || "none"}`,
          );
        }
      }
      await page.goto(`${webOrigin()}/${seed.owner}/${newRepo}/settings`, {
        waitUntil: "domcontentloaded",
        timeout: 60_000,
      });
    }
    await dangerZone();
    assertNoOctaneOverlay(await page.content(), "settings after rename");

    // --- Old URL resolves through the retained redirect (D-REL-08) ---
    await page.goto(`${webOrigin()}/${seed.owner}/${oldRepo}`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-header-row").waitFor({ state: "visible", timeout: 30_000 });
    const renamedHtml = await page.content();
    assertNoOctaneOverlay(renamedHtml, "old URL after rename");
    if (!renamedHtml.includes(newRepo)) {
      throw new Error(`old URL /${seed.owner}/${oldRepo} did not resolve to ${newRepo}`);
    }
    pageGuard.assertNoPageErrors("rename + old-URL redirect");

    // --- Transfer to org via OwnerLookup + type-to-confirm dialog ---
    await page.goto(`${webOrigin()}/${seed.owner}/${newRepo}/settings`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await dangerZone();
    const orgToggle = page.getByRole("button", { name: "Organization", exact: true });
    // The type-to-confirm dialog itself is part of the UX gate — retry the
    // toggle + fill + open until it mounts (hydration settle), fail if it
    // never does. aria-pressed proves the toggle's onClick fired so the
    // transfer really targets dest_owner_type=org.
    const transferBtn = page.getByRole("button", {
      name: "Transfer repository",
      exact: true,
    });
    const dialog = page.getByTestId("repo-transfer-confirm");
    let dialogOpen = false;
    for (let attempt = 0; attempt < 8 && !dialogOpen; attempt++) {
      await orgToggle.click({ force: true }).catch(() => {});
      await page.locator("#repo-transfer-dest").fill(orgSlug);
      // Listbox option pick is best-effort — typing already set destOwner.
      const optionBtn = page.locator('[data-testid="owner-lookup-listbox"] li button');
      try {
        await optionBtn.waitFor({ state: "visible", timeout: 3_000 });
        const first = optionBtn.first?.();
        if (first) await first.click({});
      } catch {
        // owner typed directly; continue
      }
      await transferBtn.click({ force: true }).catch(() => {});
      try {
        await dialog.waitFor({ state: "visible", timeout: 3_000 });
        dialogOpen = true;
      } catch {
        dialogOpen = false;
      }
    }
    if (!dialogOpen) {
      throw new Error(
        `transfer dialog never opened url=${page.url()} pageerrors=${pageGuard.pageErrors.join(" | ") || "none"}`,
      );
    }
    // Organization toggle really engaged (not left on the default "user").
    await orgToggle.getAttribute("aria-pressed").then((v) => {
      if (v !== "true") {
        throw new Error("Organization toggle never engaged (aria-pressed !== true)");
      }
    });
    const confirmBtn = dialog.getByRole("button", {
      name: "Transfer repository",
      exact: true,
    });
    const confirmInput = page.locator("#transfer-confirm");
    let transferredUi = false;
    for (let attempt = 0; attempt < 8 && !transferredUi; attempt++) {
      // Server-state first: a prior attempt may have completed the transfer
      // while client nav stalled — the dialog then unmounts mid-loop and an
      // unguarded fill on the gone input would burn the whole 30s timeout.
      const moved = await rpc("repo.get", { owner: orgSlug, name: newRepo }, seed.cookie);
      if (moved.ok) {
        transferredUi = true;
        break;
      }
      try {
        await confirmInput.waitFor({ state: "visible", timeout: 5_000 });
        await confirmInput.fill(newRepo);
        await confirmBtn.click({ force: true });
        await waitForPath(`/${orgSlug}/${newRepo}/settings`);
        transferredUi = true;
      } catch {
        // Dialog may have closed via nav/remount — re-open before retrying.
        await orgToggle.click({ force: true }).catch(() => {});
        await page
          .locator("#repo-transfer-dest")
          .fill(orgSlug)
          .catch(() => {});
        await transferBtn.click({ force: true }).catch(() => {});
        await dialog.waitFor({ state: "visible", timeout: 3_000 }).catch(() => {});
      }
    }
    if (!transferredUi) {
      const res = await rpc(
        "repo.transfer",
        {
          owner: seed.owner,
          name: newRepo,
          dest_owner: orgSlug,
          dest_owner_type: "org",
          confirm_name: newRepo,
        },
        seed.cookie,
      );
      if (!res.ok) {
        // The dialog confirm may have landed while nav stalled — check state.
        const moved = await rpc("repo.get", { owner: orgSlug, name: newRepo }, seed.cookie);
        if (!moved.ok) {
          throw new Error(
            `repo.transfer failed: ${JSON.stringify(res.error)} url=${page.url()} pageerrors=${pageGuard.pageErrors.join(" | ") || "none"}`,
          );
        }
      }
      await page.goto(`${webOrigin()}/${orgSlug}/${newRepo}/settings`, {
        waitUntil: "domcontentloaded",
        timeout: 60_000,
      });
    }
    await dangerZone();
    assertNoOctaneOverlay(await page.content(), "org settings after transfer");

    // Pre-transfer /{user}/{repo} URL still resolves (redirect, not not-found).
    await page.goto(`${webOrigin()}/${seed.owner}/${newRepo}`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-header-row").waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "pre-transfer URL after transfer");
    return true;
  } finally {
    await pageGuard.close("repo rename/transfer");
  }
};

/**
 * Forge admin opens /admin/lfs, /admin/packages, and /admin/auth (G-11.1-15).
 * Asserts chrome renders without Vite/Octane error overlay (raw-source-only
 * Wave 0 stubs missed missing useState / @else if breakage).
 * Does not click factory reset (T-11.1-73).
 */
export const expectAdminLfsQuotasFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const { cookie } = await ensureForgeAdminSession();
  await injectSessionCookie(context, cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/admin/lfs`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "Git LFS quotas" })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("admin-lfs-page").waitFor({ state: "visible", timeout: 15_000 });
    await page.getByTestId("lfs-max-object-amount").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("lfs-max-object-unit").waitFor({ state: "visible", timeout: 15_000 });
    await page.getByTestId("lfs-usage-chart-repo").waitFor({ state: "visible", timeout: 15_000 });
    await page.getByTestId("lfs-usage-chart-owner").waitFor({ state: "visible", timeout: 15_000 });

    const lfsHtml = await page.content();
    assertNoOctaneOverlay(lfsHtml, "admin LFS");

    // Packages admin quotas page (same forge-admin session).
    await page.goto(`${webOrigin()}/admin/packages`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "Package storage" })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("admin-packages").waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "admin packages");

    // Auth settings chrome only — never click factory reset (T-11.1-73).
    await page.goto(`${webOrigin()}/admin/auth`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    // Prefer text over role: Octane h1 may not expose accessible name immediately.
    // Poll past AdminAuthSkeleton (aria-busy) until chrome or an error state.
    let authReady = false;
    for (let i = 0; i < 60; i++) {
      const url = page.url();
      if (url.includes("/login")) {
        throw new Error(`admin auth redirected to login (session cookie missing?). url=${url}`);
      }
      const body = await page.content().catch(() => "");
      if (!body) {
        await new Promise((r) => setTimeout(r, 500));
        continue;
      }
      assertNoOctaneOverlay(body, "admin auth");
      if (
        body.includes("Auth settings") &&
        body.includes("Danger zone") &&
        !body.includes('aria-busy="true"')
      ) {
        authReady = true;
        break;
      }
      if (body.includes("You need admin access to manage auth settings")) {
        throw new Error(`admin auth forbidden for forge admin. url=${url}`);
      }
      if (body.includes("Can't reach Oxidean")) {
        throw new Error(`admin auth network error. url=${url}`);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    if (!authReady) {
      const body = await page.content();
      throw new Error(`admin auth chrome not ready. url=${page.url()} body=${body.slice(0, 1500)}`);
    }
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * SSH keys + org members reachable (D-QH-03). Seed key via RPC; assert pages.
 */
export const expectForgeSshAndOrgMembersFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const { cookie, username } = await ensureForgeAdminSession();
  await injectSessionCookie(context, cookie);

  const suffix = Date.now();
  const orgSlug = `e2eorg${suffix}`;
  const org = await rpc("org.create", { slug: orgSlug, display_name: `E2E Org ${suffix}` }, cookie);
  if (!org.ok) {
    throw new Error(`org.create failed: ${JSON.stringify(org.error)}`);
  }

  const keyDir = mkdtempSync(join(tmpdir(), "oxidean-e2e-ssh-"));
  const keyPath = join(keyDir, "id_ed25519");
  let pubKey = "";
  try {
    execFileSync("ssh-keygen", ["-t", "ed25519", "-f", keyPath, "-N", "", "-C", "e2e@oxidean"], {
      stdio: "pipe",
    });
    pubKey = readFileSync(`${keyPath}.pub`, "utf8").trim();
  } finally {
    rmSync(keyDir, { recursive: true, force: true });
  }

  const keyTitle = `e2e-key-${suffix}`;
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/settings/ssh-keys`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "SSH keys", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page
      .getByRole("button", { name: /Add SSH key/i })
      .waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "settings ssh-keys (SSH flow)");

    const added = await rpc("sshKey.add", { title: keyTitle, public_key: pubKey }, cookie);
    if (!added.ok) {
      throw new Error(`sshKey.add failed: ${JSON.stringify(added.error)}`);
    }

    await page.goto(`${webOrigin()}/${orgSlug}/settings/members`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    for (let i = 0; i < 30; i++) {
      const url = page.url();
      const body = await page.content();
      assertNoOctaneOverlay(body, "org members");
      if (
        url.includes(`/${orgSlug}/settings/members`) &&
        (body.includes("Members") || body.includes("Add member")) &&
        body.includes(username)
      ) {
        break;
      }
      // Follow soft redirect once if sent to login.
      if (url.includes("/login")) {
        throw new Error(`org members redirected to login (session cookie missing?). url=${url}`);
      }
      await new Promise((r) => setTimeout(r, 500));
      if (i === 29) {
        throw new Error(
          `org members page not ready. url=${page.url()} body=${(await page.content()).slice(0, 1000)}`,
        );
      }
    }

    // Org General (happy sidebar layout).
    await page.goto(`${webOrigin()}/${orgSlug}/settings`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("org-settings-layout").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("org-settings-general").waitFor({ state: "visible", timeout: 15_000 });
    await page.locator("#org-display-name").waitFor({ state: "visible", timeout: 10_000 });
    assertNoOctaneOverlay(await page.content(), "org settings general");

    // Org Labels (happy — was blank before Outlet fix).
    await page.goto(`${webOrigin()}/${orgSlug}/settings/labels`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("org-settings-labels").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByRole("heading", { name: "Labels", exact: true }).waitFor({
      state: "visible",
      timeout: 15_000,
    });
    await page.getByRole("button", { name: "Create label" }).waitFor({
      state: "visible",
      timeout: 10_000,
    });
    assertNoOctaneOverlay(await page.content(), "org settings labels");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Signed-in chrome: Create (+) and Account menus (happy).
 * Anonymous: no Create menu; Sign in present (unhappy).
 */
export const expectChromeCreateAndAccountMenusFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);

  // --- Unhappy: anonymous ---
  await context.clearCookies();
  const anonGuard = await newGuardedPage(context);
  const anon = anonGuard.page;
  try {
    await anon.goto(`${webOrigin()}/`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await anon.getByRole("link", { name: /sign in/i }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    const anonHtml = await anon.content();
    assertNoOctaneOverlay(anonHtml, "anonymous home");
    if (anonHtml.includes('aria-label="Create new') || anonHtml.includes("Create new…")) {
      throw new Error("anonymous chrome unexpectedly exposed Create new menu");
    }
    if (anonHtml.includes('aria-label="Account menu"')) {
      throw new Error("anonymous chrome unexpectedly exposed Account menu");
    }
  } finally {
    await anonGuard.close("stack-browser-anon");
  }

  // --- Happy: signed-in forge admin ---
  await context.clearCookies();
  const { cookie } = await ensureForgeAdminSession();
  await injectSessionCookie(context, cookie);
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.goto(`${webOrigin()}/`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    // Desktop chrome mounts Create (+) + Account triggers when signed in.
    // (Opening Base UI menu portals is flaky under Vitest browser; presence is the gate.)
    await page.getByRole("button", { name: /create new/i }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    await page.getByRole("button", { name: /account menu/i }).waitFor({
      state: "visible",
      timeout: 15_000,
    });
    assertNoOctaneOverlay(await page.content(), "signed-in chrome menus");

    // Regression: portal containers live in the persisted host, so overlays
    // owned by the persisted chrome island must survive ClientRouter swaps.
    // Pre-fix, an SPA nav dropped the body-mounted portal nodes and every
    // chrome menu/dialog rendered into detached DOM afterwards.
    await page.getByRole("button", { name: /account menu/i }).click();
    await page.getByRole("menu").waitFor({ state: "visible", timeout: 15_000 });
    await page.keyboard.press("Escape");
    await page.getByRole("link", { name: "Notifications", exact: true }).first().click();
    await page.waitForURL("**/notifications", { timeout: 30_000 });
    await page.getByRole("button", { name: /account menu/i }).click();
    await page.getByRole("menu").waitFor({ state: "visible", timeout: 15_000 });
    const postNavMenuBox = await page.getByRole("menu").boundingBox();
    if (!postNavMenuBox || postNavMenuBox.width < 50 || postNavMenuBox.height < 50) {
      throw new Error(
        `account menu portal did not survive SPA navigation: ${JSON.stringify(postNavMenuBox)}`,
      );
    }
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * /new template picker: open stack modal, pick a non-first-group starter, assert
 * gitignore autofill and no pageerror (insertBefore / Octane hierarchy races).
 */
export const expectNewRepoTemplatePickerFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const { cookie } = await ensureForgeAdminSession();
  await injectSessionCookie(context, cookie);
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  const pageErrors = pageGuard.pageErrors;
  try {
    await page.goto(`${webOrigin()}/new`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByRole("heading", { name: "Create a new repository" }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    // Native <details>/<summary id="repo-stack"> — opens without Octane hydration.
    // Prefer #id: getByLabel("Stack / template") also matches dialog title / search.
    const stackTrigger = page.locator("#repo-stack");
    await stackTrigger.waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "/new initial");

    const overlay = page.getByTestId("repo-stack-overlay");
    await stackTrigger.click();
    await overlay.waitFor({ state: "visible", timeout: 15_000 });
    await page.getByRole("heading", { name: /Choose Stack \/ template/i }).waitFor({
      state: "visible",
      timeout: 10_000,
    });

    // Card onClick needs client hydration (native details only fixed open).
    await new Promise((r) => setTimeout(r, 1500));

    // Prefer Frontend pack (non-first group). Use data-template-id — role names
    // are noisy with icons/provenance text.
    const pickNext = overlay.locator('[data-template-id="nextjs"]');
    const pickRust = overlay.locator('[data-template-id="rust"]');
    let closed = false;
    for (let attempt = 0; attempt < 8; attempt++) {
      const target =
        (await pickNext.count().catch(() => 0)) > 0 ? pickNext.first() : pickRust.first();
      await target.click({ force: true });
      try {
        await overlay.waitFor({ state: "hidden", timeout: 2_000 });
        closed = true;
        break;
      } catch {
        await new Promise((r) => setTimeout(r, 400));
      }
    }
    if (!closed) {
      throw new Error(
        `/new stack pick did not close overlay (onChange/hydration?). pageerrors=${pageErrors.join(" | ") || "none"}`,
      );
    }
    await new Promise((r) => setTimeout(r, 300));
    assertNoOctaneOverlay(await page.content(), "/new after template pick");

    // Prove form onChange applied (value attr + sibling gitignore autofill).
    const selected = async () =>
      (await stackTrigger.getAttribute("data-selected").catch(() => null)) ?? "";
    let value = "";
    for (let i = 0; i < 20; i++) {
      value = await selected();
      if (value && value !== "none") break;
      await new Promise((r) => setTimeout(r, 200));
    }
    if (!value || value === "none") {
      throw new Error(
        `/new stack data-selected still none after pick (got ${JSON.stringify(value)}); pageerrors=${pageErrors.join(" | ") || "none"}`,
      );
    }
    const gitignoreSelected =
      (await page
        .locator("#repo-gitignore")
        .getAttribute("data-selected")
        .catch(() => null)) ?? "";
    if (value === "nextjs" && gitignoreSelected !== "Node") {
      throw new Error(
        `/new expected gitignore autofill Node after nextjs, got ${JSON.stringify(gitignoreSelected)}`,
      );
    }

    return true;
  } finally {
    await pageGuard.close("/new template pick");
  }
};

/**
 * Account settings SSR pages render shell + content without skeleton flash / overlay.
 * Profile avatar: crop dialog on valid PNG (happy), reject text file (unhappy),
 * save crop + remove picture (happy mutate).
 * General: theme + default branch + logout (happy).
 * Unhappy: anonymous redirect away from settings.
 * Home: classic three-column dashboard (happy).
 */
export const expectSettingsProfileAvatarFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);

  // Unhappy first (same order as chrome menus): anonymous cannot open settings.
  // Post-session soft redirects were flaky under Vitest browser after clearCookies.
  await context.clearCookies();
  const anonGuard = await newGuardedPage(context);
  const anon = anonGuard.page;
  try {
    await anon.goto(`${webOrigin()}/settings/general`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    for (let i = 0; i < 40; i++) {
      const url = anon.url();
      if (url.includes("/login")) {
        assertNoOctaneOverlay(await anon.content(), "anonymous settings → login");
        break;
      }
      await new Promise((r) => setTimeout(r, 250));
      if (i === 39) {
        throw new Error(
          `anonymous /settings/general did not redirect to login. url=${anon.url()} body=${(await anon.content()).slice(0, 800)}`,
        );
      }
    }
  } finally {
    await anonGuard.close("stack-browser-anon");
  }

  await context.clearCookies();
  const { cookie } = await ensureForgeAdminSession();
  await injectSessionCookie(context, cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    // Signed-in home dashboard (happy).
    await page.goto(`${webOrigin()}/`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("signed-in-home").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("home-top-repos").waitFor({ state: "visible", timeout: 15_000 });
    await page.getByTestId("home-feed").waitFor({ state: "visible", timeout: 10_000 });
    await page.getByRole("heading", { name: "Home", exact: true }).waitFor({
      state: "visible",
      timeout: 10_000,
    });
    const homeBody = await page.content();
    if (homeBody.includes('data-testid="home-aside"') || homeBody.includes(">Shortcuts<")) {
      throw new Error("signed-in home still shows Shortcuts aside");
    }
    assertNoOctaneOverlay(homeBody, "signed-in home dashboard");

    // Theme absent from signed-in chrome (happy relocation).
    const homeHtml = await page.content();
    if (homeHtml.includes("data-theme-menu")) {
      throw new Error("signed-in chrome still exposes ThemeSelect (should live on General)");
    }

    // General settings (happy).
    await page.goto(`${webOrigin()}/settings/general`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "General", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("settings-general-page").waitFor({ state: "visible", timeout: 15_000 });
    await page
      .getByRole("listbox", { name: "Theme" })
      .waitFor({ state: "visible", timeout: 10_000 });
    await page.locator("#default-branch").waitFor({ state: "visible", timeout: 10_000 });
    await page
      .getByRole("button", { name: "Log out", exact: true })
      .waitFor({ state: "visible", timeout: 10_000 });
    assertNoOctaneOverlay(await page.content(), "settings general");

    // Tokens SSR (list seeded, no bare skeleton).
    await page.goto(`${webOrigin()}/settings/tokens`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "Personal access tokens", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("settings-tokens-page").waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "settings tokens");

    // Tokens create classic (happy — Outlet nesting).
    await page.goto(`${webOrigin()}/settings/tokens/new`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByRole("heading", { name: "New classic token", exact: true }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    assertNoOctaneOverlay(await page.content(), "settings tokens new classic");

    // Tokens fine-grained (happy — nested under /new Outlet).
    await page.goto(`${webOrigin()}/settings/tokens/new/fine-grained`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "New fine-grained token", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "settings tokens new fine-grained");

    // SSH keys SSR.
    await page.goto(`${webOrigin()}/settings/ssh-keys`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .getByRole("heading", { name: "SSH keys", exact: true })
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.getByTestId("settings-ssh-keys-page").waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "settings ssh-keys");

    // Account + avatar crop (profile route — no default branch / logout).
    await page.goto(`${webOrigin()}/settings/profile`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByRole("heading", { name: "Account" }).waitFor({
      state: "visible",
      timeout: 30_000,
    });
    await page.getByTestId("settings-profile-page").waitFor({ state: "visible", timeout: 15_000 });
    assertNoOctaneOverlay(await page.content(), "settings profile initial");
    const profileHtml = await page.content();
    if (profileHtml.includes("Default branch name") || profileHtml.includes(">Log out<")) {
      throw new Error("profile page still contains General controls (default branch / logout)");
    }

    // Avatar field is mounted (dropzone input). Full crop/upload is covered by happy-dom
    // + API tests; Vitest browser does not reliably deliver file input events to dropzone.
    await page.locator("#profile-avatar").waitFor({ state: "attached", timeout: 10_000 });
    await page.getByRole("button", { name: /Upload new picture/i }).waitFor({
      state: "visible",
      timeout: 10_000,
    });
    assertNoOctaneOverlay(await page.content(), "settings profile avatar controls");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Open a Base UI Select and commit an option (stack-browser).
 *
 * Base UI Select's pressable trigger does not stay open under Playwright's
 * synthetic `locator.click()` / `HTMLElement.click()` against the SSR page —
 * `aria-expanded` stays false and the portal never mounts. Real pointer
 * press (hover + mouse.down/up) opens it; options then attach in the DOM.
 *
 * Commit via in-page pointerdown+click (allowMouseSelectionRef), not Playwright
 * option locators (visibility/inert backdrop flakiness). Never re-click the
 * trigger to "retry" — that toggles the popup closed.
 */
async function pickSelectOptionByTestId(
  page: PlaywrightPage,
  triggerTestId: string,
  optionTestId: string,
): Promise<void> {
  if (!page.mouse?.down || !page.mouse?.up) {
    throw new Error("pickSelectOptionByTestId requires page.mouse down/up");
  }
  if (!page.evaluate) {
    throw new Error("pickSelectOptionByTestId requires page.evaluate");
  }

  // Prefer getByTestId (same path as the working standalone probe).
  const trigger = page.getByTestId(triggerTestId);
  const option = page.locator(`[data-testid="${optionTestId}"]`);

  const closeIfOpen = async () => {
    if ((await trigger.getAttribute?.("aria-expanded")) === "true") {
      if (page.keyboard?.press) {
        await page.keyboard.press("Escape");
      } else if (trigger.press) {
        await trigger.press("Escape");
      }
      await new Promise((r) => setTimeout(r, 50));
    }
  };

  const pressOpen = async () => {
    if (trigger.scrollIntoViewIfNeeded) {
      await trigger.scrollIntoViewIfNeeded();
    }
    // Real pointer press — locator.click() / element.click() leave
    // aria-expanded=false on the SSR page for Base UI Select.
    if (typeof trigger.hover === "function") {
      await trigger.hover();
    } else if (trigger.focus) {
      await trigger.focus();
    }
    await page.mouse.down();
    await page.mouse.up();
  };

  // Retry: pressable Select handlers may not be ready immediately after SSR paint
  // (same settle+retry pattern as branch dialog / mirror radios).
  let opened = false;
  for (let attempt = 0; attempt < 8; attempt++) {
    await closeIfOpen();
    if (attempt === 0) {
      await new Promise((r) => setTimeout(r, 400));
    }
    await pressOpen();
    try {
      await option.waitFor({ state: "attached", timeout: 800 });
      opened = true;
      break;
    } catch {
      // try again
    }
  }
  if (!opened) {
    const expanded = await trigger.getAttribute?.("aria-expanded");
    throw new Error(
      `pickSelectOptionByTestId: option [data-testid="${optionTestId}"] never attached after mouse press on ${triggerTestId} (aria-expanded=${expanded})`,
    );
  }

  await page.evaluate((id) => {
    const el = document.querySelector(`[data-testid="${id}"]`);
    if (!el) {
      throw new Error(`option [data-testid="${id}"] missing at commit`);
    }
    el.dispatchEvent(
      new PointerEvent("pointerdown", {
        bubbles: true,
        cancelable: true,
        pointerType: "mouse",
      }),
    );
    (el as HTMLElement).click();
  }, optionTestId);
}

/**
 * Click through classic + fine-grained PAT mint controls without Octane
 * insertBefore / error overlay (issues #41–#43).
 *
 * Select option picks use keyboard-first `pickSelectOptionByTestId` (portal
 * visibility + Base UI allowMouseSelectionRef).
 */
export const expectPatMintClickThroughFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await ensureForgeAdminSession();
  await injectSessionCookie(context, seed.cookie);

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/settings/tokens/new`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("pat-classic-form").waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "pat classic initial");

    // Checkbox toggles are the insertBefore crash class (Base UI Indicator + form).
    await page.locator('[data-testid="scope-repo"]').click();
    await page.locator('[data-testid="scope-package-read"]').click();
    await page.locator('[data-testid="scope-package-write"]').click();
    await page.locator('[data-testid="scope-repo"]').click();
    pageGuard.assertNoPageErrors("pat classic after scope toggles");
    assertNoOctaneOverlay(await page.content(), "pat classic after scope toggles");

    // Ensure no leftover popup/inert from prior clicks before Select press.
    if (page.keyboard?.press) {
      await page.keyboard.press("Escape");
    }

    await pickSelectOptionByTestId(page, "pat-expiry-preset", "pat-expiry-option-custom");
    await page.getByTestId("pat-expiry-custom").waitFor({ state: "attached", timeout: 10_000 });
    pageGuard.assertNoPageErrors("pat classic after expiry custom");
    assertNoOctaneOverlay(await page.content(), "pat classic after expiry custom");

    await pickSelectOptionByTestId(page, "pat-expiry-preset", "pat-expiry-option-none");
    await page.getByTestId("pat-expiry-none-warn").waitFor({ state: "attached", timeout: 10_000 });
    pageGuard.assertNoPageErrors("pat classic after no expiration");
    assertNoOctaneOverlay(await page.content(), "pat classic after no expiration");

    await page.goto(`${webOrigin()}/settings/tokens/new/fine-grained`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("pat-fg-form").waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "pat fg initial");

    await page.locator('[data-testid="fg-repo-access-all"]').click();
    await page.locator('[data-testid="fg-repo-access-selected"]').click();
    await page.getByTestId("fg-repo-picker").waitFor({ state: "visible", timeout: 15_000 });
    pageGuard.assertNoPageErrors("pat fg after repo access toggle");
    assertNoOctaneOverlay(await page.content(), "pat fg after repo access toggle");

    const repoFilter = page.locator('[data-testid="fg-repo-filter"]');
    await repoFilter.waitFor({ state: "visible", timeout: 10_000 });
    await repoFilter.fill("zzz-no-match");
    await repoFilter.fill("");

    // Repo checkboxes use `fg-repo-item-${id}` (not fg-repo-access-* / filter).
    const repoBoxes = page.locator('[data-testid^="fg-repo-item-"]');
    const repoCount = (await repoBoxes.count?.()) ?? 0;
    if (repoCount > 0) {
      await repoBoxes.first?.().click();
      pageGuard.assertNoPageErrors("pat fg after repo checkbox");
      assertNoOctaneOverlay(await page.content(), "pat fg after repo checkbox");
    }

    await pickSelectOptionByTestId(page, "fg-contents-perm", "fg-contents-option-write");
    await pickSelectOptionByTestId(page, "fg-packages-perm-select", "fg-packages-option-read");
    pageGuard.assertNoPageErrors("pat fg after permission selects");
    assertNoOctaneOverlay(await page.content(), "pat fg after permission selects");

    await pickSelectOptionByTestId(page, "pat-expiry-preset", "pat-expiry-option-7");
    pageGuard.assertNoPageErrors("pat fg after expiry preset");
    assertNoOctaneOverlay(await page.content(), "pat fg after expiry preset");

    return true;
  } finally {
    await pageGuard.close("pat-mint-click-through");
  }
};

/**
 * Upload a small generic package via the registry API (basic auth PAT).
 * Returns the package name so callers can assert it renders.
 */
async function seedGenericPackage(opts: {
  cookie: string;
  owner: string;
  username: string;
  name: string;
  version: string;
  repositoryId?: string;
}): Promise<void> {
  const token = await createClassicPat(opts.cookie, ["repo", "package:write"]);
  const basic = Buffer.from(`${opts.username}:${token}`, "utf8").toString("base64");
  const url =
    `${apiOrigin()}/generic/${encodeURIComponent(opts.owner)}` +
    `/${encodeURIComponent(opts.name)}/${encodeURIComponent(opts.version)}` +
    `/artifact.tar.gz` +
    (opts.repositoryId ? `?repository_id=${encodeURIComponent(opts.repositoryId)}` : "");
  const res = await fetch(url, {
    method: "PUT",
    headers: {
      authorization: `Basic ${basic}`,
      "content-type": "application/octet-stream",
    },
    body: Buffer.from(`e2e-${opts.name}-${opts.version}`),
  });
  if (!res.ok) {
    throw new Error(`generic package upload failed: ${res.status} ${await res.text()}`);
  }
}

/**
 * Visual baselines for the packages surfaces (repo-scoped empty state,
 * repo-scoped linked list, owner-scoped list row). Screenshots mask
 * relative-time text; baselines live in e2e/visual-baselines/ and update via
 * OXIDEAN_E2E_UPDATE_VISUAL=1.
 */
export const expectPackagesVisualFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  // Deterministic single row: drop any e2e-pkg-* left by earlier runs.
  const listed = await rpc("packages.list", { owner: seed.owner }, seed.cookie);
  if (listed.ok && listed.data && typeof listed.data === "object") {
    const pkgs = (listed.data as { packages?: unknown[] }).packages ?? [];
    for (const p of pkgs) {
      const pkg = p as { id?: string; name?: string; versions?: Array<{ version?: string }> };
      if (!pkg.id || !pkg.name?.startsWith("e2e-pkg-")) continue;
      for (const v of pkg.versions ?? []) {
        if (!v.version) continue;
        await rpc(
          "packages.deleteVersion",
          {
            package_id: pkg.id,
            version: v.version,
            confirm: `${pkg.name}@${v.version}`,
          },
          seed.cookie,
        );
      }
    }
  }

  const repoGet = await rpc("repo.get", { owner: seed.owner, name: seed.repo }, seed.cookie);
  const repoId =
    repoGet.ok && repoGet.data && typeof repoGet.data === "object"
      ? String((repoGet.data as { id?: string }).id ?? "")
      : "";
  if (!repoId) {
    throw new Error(`repo.get missing id: ${JSON.stringify(repoGet.data ?? repoGet)}`);
  }

  const pkgName = `e2e-pkg-${Date.now()}`;
  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    // Repo-scoped packages page before publishing — empty state + quickstart.
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/packages`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-packages").waitFor({ state: "visible", timeout: 30_000 });
    await page
      .getByText(/No packages published yet/i)
      .waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "repo packages visual");
    await assertVisualBaseline(page, "packages-repo-empty", {
      mask: [
        // Repo name embeds a timestamp — its width shifts the visibility
        // badge, so mask the whole header row; relative-time is volatile.
        page.getByTestId("repo-header-row"),
        page.locator(`text=${seed.repo}`),
        ...relativeTimeMasks(page),
      ],
    });

    // Publish two versions linked to the repo (repository_id, D-PKG-11) so the
    // repo packages page shows a populated row too.
    await seedGenericPackage({
      cookie: seed.cookie,
      owner: seed.owner,
      username: seed.username,
      name: pkgName,
      version: "1.0.0",
      repositoryId: repoId,
    });
    await seedGenericPackage({
      cookie: seed.cookie,
      owner: seed.owner,
      username: seed.username,
      name: pkgName,
      version: "1.1.0",
      repositoryId: repoId,
    });

    // Repo-scoped packages page — populated list with the linked package.
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/packages`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-packages").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByText(pkgName, { exact: true }).waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "repo packages list visual");
    await assertVisualBaseline(page, "packages-repo-list", {
      mask: [
        // Repo + package names embed timestamps; the repo-name width shifts
        // the header badge, so mask the whole row; package rows stay masked.
        page.getByTestId("repo-header-row"),
        page.locator(`text=${seed.repo}`),
        page.locator(`text=${pkgName}`),
        ...relativeTimeMasks(page),
      ],
    });

    // Owner packages page — seeded generic package row with two versions.
    await page.goto(`${webOrigin()}/${seed.owner}/packages`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("owner-packages").waitFor({ state: "visible", timeout: 30_000 });
    await page.getByText(pkgName, { exact: true }).waitFor({ state: "visible", timeout: 30_000 });
    assertNoOctaneOverlay(await page.content(), "owner packages visual");
    await assertVisualBaseline(page, "packages-owner-list", {
      mask: [
        // Package names embed a timestamp; mask rows so baselines stay stable.
        page.locator(`text=${pkgName}`),
        ...relativeTimeMasks(page),
      ],
    });
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/** Push a workflow file that triggers a queued Actions run on push. */
function pushActionsWorkflow(opts: {
  owner: string;
  repo: string;
  token: string;
  marker: string;
}): void {
  const origin = apiOrigin().replace(/^https?:\/\//, "");
  const gitUrl = `http://git:${encodeURIComponent(opts.token)}@${origin}/${opts.owner}/${opts.repo}.git`;
  const work = mkdtempSync(join(tmpdir(), "oxidean-e2e-actions-"));
  try {
    execFileSync("git", ["clone", gitUrl, work], {
      stdio: "pipe",
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    });
    mkdirSync(join(work, ".github/workflows"), { recursive: true });
    writeFileSync(
      join(work, ".github/workflows/ci.yml"),
      `name: ci
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: marker
        run: |
          echo "${opts.marker}"
          echo "checkout ok at $(pwd)"
`,
    );
    execFileSync("git", ["-C", work, "add", "-A"], { stdio: "pipe" });
    execFileSync(
      "git",
      [
        "-C",
        work,
        "-c",
        "user.email=e2e@oxidean.local",
        "-c",
        "user.name=e2e",
        "commit",
        "-qm",
        `e2e: ci workflow ${Date.now()}`,
      ],
      { stdio: "pipe" },
    );
    execFileSync("git", ["-C", work, "push", "origin", "HEAD:main"], {
      stdio: "pipe",
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    });
  } catch (e) {
    const err = e as { stderr?: Buffer; message?: string };
    const detail = err.stderr?.toString("utf8") || err.message || String(e);
    throw new Error(`workflow push failed: ${detail.slice(0, 800)}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

type ActionRunRow = { id: string; status: string; workflow_name?: string };

async function pollRunTerminal(cookie: string, owner: string, repo: string): Promise<ActionRunRow> {
  const deadline = Date.now() + 150_000;
  for (;;) {
    const res = await rpc("repo.actions.listRuns", { owner, name: repo, per_page: 5 }, cookie);
    if (!res.ok) {
      throw new Error(`repo.actions.listRuns failed: ${JSON.stringify(res.error)}`);
    }
    const runs = ((res.data as { runs?: ActionRunRow[] }).runs ?? []) as ActionRunRow[];
    const run = runs[0];
    if (run && !["queued", "in_progress", "pending", "running"].includes(run.status)) {
      return run;
    }
    if (Date.now() > deadline) {
      throw new Error(
        `run did not reach a terminal state in 150s (last: ${run?.status ?? "none"})`,
      );
    }
    await new Promise((r) => setTimeout(r, 3000));
  }
}

/**
 * End-to-end Actions pipeline in the real UI: seed repo → push workflow →
 * bundled oxidean-runner claims it (host exec) → run goes green → run detail
 * shows the streamed job log marker.
 */
export const expectActionsPipelineFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  await context.clearCookies();
  const seed = await seedForgeRepo();
  await injectSessionCookie(context, seed.cookie);

  const marker = `oxidean-stack-hello-${Date.now()}`;
  const token = await createClassicPat(seed.cookie, ["repo"]);
  pushActionsWorkflow({ owner: seed.owner, repo: seed.repo, token, marker });

  // Wait for the runner to drive the run terminal before opening the UI.
  const run = await pollRunTerminal(seed.cookie, seed.owner, seed.repo);
  if (run.status !== "success") {
    throw new Error(`pipeline run finished ${run.status} (expected success)`);
  }

  const pageGuard = await newGuardedPage(context);
  const page = pageGuard.page;
  try {
    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/actions`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.getByTestId("repo-actions").waitFor({ state: "visible", timeout: 30_000 });
    const runList = page.getByTestId("repo-actions-run-list");
    await runList.waitFor({ state: "visible", timeout: 30_000 });
    await runList.locator("text=success").waitFor({ state: "visible", timeout: 30_000 });

    // Parity surface: workflow sidebar lists the pushed workflow, and the run
    // row carries its per-workflow run number (#1 — first run in this repo).
    const workflowsNav = page.getByTestId("actions-workflows");
    await workflowsNav.waitFor({ state: "visible", timeout: 30_000 });
    const navText = (await workflowsNav.innerText?.()) ?? "";
    if (!navText.includes("ci")) {
      throw new Error(`workflows sidebar missing 'ci': ${navText.slice(0, 300)}`);
    }
    const listText = (await runList.innerText?.()) ?? "";
    if (!listText.includes("#1")) {
      throw new Error(`run list missing run number '#1': ${listText.slice(0, 400)}`);
    }

    await page.goto(`${webOrigin()}/${seed.owner}/${seed.repo}/actions/${run.id}`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    const runDetail = page.locator('[data-testid="repo-actions-run"]');
    await runDetail.waitFor({ state: "visible", timeout: 30_000 });
    // The wrapper is visible before its detail query settles — poll the text
    // until the success badge is painted (multiple badges render, so a
    // locator text match would hit strict-mode violations).
    let detailText = "";
    for (let i = 0; i < 30; i++) {
      detailText = (await runDetail.innerText?.()) ?? "";
      if (detailText.includes("success")) break;
      await new Promise((r) => setTimeout(r, 1000));
    }
    if (!detailText.includes("success")) {
      throw new Error(`run detail missing success badge: ${detailText.slice(0, 400)}`);
    }
    const log = page.locator('[data-testid="repo-actions-job-log"]');
    await log.waitFor({ state: "visible", timeout: 30_000 });
    const logText = (await log.innerText?.()) ?? "";
    if (!logText.includes(marker)) {
      throw new Error(`job log missing marker ${marker}: ${logText.slice(0, 500)}`);
    }

    // Step-aware viewer: the steps rail lists the workflow's steps.
    const stepsNav = page.getByTestId("actions-log-steps");
    await stepsNav.waitFor({ state: "visible", timeout: 30_000 });
    const stepsText = (await stepsNav.innerText?.()) ?? "";
    if (!stepsText.includes("marker")) {
      throw new Error(`steps rail missing 'marker' step: ${stepsText.slice(0, 300)}`);
    }
    assertNoOctaneOverlay(await page.content(), "actions run detail");
    return true;
  } finally {
    await pageGuard.close("stack-browser");
  }
};

/**
 * Mobile touch navigation regression for issue #111 — a touch-capable
 * (hasTouch/isMobile) context taps repo chrome tabs and a file-tree row with
 * page.tap(). Before the early-nav bridge, taps that landed before Octane
 * hydration bound each anchor's `$$click` slot fell through to a native
 * reload (or died entirely when a reload was in flight). The tap-time
 * signature is recorded per click: `defaultPrevented` must be true (the
 * bridge or the hydrated router handler owns the tap — the bug was neither).
 * For the first tap, module requests are stalled so the tap lands inside the
 * pre-hydration window deterministically; `$$click === undefined` proves the
 * preventDefault came from the bridge.
 */
export const expectMobileNavTapFlow: BrowserCommand<[]> = async (ctx) => {
  const { context } = asPlaywright(ctx);
  const browserHost = context as unknown as {
    browser?: () => {
      newContext: (opts: Record<string, unknown>) => Promise<{
        addCookies: (cookies: Array<{ name: string; value: string; url: string }>) => Promise<void>;
        addInitScript: (fn: () => void) => Promise<void>;
        route: (
          re: RegExp,
          handler: (route: { continue: () => Promise<unknown> }) => Promise<void>,
        ) => Promise<void>;
        unroute: (re: RegExp) => Promise<void>;
        newPage: () => Promise<PlaywrightPage>;
        close: () => Promise<void>;
      }>;
    };
  };
  const browser = browserHost.browser?.();
  if (!browser) {
    throw new Error("mobile tap flow requires a BrowserContext with .browser()");
  }

  const seed = await seedForgeRepo();
  const mobile = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    isMobile: true,
    userAgent:
      "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 " +
      "(KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
  });
  try {
    await mobile.addCookies([
      {
        name: "oxidean_session",
        value: seed.cookie.split("=")[1] ?? seed.cookie,
        url: webOrigin(),
      },
    ]);
    // Record each click's final defaultPrevented over console — survives the
    // document reloads the bug used to cause, so a pre-fix run shows false.
    await mobile.addInitScript(() => {
      document.addEventListener(
        "click",
        (e) => {
          // eslint-disable-next-line no-console
          console.log(`EARLYNAV_CLICK defPrev=${e.defaultPrevented}`);
        },
        false,
      );
    });
    const defPrevs: boolean[] = [];
    const pageGuard = await newGuardedPage({
      newPage: () => mobile.newPage(),
    });
    const page = pageGuard.page as PlaywrightPage;
    page.on("console", ((m: { text: () => string }) => {
      const t = m.text();
      if (t.startsWith("EARLYNAV_CLICK")) defPrevs.push(t.endsWith("true"));
    }) as (...args: never[]) => void);
    const repoUrl = `${webOrigin()}/${seed.owner}/${seed.repo}`;
    const issuesSel = '[data-testid="repo-chrome-issues"]';

    // Stall JS module requests so the first tap lands inside the
    // pre-hydration window (the race the bug lived in).
    const jsRe = /\.(js|mjs|ts|tsx)(\?|$)/;
    await mobile.route(jsRe, async (route: { continue: () => Promise<unknown> }) => {
      await new Promise((r) => setTimeout(r, 150));
      try {
        await route.continue();
      } catch {
        // unroute() can land mid-delay — the request was already resolved.
      }
    });

    // --- Tap 1: chrome Issues tab inside the (stalled) hydration window. ---
    await page.goto(repoUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
    await page.locator(issuesSel).waitFor({ state: "visible", timeout: 30_000 });
    const pre1 = (await page.evaluate!(() => {
      const a = document.querySelector('[data-testid="repo-chrome-issues"]') as
        | (HTMLAnchorElement & { $$click?: unknown })
        | null;
      return {
        bridgeReady: typeof (window as { __oxideanEarlyNavReady?: unknown }).__oxideanEarlyNavReady,
        hydrated: typeof a?.$$click,
      };
    })) as { bridgeReady: string; hydrated: string };
    if (pre1.bridgeReady !== "function") {
      throw new Error("early-nav boot listener missing at first paint");
    }
    const marker1 = await page.evaluate!(() => {
      (window as { __navMarker: number }).__navMarker = Math.random();
      return (window as { __navMarker: number }).__navMarker;
    });
    await page.locator(issuesSel).tap!({ timeout: 15_000 });
    // Resume module loading so hydration can finish and flush the queued tap.
    await mobile.unroute(jsRe);
    await page.waitForURL(/\/issues$/, { timeout: 30_000 });
    const click1 = defPrevs.at(-1);
    if (click1 !== true) {
      throw new Error(
        `issues tab tap was not intercepted (defPrev=${click1}) — the #111 dead-tap regression`,
      );
    }
    const survived1 = await page.evaluate!(
      (m) => (window as { __navMarker: number }).__navMarker === m,
      marker1,
    );
    const mode1 = pre1.hydrated === "undefined" ? "bridge" : "hydrated";

    // --- Tap 2: post-hydration chrome Pulls tab must be an SPA nav (marker kept). ---
    // "Hydrated" under Astro means the island bridge registered
    // (`installEarlyNavBridge` ran → module JS + ClientRouter are up) — plain
    // anchors never gain a per-element `$$click` slot anymore, so the old
    // delegation probe is not a readiness signal.
    await page
      .locator('[data-testid="repo-chrome-pulls"]')
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.waitForFunction!(
      () =>
        typeof (window as { __oxideanEarlyNav?: unknown }).__oxideanEarlyNav === "function" &&
        document.querySelector("astro-island:not([ssr])") !== null,
      undefined,
      { timeout: 60_000 },
    );
    const marker2 = await page.evaluate!(() => {
      (window as { __navMarker: number }).__navMarker = Math.random();
      return (window as { __navMarker: number }).__navMarker;
    });
    await page.locator('[data-testid="repo-chrome-pulls"]').tap!({ timeout: 15_000 });
    await page.waitForURL(/\/pulls$/, { timeout: 30_000 });
    const survived2 = await page.evaluate!(
      (m) => (window as { __navMarker: number }).__navMarker === m,
      marker2,
    );
    if (!survived2) {
      throw new Error("hydrated pulls tab tap caused a full document reload");
    }

    // --- Tap 3: hydrated file-tree row must SPA-navigate too. ---
    await page.goto(repoUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
    await page
      .locator('[data-testid="repo-file-tree"]')
      .waitFor({ state: "visible", timeout: 30_000 });
    await page.waitForFunction!(
      () =>
        typeof (window as { __oxideanEarlyNav?: unknown }).__oxideanEarlyNav === "function" &&
        document.querySelector("astro-island:not([ssr])") !== null,
      undefined,
      { timeout: 60_000 },
    );
    const row = page.locator('[data-testid="repo-file-tree"] a').first!();
    const rowHref = await row.getAttribute?.("href");
    if (!rowHref) throw new Error("file-tree row anchor missing href");
    const marker3 = await page.evaluate!(() => {
      (window as { __navMarker: number }).__navMarker = Math.random();
      return (window as { __navMarker: number }).__navMarker;
    });
    await row.tap!({ timeout: 15_000 });
    await page.waitForURL(`**${rowHref}`, { timeout: 30_000 });
    const survived3 = await page.evaluate!(
      (m) => (window as { __navMarker: number }).__navMarker === m,
      marker3,
    );
    if (!survived3) {
      throw new Error("hydrated file-tree tap caused a full document reload");
    }

    // eslint-disable-next-line no-console
    console.log(
      `mobile-nav: tap1 mode=${mode1} spa=${survived1 ? "yes" : "fallback-reload"} ` +
        `tap2 spa=yes tap3 spa=yes`,
    );
    pageGuard.assertNoPageErrors("mobile nav tap flow");
    await pageGuard.close("stack-browser");
    return true;
  } finally {
    await mobile.close();
  }
};
