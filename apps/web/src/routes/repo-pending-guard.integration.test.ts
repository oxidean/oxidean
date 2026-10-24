/**
 * Pending-before-not-found guard (issue #169).
 *
 * Repo leaf pages mount only after `RepoLayout`'s `repo.get` resolves; their
 * own loader queries then sit pending. A leaf that treats "no data yet" as
 * "not found" flashes `RepoNotFound` ("Page not found") on every cold load
 * and client-side navigation — the production symptom behind #169 (and the
 * earlier #120 fix, which insights missed).
 *
 * This suite mounts every `$owner.$repo.*` route through the real
 * `AppPage` → `RepoLayout` path with `repo.get` resolved but every other RPC
 * permanently pending, waits for the layout chrome, and asserts no not-found
 * copy renders. New repo routes are picked up automatically via
 * `import.meta.glob` — the guard cannot silently drift out of coverage.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const USER = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

// Every unit flag on so leaves mount their real views (not the disabled
// state) — the pending gap is what this guard exercises.
const REPO = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user",
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public",
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: true,
  can_write: true,
  issues_enabled: true,
  pulls_enabled: true,
  wiki_enabled: true,
  projects_enabled: true,
  discussions_enabled: true,
};

vi.mock("@/lib/api-client", () => {
  // Callable that returns a promise which never resolves — leaf queries stay
  // pending forever. Any property access returns the same callable so nested
  // namespaces (`apiClient.repo.actions.listRuns`) keep working.
  const pending: unknown = new Proxy(() => new Promise(() => {}), {
    get(_t, prop) {
      if (typeof prop === "symbol" || prop === "then") return undefined;
      return pending;
    },
  });
  const withPending = (overrides: Record<string, unknown>) =>
    new Proxy(overrides, {
      get(t, prop) {
        if (typeof prop === "symbol") return undefined;
        return prop in t ? t[prop as keyof typeof t] : pending;
      },
    });
  return {
    apiClient: withPending({
      auth: withPending({ me: () => Promise.resolve({ ok: true, data: USER }) }),
      repo: withPending({ get: () => Promise.resolve({ ok: true, data: REPO }) }),
    }),
  };
});

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: USER,
    needsSetup: false,
    allowSignup: true,
  }),
  resolveAllowSignup: () => true,
}));

// ── Route table: every $owner.$repo.* leaf, derived from the file set ──────

const modules = import.meta.glob<{ [key: string]: unknown }>("./*.tsrx");

/** Named param fixtures — valid shapes so leaves bind and stay pending. */
const PARAM: Record<string, string> = {
  n: "5",
  sha: "0123456789abcdef0123456789abcdef01234567",
  tag: "v1.0.0",
  run: "123",
};

/** Splat fixtures per parent literal (file-path splats use the default). */
const SPLAT: Record<string, string> = {
  compare: "main...feature",
  commits: "main",
  discussions: "1",
  projects: "1",
  wiki: "Home",
};
const DEFAULT_SPLAT = "main/src/x.ts";

/** `$owner.$repo.issues.$n` → `/ada/hello/issues/5`; trailing `.$` → splat. */
function urlFor(file: string): string | null {
  const segs = file
    .replace(/^\.\//, "")
    .replace(/\.tsrx$/, "")
    .split(".");
  if (segs[0] !== "$owner" || segs[1] !== "$repo" || segs.length < 3) return null;
  const out = ["ada", "hello"];
  for (let i = 2; i < segs.length; i++) {
    const seg = segs[i]!;
    if (seg === "index") continue;
    if (seg === "$") out.push(SPLAT[segs[i - 1]!] ?? DEFAULT_SPLAT);
    else if (seg.startsWith("$")) out.push(PARAM[seg.slice(1)] ?? "x");
    else out.push(seg);
  }
  return `/${out.join("/")}`;
}

const routes = Object.keys(modules)
  .map((file) => ({ file, url: urlFor(file) }))
  .filter((r): r is { file: string; url: string } => r.url !== null)
  .sort((a, b) => a.url.localeCompare(b.url));

const NOT_FOUND_COPY = ["Page not found", "We couldn't find that page"];

describe("repo leaf pending guard (#169)", () => {
  it("covers the whole $owner.$repo.* leaf set", () => {
    // Layout file itself (`$owner.$repo.tsrx`) is not a leaf.
    expect(routes.length).toBeGreaterThanOrEqual(30);
  });

  afterEach(cleanup);

  for (const { file, url } of routes) {
    it(`${url} shows no not-found while leaf queries are pending`, async () => {
      window.history.pushState({}, "", url);
      const mod = await modules[file]!();
      const Page = Object.entries(mod).find(
        ([k, v]) => k.endsWith("Page") && typeof v === "function",
      )?.[1];
      expect(Page, `${file} exports a *Page root`).toBeTruthy();
      renderWithQueryClient(Page as never);

      // Layout resolved → repo chrome up → leaf mounted on pending queries.
      await waitFor(() => {
        expect(screen.getByTestId("repo-header-row")).toBeInTheDocument();
      });
      const text = document.body.textContent ?? "";
      for (const copy of NOT_FOUND_COPY) {
        expect(text, `${file} rendered "${copy}" while pending`).not.toContain(copy);
      }
      expect(text.trim().length).toBeGreaterThan(0);
      // First-mount import of the heaviest leaves can exceed the 5s default
      // on cold CI runners.
    }, 15000);
  }
});
