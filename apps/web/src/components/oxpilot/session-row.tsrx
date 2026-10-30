import type { OxpilotSession } from "@oxidean/api-client";
import { useMutation, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Trash2 } from "@octanejs/lucide";
import { OxpilotConfirmDialog } from "@/components/oxpilot/confirm-dialog";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ListRow } from "@/components/ui/list";
import { invalidateOxpilotQueries, oxpilotSessionDeleteMutation } from "@/lib/oxpilot-queries";
import { formatRelative } from "@/lib/relative-time";

/**
 * One `/oxpilot` session row — a `listitem` inside the route's `List`.
 * The row owns its delete flow (WF9): `[Delete]` opens the shared
 * `OxpilotConfirmDialog` (focus on Cancel) → `session_delete` →
 * invalidate → `onDelete` (the parent's refresh/toast hook). The
 * affordance is `group-focus-within`-revealed — keyboard reachable,
 * never hover-only. The status badge is always text (BR10.3).
 */
export function OxpilotSessionRow(props: {
  session: OxpilotSession;
  onDelete: () => void;
}) @{
  const qc = useQueryClient();
  const s = props.session;
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const deleteM = useMutation({
    ...oxpilotSessionDeleteMutation(),
    onSuccess: () => {
      setDeleteError(null);
      invalidateOxpilotQueries(qc);
      props.onDelete();
    },
    onError: (e) => {
      setDeleteError(e instanceof Error ? e.message : "Could not delete the session.");
    },
  });
  const deleting = deleteM.isPending;

  <>
    <ListRow
      role="listitem"
      className="group flex items-center justify-between gap-3 px-4 py-3"
      data-testid="oxpilot-session-row"
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <AppLink
            href={`/oxpilot/${s.id}`}
            className="block truncate text-[14px] font-medium text-foreground hover:underline"
          >
            {s.title as string}
          </AppLink>
          <Badge variant="outline" className="shrink-0">
            {s.status as string}
          </Badge>
        </div>
        <p className="mt-0.5 truncate text-[13px] text-muted-foreground">
          {`${s.repo_owner}/${s.repo_name} · ${s.branch}` as string}
        </p>
        @if (deleteError) {
          <p role="alert" className="mt-1 text-[13px] text-destructive">
            {deleteError as string}
          </p>;
        }
      </div>
      <div className="flex shrink-0 items-center gap-3">
        <span className="text-[12px] text-muted-foreground">
          {formatRelative(s.last_activity_at) as string}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="text-destructive opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 group-focus-within:opacity-100 group-hover:opacity-100"
          aria-label={`Delete session ${s.title}`}
          disabled={deleting}
          onClick={() => setConfirmOpen(true)}
        >
          <Trash2 size={14} aria-hidden />
          Delete
        </Button>
      </div>
    </ListRow>
    <OxpilotConfirmDialog
      open={confirmOpen}
      title={`Delete session “${s.title}”?`}
      body="The transcript and every card in this session are removed. This cannot be undone."
      confirmLabel="Delete session"
      danger
      onConfirm={async () => {
        // Failure lands in `deleteError` (inline `role="alert"` on the
        // row) — swallow here so the dialog closes over the typed surface.
        await deleteM.mutateAsync({ session_id: s.id }).catch(() => {});
      }}
      onClose={() => setConfirmOpen(false)}
    />
  </>;
}
