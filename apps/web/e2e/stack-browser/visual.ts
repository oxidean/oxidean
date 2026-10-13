/**
 * Screenshot baseline helpers for stack-browser e2e.
 *
 * Baselines are committed PNGs under `e2e/visual-baselines/`. A run compares a
 * fresh page screenshot against the baseline with pixelmatch; on mismatch (or
 * missing baseline) it writes `actual` + `diff` artifacts to the gitignored
 * repo `tmp/e2e-visual/` directory and throws.
 *
 * Regenerate/seed baselines with `OXIDEAN_E2E_UPDATE_VISUAL=1` — the run still
 * asserts page errors but writes new baselines instead of comparing.
 *
 * Pages render relative timestamps ("5m ago"), so callers should mask volatile
 * regions via `mask` locators — masked areas are painted over in both shots.
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const baselineDir = path.resolve(here, "../visual-baselines");
const artifactDir = path.resolve(here, "../../../../tmp/e2e-visual");

export type ScreenshotPage = {
  locator: (sel: string) => unknown;
  screenshot: (opts?: {
    mask?: unknown[];
    animations?: "disabled";
    caret?: "hide" | "initial";
    fullPage?: boolean;
  }) => Promise<Uint8Array>;
  /** Optional Playwright `evaluate` — used to wait for webfonts before capture. */
  evaluate?: (fn: () => unknown) => Promise<unknown>;
};

export type VisualBaselineOptions = {
  /** Playwright locators to mask (volatile text like relative timestamps). */
  mask?: unknown[];
  /** Fraction of pixels allowed to differ. Default 0.005 (0.5%). */
  maxDiffRatio?: number;
  /** pixelmatch color threshold. Default 0.15. */
  threshold?: number;
  /** Capture the full scroll height instead of the viewport. Default true. */
  fullPage?: boolean;
};

function updateMode(): boolean {
  try {
    const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
    return proc?.env?.OXIDEAN_E2E_UPDATE_VISUAL === "1";
  } catch {
    return false;
  }
}

function safeName(name: string): string {
  const cleaned = name.replace(/[^a-z0-9._-]+/gi, "-").replace(/^-+|-+$/g, "");
  if (!cleaned) throw new Error("visual baseline name must contain alphanumerics");
  return cleaned.toLowerCase();
}

/**
 * Screenshot `page`, compare to committed baseline `<name>.png`.
 * Throws with artifact paths when the diff exceeds `maxDiffRatio`.
 */
export async function assertVisualBaseline(
  page: ScreenshotPage,
  name: string,
  opts: VisualBaselineOptions = {},
): Promise<void> {
  const file = `${safeName(name)}.png`;
  const baselinePath = path.join(baselineDir, file);
  // SSR renders text before webfonts finish loading — wait or the screenshot
  // races the font swap and every text row ghosts vs the baseline. Scrollbar
  // metrics are also normalized away: classic-metric engines reserve layout
  // width for the app's scrollbar-gutter while overlay engines reserve none,
  // so an unmodified capture is platform-dependent and can't match one
  // committed baseline everywhere.
  if (page.evaluate) {
    try {
      await page.evaluate(() => {
        const style = document.createElement("style");
        style.textContent =
          "html{scrollbar-gutter:auto !important;overflow-y:auto !important}" +
          "::-webkit-scrollbar{display:none !important}" +
          "*{scrollbar-width:none !important}";
        document.head.appendChild(style);
        return document.fonts.ready.then(() => undefined);
      });
    } catch {
      // Non-Playwright page implementations may not support evaluate.
    }
  }
  const shot = await page.screenshot({
    mask: opts.mask,
    animations: "disabled",
    caret: "hide",
    fullPage: opts.fullPage ?? true,
  });
  const actual = PNG.sync.read(Buffer.from(shot));

  if (updateMode()) {
    mkdirSync(baselineDir, { recursive: true });
    writeFileSync(baselinePath, shot);
    return;
  }

  let expected: PNG;
  try {
    expected = PNG.sync.read(readFileSync(baselinePath));
  } catch {
    mkdirSync(artifactDir, { recursive: true });
    const actualPath = path.join(artifactDir, file);
    writeFileSync(actualPath, shot);
    throw new Error(
      `visual baseline missing: ${baselinePath}. Actual written to ${actualPath}. ` +
        `Seed with OXIDEAN_E2E_UPDATE_VISUAL=1 after reviewing the screenshot.`,
    );
  }

  if (expected.width !== actual.width || expected.height !== actual.height) {
    mkdirSync(artifactDir, { recursive: true });
    const actualPath = path.join(artifactDir, file);
    writeFileSync(actualPath, shot);
    throw new Error(
      `visual baseline size mismatch for ${file}: baseline ${expected.width}x${expected.height}, ` +
        `actual ${actual.width}x${actual.height}. Actual written to ${actualPath}.`,
    );
  }

  const diff = new PNG({ width: actual.width, height: actual.height });
  const diffPixels = pixelmatch(
    expected.data,
    actual.data,
    diff.data,
    actual.width,
    actual.height,
    { threshold: opts.threshold ?? 0.15, includeAA: false },
  );
  const ratio = diffPixels / (actual.width * actual.height);
  const max = opts.maxDiffRatio ?? 0.005;
  if (ratio > max) {
    mkdirSync(artifactDir, { recursive: true });
    const actualPath = path.join(artifactDir, file);
    const diffPath = path.join(artifactDir, `${safeName(name)}-diff.png`);
    writeFileSync(actualPath, shot);
    writeFileSync(diffPath, PNG.sync.write(diff));
    throw new Error(
      `visual baseline drift for ${file}: ${(ratio * 100).toFixed(2)}% of pixels differ ` +
        `(allowed ${(max * 100).toFixed(2)}%). Actual: ${actualPath}, diff: ${diffPath}.`,
    );
  }
}

/** Mask locators covering volatile relative-timestamp text on a page. */
export function relativeTimeMasks(page: ScreenshotPage): unknown[] {
  return [page.locator("text=/\\b(ago|just now|recently)\\b/")];
}
