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
  const page = await context.newPage();
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
