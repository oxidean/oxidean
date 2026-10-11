/**
 * Playwright pageerror guard for stack-browser e2e commands.
 * Fails on Octane DOM races (insertBefore) and any other uncaught pageerror.
 */
import { DOM_RACE_RE } from "../../src/test/dom-errors.ts";
/**
 * Fail fast on Vite overlay, runtime ReferenceErrors, or Octane's default
 * error UI (`<strong style="font-size:1rem">Something went wrong!</strong>`).
 */
export function assertNoOctaneOverlay(html: string, label: string): void {
  if (
    html.includes("vite-error-overlay") ||
    html.includes("Something went wrong!") ||
    /is not defined|ReferenceError|Octane error|@else if/i.test(html)
  ) {
    throw new Error(`${label} showed Vite/Octane render error. body=${html.slice(0, 1200)}`);
  }
}

/** Minimal page surface shared with commands.ts. */
export type GuardablePage = {
  on: (event: string, handler: (...args: never[]) => void) => void;
  close: () => Promise<unknown>;
  content?: () => Promise<string>;
};

/**
 * Bounded `page.content()`. Under Astro's ClientRouter every document commit
 * is followed by a `history.replaceState` (scroll-state stash) and pending
 * view-transition bookkeeping — a `content()` call landing in that window
 * throws "page is navigating" under plain Playwright but can wedge the vitest
 * command transport entirely. Race each attempt and retry through the window.
 */
export async function readPageHtml(
  page: { content?: () => Promise<string> },
  attempts = 20,
): Promise<string> {
  if (!page.content) throw new Error("page.content unavailable");
  let lastErr: unknown = null;
  for (let i = 0; i < attempts; i++) {
    try {
      return await Promise.race([
        page.content(),
        new Promise<string>((_, rej) =>
          setTimeout(() => rej(new Error("page.content exceeded 10s")), 10_000),
        ),
      ]);
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 400));
    }
  }
  throw lastErr instanceof Error ? lastErr : new Error(String(lastErr));
}

function boundedCall<T>(call: Promise<T>, ms: number, label: string): Promise<T> {
  return Promise.race([
    call,
    new Promise<T>((_, rej) => setTimeout(() => rej(new Error(`${label} exceeded ${ms}ms`)), ms)),
  ]);
}

type GotoFn = (url: string, opts?: { timeout?: number }) => Promise<unknown>;
type EvalFn = (fn: unknown, arg?: unknown) => Promise<unknown>;

/**
 * Wrap a Playwright page so transport-level wedge points get bounded:
 * - `content()` retries through the post-commit navigation window;
 * - `goto()` races past its own timeout (the vitest transport can swallow
 *   playwright's per-call timeout during an in-flight swap);
 * - `close()` is bounded so a wedged page cannot hang the flow's finally.
 * Every other member binds to the real page unchanged.
 */
export function wrapPageForNavRaces<P extends GuardablePage>(page: P): P {
  const target = page as unknown as Record<string | symbol, unknown>;
  return new Proxy(page, {
    get(t, prop, recv) {
      if (prop === "content") {
        return () => readPageHtml(page);
      }
      if (prop === "goto") {
        // Retry: a document goto issued while a ClientRouter in-flight swap
        // still owns the frame gets superseded (net::ERR_ABORTED); the retry
        // lands after the swap settles. Also bound each attempt — the vitest
        // transport can swallow playwright's own per-call timeout mid-swap.
        return async (url: string, opts?: { timeout?: number }) => {
          let lastErr: unknown = null;
          for (let i = 0; i < 5; i++) {
            try {
              return await boundedCall(
                (target.goto as GotoFn).call(page, url, opts),
                (opts?.timeout ?? 60_000) + 15_000,
                "page.goto",
              );
            } catch (e) {
              lastErr = e;
              await new Promise((r) => setTimeout(r, 400));
            }
          }
          throw lastErr instanceof Error ? lastErr : new Error(String(lastErr));
        };
      }
      if (prop === "close") {
        return () =>
          boundedCall((target.close as () => Promise<unknown>).call(page), 15_000, "page.close");
      }
      if (prop === "evaluate") {
        // Same post-commit window as content(): an evaluate landing mid-swap
        // can wedge the transport. Bound it; callers' logic stays unchanged.
        return (fn: unknown, arg?: unknown) =>
          boundedCall((target.evaluate as EvalFn).call(page, fn, arg), 30_000, "page.evaluate");
      }
      const v = Reflect.get(t, prop, recv);
      return typeof v === "function" ? v.bind(t) : v;
    },
  }) as P;
}

export type PageContext = {
  newPage: () => Promise<GuardablePage>;
};

export type GuardedPage<P extends GuardablePage = GuardablePage> = {
  page: P;
  pageErrors: string[];
  assertNoPageErrors: (label?: string) => void;
  /** Assert no races/errors, then close. */
  close: (label?: string) => Promise<void>;
};

export async function newGuardedPage<P extends GuardablePage>(context: {
  newPage: () => Promise<P>;
}): Promise<GuardedPage<P>> {
  const rawPage = await context.newPage();
  const page = wrapPageForNavRaces(rawPage);
  const pageErrors: string[] = [];
  page.on("pageerror", ((err: Error) => {
    pageErrors.push(err?.message ?? String(err));
  }) as (...args: never[]) => void);

  const assertNoPageErrors = (label = "page") => {
    const races = pageErrors.filter((m) => DOM_RACE_RE.test(m));
    if (races.length > 0) {
      throw new Error(`${label} DOM race pageerror: ${races.join(" | ")}`);
    }
    if (pageErrors.length > 0) {
      throw new Error(`${label} unexpected pageerror: ${pageErrors.join(" | ")}`);
    }
  };

  return {
    page,
    pageErrors,
    assertNoPageErrors,
    async close(label = "page") {
      // Always close first so a failed assertion cannot leak browser contexts.
      await page.close();
      assertNoPageErrors(label);
    },
  };
}
