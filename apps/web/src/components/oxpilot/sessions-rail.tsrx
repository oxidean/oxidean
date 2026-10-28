import type { OxpilotSession, OxpilotTargetView } from "@oxidean/api-client";
import { Menu } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Same lifecycle → text map as the target row (status is always text). */
function targetStatusLabel(t: OxpilotTargetView): string {
  switch (t.lifecycle) {
    case "pending_consent":
      return "needs consent";
    case "connected":
      return "connected";
    case "disconnected":
      return "offline";
    case "revoked":
      return "revoked";
    default:
      return t.status;
  }
}

/**
 * WF11 sessions rail — the left `nav` pane of the workspace. The one
 * toggle is responsive: `<768px` it renders the `[☰ Sessions]`
 * disclosure; `≥768px` the `[‹]`/`[›]` collapse control — both carry
 * `aria-expanded` on the same `data-testid="oxpilot-rail-toggle"`
 * button. Body = session list (active row highlighted via
 * `aria-current`) + the target status footer (`●N targets` + name +
 * status text chip — AC4.5.1).
 */
export function SessionsRail(props: {
  sessionId: string;
  sessions: OxpilotSession[];
  targets: OxpilotTargetView[];
  collapsed: boolean;
  onToggle: () => void;
}) @{
  const expanded = !props.collapsed;

  <nav aria-label="Sessions" className="w-60 shrink-0 max-md:w-full">
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="w-full justify-start gap-2 md:w-auto"
      aria-expanded={expanded}
      aria-controls="oxpilot-rail-panel"
      data-testid="oxpilot-rail-toggle"
      onClick={() => props.onToggle()}
    >
      <Menu size={14} aria-hidden className="md:hidden" />
      <span aria-hidden className="hidden md:inline">
        {(props.collapsed ? "›" : "‹") as string}
      </span>
      Sessions
    </Button>
    @if (expanded) {
      <div id="oxpilot-rail-panel" className="mt-3 flex flex-col gap-4">
        @if (props.sessions.length === 0) {
          <p className="text-[13px] text-muted-foreground">No sessions yet.</p>;
        } @else {
          <ul className="flex flex-col gap-1">
            @for (const s of props.sessions; key s.id) {
              <li>
                <AppLink
                  href={`/oxpilot/${s.id}`}
                  aria-current={s.id === props.sessionId ? "page" : undefined}
                  className={cn(
                    "block truncate rounded-md px-2 py-1.5 text-[13px]",
                    s.id === props.sessionId
                      ? "bg-muted font-medium text-foreground"
                      : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                  )}
                >
                  {s.title as string}
                </AppLink>
              </li>;
            }
          </ul>;
        }
        <footer className="border-t border-border pt-3">
          <p className="text-[12px] font-medium text-muted-foreground">
            {`●${props.targets.length} targets` as string}
          </p>
          @if (props.targets.length > 0) {
            <ul className="mt-1.5 flex flex-col gap-1">
              @for (const t of props.targets; key t.id) {
                <li className="flex items-center justify-between gap-2 text-[12px]">
                  <span className="truncate text-foreground">
                    {t.label as string}
                  </span>
                  <span role="status" className="shrink-0 text-muted-foreground">
                    {targetStatusLabel(t) as string}
                  </span>
                </li>;
              }
            </ul>;
          }
        </footer>
      </div>;
    }
  </nav>;
}
