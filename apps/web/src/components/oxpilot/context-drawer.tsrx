import type { OxpilotProposedDiff, OxpilotSession, OxpilotTargetView } from "@oxidean/api-client";
import { Monitor, X } from "@octanejs/lucide";
import { OxpilotDiffFileRows } from "@/components/oxpilot/diff-card";
import { lifecycleLabel } from "@/components/oxpilot/target-row";
import { Button } from "@/components/ui/button";

/** Return focus to the workspace's `[Context]` toggle after Esc. */
function focusContextToggle() {
  (document.querySelector('[data-testid="oxpilot-context-toggle"]') as HTMLElement | null)?.focus();
}

/**
 * WF11 context drawer — the right `complementary` pane. 320px inline at
 * ≥1024px, a right-edge overlay under that (`max-lg:fixed`). Esc inside
 * closes and returns focus to the route's `[Context]` toggle
 * (`oxpilot-context-toggle`). Content: repo `owner/name · branch` from
 * `session_get`, the proposed-diff file list (lazy `useLazyDiffRows`
 * via `OxpilotDiffFileRows`), and the bound computer-use target's
 * name + status text + granted caps (caller resolves the binding).
 */
export function ContextDrawer(props: {
  open: boolean;
  onClose: () => void;
  session: OxpilotSession | null;
  diff: OxpilotProposedDiff | null;
  target: OxpilotTargetView | null;
  /** Caller-derived `connected·busy` flag for the bound target. */
  targetBusy?: boolean;
}) @{
  const s = props.session;
  const d = props.diff;
  const t = props.target;

  function escClose(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    props.onClose();
    focusContextToggle();
  }

  @if (props.open) {
    <aside
      aria-label="Session context"
      className="max-h-[calc(100vh-2rem)] w-80 shrink-0 overflow-y-auto rounded-lg border border-border bg-card max-lg:fixed max-lg:inset-y-0 max-lg:right-0 max-lg:z-50 max-lg:max-h-none max-lg:rounded-none max-lg:border-y-0 max-lg:border-r-0 max-lg:shadow-xl"
      data-testid="oxpilot-drawer"
      onKeydown={escClose}
    >
      <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
        <p className="text-[14px] font-medium text-foreground">Context</p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-label="Close context drawer"
          onClick={() => {
            props.onClose();
            focusContextToggle();
          }}
        >
          <X size={14} aria-hidden />
        </Button>
      </div>
      <div className="flex flex-col gap-4 p-4">
        <section aria-label="Repository">
          <p className="text-[12px] font-medium text-muted-foreground">Repository</p>
          @if (s) {
            <>
              <p className="mt-1 text-[14px] text-foreground">
                {`${s.repo_owner}/${s.repo_name} · ${s.branch}` as string}
              </p>
              @if (s.goal) {
                <p className="mt-1 text-[13px] text-muted-foreground">
                  {s.goal as string}
                </p>;
              }
            </>;
          } @else {
            <p className="mt-1 text-[13px] text-muted-foreground">Loading…</p>;
          }
        </section>
        <section aria-label="Proposed diff">
          <p className="text-[12px] font-medium text-muted-foreground">Proposed diff</p>
          @if (d) {
            <>
              <p className="mt-1 text-[13px] text-foreground">
                {
                  `${d.title} — ${d.files.length} file${d.files.length === 1 ? "" : "s"} on ${d.base}` as string
                }
              </p>
              <div className="mt-2 overflow-hidden rounded-md border border-border">
                @for (const f of d.files; key f.path) {
                  <OxpilotDiffFileRows path={f.path} patch={f.patch} />;
                }
              </div>
            </>;
          } @else {
            <p className="mt-1 text-[13px] text-muted-foreground">No proposed diff yet.</p>;
          }
        </section>
        <section aria-label="Computer-use target">
          <p className="text-[12px] font-medium text-muted-foreground">Computer-use target</p>
          @if (t) {
            <>
              <div className="mt-1 flex items-center gap-2">
                <Monitor size={14} aria-hidden className="shrink-0 text-muted-foreground" />
                <p className="truncate text-[14px] text-foreground">{t.label as string}</p>
                <span role="status" className="shrink-0 text-[12px] text-muted-foreground">
                  {lifecycleLabel(t, props.targetBusy === true) as string}
                </span>
              </div>
              @if (t.granted_caps.length > 0) {
                <p className="mt-1 text-[12px] text-muted-foreground">
                  {`Granted: ${t.granted_caps.join(", ")}` as string}
                </p>;
              } @else {
                <p className="mt-1 text-[12px] text-muted-foreground">
                  No capabilities granted.
                </p>;
              }
            </>;
          } @else {
            <p className="mt-1 text-[13px] text-muted-foreground">
              No target bound to this session.
            </p>;
          }
        </section>
      </div>
    </aside>;
  }
}
