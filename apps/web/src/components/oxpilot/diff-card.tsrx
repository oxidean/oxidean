import type { OxpilotProposedDiff } from "@oxidean/api-client";
import { useMemo, useState } from "octane";
import { Check, Copy, GitPullRequest } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import type { DiffLineKind } from "@/lib/parse-unified-diff";
import { cn } from "@/lib/utils";

function lineClass(kind: DiffLineKind): string {
  switch (kind) {
    case "add":
      return "bg-[var(--diff-add-bg)]";
    case "del":
      return "bg-[var(--diff-del-bg)]";
    case "hunk":
      return "bg-[var(--diff-hunk-bg)] text-[var(--diff-hunk-fg)]";
    case "meta":
    case "note":
      return "text-muted-foreground";
    case "ctx":
    default:
      return "text-foreground";
  }
}

function prefixClass(kind: DiffLineKind): string {
  switch (kind) {
    case "add":
      return "text-[var(--diff-add-fg)]";
    case "del":
      return "text-[var(--diff-del-fg)]";
    default:
      return "text-muted-foreground";
  }
}

/**
 * One file's proposed-diff rows — `useLazyDiffRows` produces escaped
 * `contentHtml` (plain rows first, Shiki upgrade near the viewport). The
 * single `dangerouslySetInnerHTML` here consumes ONLY that allowlisted
 * pipeline output (SEC-2 sink #2).
 */
export function OxpilotDiffFileRows(props: { path: string; patch: string }) @{
  const { rows, rootRef } = useLazyDiffRows({
    patch: props.patch,
    path: props.path,
    open: true,
  });
  const keyed = useMemo(() => rows.map((line, index) => ({ ...line, index })), [rows]);

  <div ref={rootRef} className="border-t border-border first:border-t-0">
    <p className="bg-muted/40 px-3 py-1.5 font-mono text-[12px] font-medium text-foreground">
      {props.path as string}
    </p>
    @if (keyed.length === 0) {
      <p className="px-3 py-2 text-[13px] text-muted-foreground">
        {
          (props.patch.trim()
            ? "Rendering diff…"
            : "No textual patch for this file.") as string
        }
      </p>;
    } @else {
      <div
        className="diff-code overflow-x-auto font-mono text-[12px] leading-[1.5]"
        role="table"
        aria-label={`Diff for ${props.path}`}
      >
        @for (const line of keyed; key line.index) {
          <div
            className={cn("flex whitespace-pre py-0.5", lineClass(line.kind) as string)}
            role="row"
            data-diff-kind={line.kind as string}
          >
            @if (line.prefix) {
              <span
                aria-hidden="true"
                className={cn(
                  "shrink-0 select-none pl-3",
                  prefixClass(line.kind) as string,
                )}
              >
                {line.prefix as string}
              </span>;
            } @else {
              <span aria-hidden="true" className="shrink-0 select-none pl-3" />;
            }
            {/* oxlint-disable: allowlisted sink — escaped/highlighted diff
                contentHtml from useLazyDiffRows only (SEC-2). */}
            <span
              className="min-w-0 grow pr-3"
              dangerouslySetInnerHTML={{ __html: line.contentHtml as string }}
            />
          </div>;
        }
      </div>;
    }
  </div>;
}

/**
 * Proposed-diff card — title + base + status, per-file highlighted rows
 * ("View diff"), the raw unified patch in a copyable `<pre>` ("Show
 * patch"), and `[Create branch + PR]` → `onApply` — rendered ONLY while
 * `diff.status === "pending"` and always routed through the caller's
 * explicit confirm (SEC-12; the card itself never calls `diff_apply`).
 * `onView` overrides the default inline-expand behavior (e.g. to open
 * the context drawer). Applied diffs show their `pr_ref` link instead.
 */
export function OxpilotDiffCard(props: {
  diff: OxpilotProposedDiff;
  onView?: () => void;
  onApply?: () => void;
}) @{
  const d = props.diff;
  const [showDiff, setShowDiff] = useState(false);
  const [showPatch, setShowPatch] = useState(false);
  const [copied, setCopied] = useState(false);

  const fullPatch = useMemo(() => d.files.map((f) => f.patch).join("\n"), [d.files]);
  const pending = d.status === "pending";
  const prHref = useMemo(() => {
    const ref = d.pr_ref;
    if (!ref) return null;
    const hash = ref.indexOf("#");
    if (hash === -1) return null;
    const repo = ref.slice(0, hash);
    const n = ref.slice(hash + 1);
    return `/${repo}/pull/${n}`;
  }, [d.pr_ref]);

  function viewDiff() {
    if (props.onView) props.onView();
    else setShowDiff((v) => !v);
  }

  async function copyPatch() {
    try {
      await navigator.clipboard.writeText(fullPatch);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via text selection */
    }
  }

  <div className="rounded-md border border-border bg-card" data-testid="oxpilot-diff-card">
    <div className="flex items-start justify-between gap-3 px-3 py-2.5">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <GitPullRequest size={14} aria-hidden className="shrink-0 text-muted-foreground" />
          <p className="truncate text-[14px] font-medium text-foreground">{d.title as string}</p>
          <Badge variant="outline" className="shrink-0 font-normal">
            {d.status as string}
          </Badge>
        </div>
        <p className="mt-0.5 text-[12px] text-muted-foreground">
          {
            `Proposed on ${d.base} · ${d.files.length} file${d.files.length === 1 ? "" : "s"}` as string
          }
        </p>
      </div>
    </div>
    <div className="flex flex-wrap items-center gap-2 border-t border-border px-3 py-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        aria-expanded={props.onView ? undefined : showDiff}
        onClick={() => viewDiff()}
      >
        {(!props.onView && showDiff ? "Hide diff" : "View diff") as string}
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        aria-expanded={showPatch}
        onClick={() => setShowPatch((v) => !v)}
      >
        {(showPatch ? "Hide patch" : "Show patch") as string}
      </Button>
      @if (pending && props.onApply) {
        <Button type="button" variant="outline" size="sm" onClick={() => props.onApply?.()}>
          Create branch + PR
        </Button>;
      }
      @if (!pending && d.pr_ref && prHref) {
        <AppLink
          href={prHref}
          className="text-[13px] font-medium text-foreground underline underline-offset-2"
        >
          {d.pr_ref as string}
        </AppLink>;
      } @else {
        @if (!pending && d.pr_ref) {
          <span className="text-[13px] text-foreground">{d.pr_ref as string}</span>;
        }
      }
    </div>
    @if (!props.onView && showDiff) {
      <div className="border-t border-border">
        @for (const f of d.files; key f.path) {
          <OxpilotDiffFileRows path={f.path} patch={f.patch} />;
        }
      </div>;
    }
    @if (showPatch) {
      <div className="border-t border-border p-3">
        <div className="mb-2 flex justify-end">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label="Copy unified patch"
            onClick={() => void copyPatch()}
          >
            {copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
            {(copied ? "Copied" : "Copy") as string}
          </Button>
        </div>
        <pre className="max-h-96 overflow-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-[12px] leading-[1.5] text-foreground">
          <code>{fullPatch as string}</code>
        </pre>
      </div>;
    }
  </div>;
}
