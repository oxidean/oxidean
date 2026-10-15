/**
 * Rendering budgets for diff / blame surfaces (issue #110).
 *
 * Server payloads are already soft-capped (`DIFF_SOFT_MAX_BYTES` = 1 MiB,
 * `BLAME_SOFT_MAX_LINES` = 10k lines in `oxidean-git`), but that is still far
 * more DOM + highlighting work than a mobile client survives in one pass.
 * These budgets cap initial mount size and defer the rest behind incremental
 * batches and explicit "Load diff" gates, GitHub-style.
 */

/** Files mounted per batch on commit / compare / pull diff lists. */
export const DIFF_FILES_PAGE_SIZE = 10;

/**
 * Per-file patch size above which the diff body is not rendered by default —
 * the header + stats stay visible behind a "Load diff" button instead
 * (~16k chars ≈ 300–400 diff lines ≈ ~1.5k DOM nodes once expanded).
 */
export const DIFF_LARGE_PATCH_CHARS = 16_000;

/** Blame rows mounted per batch on the blame page. */
export const BLAME_LINES_PAGE_SIZE = 400;

/**
 * Files eagerly pre-highlighted in the page loader (commit / compare). The
 * rest highlight lazily when the file scrolls near the viewport — the eager
 * pass is a synchronous Shiki burst on the main thread, so it stays tiny.
 */
export const SSR_DIFF_HIGHLIGHT_MAX_FILES = 3;
