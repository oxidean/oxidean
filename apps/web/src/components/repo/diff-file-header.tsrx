import { useMemo } from "octane";
import { ChevronDown, ChevronRight } from "@octanejs/lucide";
import { diffPatchStatCounts } from "@/lib/parse-unified-diff";

/**
 * Collapsible diff-file header (issue #110): chevron toggle + path + status +
 * `+a/−d` stats — shared by the unified and split diff views.
 */
export function DiffFileHeader(props: {
  path: string;
  status: string;
  patch: string;
  open: boolean;
  onToggle: () => void;
  /** id of the collapsible diff-body region (`aria-controls`). */
  controlsId: string;
}) @{
  // O(patch size) scan — memoize so header re-renders don't re-scan a gated
  // 1 MiB patch on every parent update / toggle.
  const [adds, dels] = useMemo(() => diffPatchStatCounts(props.patch), [props.patch]);

  <div className="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-2">
    <button
      type="button"
      className="inline-flex shrink-0 items-center text-muted-foreground hover:text-foreground"
      aria-expanded={props.open}
      aria-controls={props.controlsId}
      aria-label={`Toggle diff for ${props.path}`}
      onClick={() => props.onToggle()}
    >
      @if (props.open) {
        <ChevronDown aria-hidden size={14} />;
      } @else {
        <ChevronRight aria-hidden size={14} />;
      }
    </button>
    <h3 className="min-w-0 flex-1 truncate font-mono text-[14px] text-foreground">
      {props.path as string}{" "}
      <span className="text-muted-foreground">({props.status as string})</span>
    </h3>
    @if (adds + dels > 0) {
      <span className="shrink-0 tabular-nums text-[13px]">
        <span className="text-success">{`+${adds}` as string}</span>{" "}
        <span className="text-destructive">{`−${dels}` as string}</span>
      </span>;
    }
  </div>;
}

/**
 * GitHub-style "large diffs are not rendered by default" gate (issue #110) —
 * the file header + stats stay visible; the body mounts only on request.
 */
export function DiffLargeGate(props: { onLoad: () => void }) @{
  <div className="px-3 py-3">
    <p className="text-[14px] text-muted-foreground">This diff is too large to show by default.</p>
    <button
      type="button"
      className="mt-2 rounded-md border border-border px-2 py-1 text-[13px] text-foreground hover:bg-muted"
      onClick={() => props.onLoad()}
    >
      Load diff
    </button>
  </div>;
}
