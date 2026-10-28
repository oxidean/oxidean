import type { OxpilotTargetView } from "@oxidean/api-client";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Monitor } from "@octanejs/lucide";
import { OxpilotConfirmDialog } from "@/components/oxpilot/confirm-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ListRow } from "@/components/ui/list";
import {
  invalidateOxpilotQueries,
  oxpilotGrantListQuery,
  oxpilotGrantRespondMutation,
  oxpilotTargetDisconnectMutation,
  oxpilotTargetRevokeMutation,
} from "@/lib/oxpilot-queries";

/**
 * Wire `lifecycle` (`{pending_consent, connected, disconnected, revoked}`)
 * → the pinned text label. Status is always text, never color-only
 * (BR1.3/BR10.3); `connected·busy` is a client-derived hint the caller
 * supplies from in-flight exec/fs tool cards — never a wire state.
 */
function lifecycleLabel(t: OxpilotTargetView, busy: boolean): string {
  switch (t.lifecycle) {
    case "pending_consent":
      return "needs consent";
    case "connected":
      return busy ? "connected·busy" : "connected·idle";
    case "disconnected":
      return "offline";
    case "revoked":
      return "revoked";
    default:
      return t.status;
  }
}

/**
 * One `Computer use` target row — a `listitem` in the route's `List`.
 * The row owns its procs (WF9): `[Disconnect]`/`[Revoke]` ride the shared
 * `OxpilotConfirmDialog`; the Manage expander lists pending `connect`
 * grants with `[Allow]`/`[Deny]` → `grant_respond` (a `pending_consent`
 * row's `[Connect]` re-offers consent by opening that expander — BR7.4).
 * Status text via `role="status"`; every button is `aria-label`led with
 * the target's name. There is no `pairing` row state — rows mint already
 * `pending_consent`. `onManage`/`onConnect`/`onDisconnect`/`onRevoke`
 * are the parent's cache-refresh hooks, fired after success.
 */
export function OxpilotTargetRow(props: {
  target: OxpilotTargetView;
  onDisconnect: () => void;
  onRevoke: () => void;
  onManage: () => void;
  onConnect: () => void;
  /** Optional caller-derived `connected·busy` hint (in-flight tool calls). */
  busy?: boolean;
}) @{
  const qc = useQueryClient();
  const t = props.target;
  const [expanded, setExpanded] = useState(false);
  const [confirm, setConfirm] = useState<"disconnect" | "revoke" | null>(null);
  const [rowError, setRowError] = useState<string | null>(null);

  const grantsQ = useQuery(oxpilotGrantListQuery({ target_id: t.id }, expanded));
  const pendingGrants = (grantsQ.data ?? []).filter(
    (g) => g.kind === "connect" && g.state === "pending",
  );

  const respondM = useMutation({
    ...oxpilotGrantRespondMutation(),
    onSuccess: () => {
      setRowError(null);
      invalidateOxpilotQueries(qc);
      props.onManage();
    },
    onError: (e) => {
      setRowError(e instanceof Error ? e.message : "Could not respond to the request.");
    },
  });
  const disconnectM = useMutation({
    ...oxpilotTargetDisconnectMutation(),
    onSuccess: () => {
      setRowError(null);
      invalidateOxpilotQueries(qc);
      props.onDisconnect();
    },
    onError: (e) => {
      setRowError(e instanceof Error ? e.message : "Could not disconnect the target.");
    },
  });
  const revokeM = useMutation({
    ...oxpilotTargetRevokeMutation(),
    onSuccess: () => {
      setRowError(null);
      invalidateOxpilotQueries(qc);
      props.onRevoke();
    },
    onError: (e) => {
      setRowError(e instanceof Error ? e.message : "Could not revoke the target.");
    },
  });
  const busy = respondM.isPending || disconnectM.isPending || revokeM.isPending;

  function toggleManage() {
    setExpanded((prev) => !prev);
    props.onManage();
  }

  function connect() {
    // BR7.4 — "Connect" on a needs-consent row re-offers consent by
    // opening the Manage expander's pending-grant block.
    setExpanded(true);
    props.onConnect();
  }

  async function runConfirm() {
    // Failure lands in `rowError` — swallow so the dialog closes over
    // the typed inline surface.
    if (confirm === "disconnect") {
      await disconnectM.mutateAsync({ target_id: t.id }).catch(() => {});
    } else if (confirm === "revoke") {
      await revokeM.mutateAsync({ target_id: t.id }).catch(() => {});
    }
  }

  const label = lifecycleLabel(t, props.busy === true);
  const revoked = t.lifecycle === "revoked";

  <>
    <ListRow
      role="listitem"
      className="flex-col items-stretch gap-0 px-4 py-3"
      data-testid="oxpilot-target-row"
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Monitor size={16} aria-hidden className="shrink-0 text-muted-foreground" />
            <p className="truncate text-[14px] font-medium text-foreground">{t.label as string}</p>
            <span role="status" className="shrink-0 text-[12px] text-muted-foreground">
              {label as string}
            </span>
          </div>
          @if (t.granted_caps.length > 0) {
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              @for (const cap of t.granted_caps; key cap) {
                <Badge variant="outline">{cap as string}</Badge>;
              }
            </div>;
          }
        </div>
        <div className="flex shrink-0 items-center gap-2">
          @if (t.lifecycle === "pending_consent") {
            <Button
              type="button"
              variant="outline"
              size="sm"
              aria-label={`Connect ${t.label}`}
              onClick={() => connect()}
            >
              Connect
            </Button>;
          }
          @if (!revoked) {
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label={`Manage ${t.label}`}
              aria-expanded={expanded}
              onClick={() => toggleManage()}
            >
              Manage
            </Button>;
          }
          @if (!revoked) {
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-destructive hover:bg-destructive/10 hover:text-destructive"
              aria-label={`Revoke ${t.label}`}
              onClick={() => setConfirm("revoke")}
            >
              Revoke
            </Button>;
          }
        </div>
      </div>
      @if (rowError) {
        <p role="alert" className="mt-2 text-[13px] text-destructive">
          {rowError as string}
        </p>;
      }
      @if (expanded) {
        <div className="mt-3 rounded-md border border-border bg-muted/30 p-3">
          <p className="text-[13px] font-medium text-foreground">{t.label as string}</p>
          @if (t.granted_caps.length > 0) {
            <p className="mt-1 text-[12px] text-muted-foreground">
              {`Granted capabilities: ${t.granted_caps.join(", ")}` as string}
            </p>;
          } @else {
            <p className="mt-1 text-[12px] text-muted-foreground">
              No capabilities granted yet.
            </p>;
          }
          @if (grantsQ.isPending) {
            <p className="mt-2 text-[13px] text-muted-foreground">Loading requests…</p>;
          } @else {
            @if (grantsQ.isError) {
              <div className="mt-2 flex items-center gap-2">
                <p role="alert" className="text-[13px] text-destructive">
                  Could not load consent requests.
                </p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void grantsQ.refetch()}
                >
                  Retry
                </Button>
              </div>;
            } @else {
              @if (pendingGrants.length === 0) {
                <p className="mt-2 text-[13px] text-muted-foreground">
                  No pending consent requests.
                </p>;
              } @else {
                <div className="mt-2 flex flex-col gap-2">
                  @for (const g of pendingGrants; key g.id) {
                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-card px-3 py-2">
                      <p className="text-[13px] text-foreground">
                        {
                          (`Consent to connect ${t.label}` +
                            (g.capability ? ` (${g.capability})` : "")) as string
                        }
                      </p>
                      <div className="flex items-center gap-2">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          aria-label={`Allow ${t.label} to connect`}
                          disabled={busy}
                          onClick={() =>
                            respondM.mutate({ grant_id: g.id, allow: true })
                          }
                        >
                          Allow
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          aria-label={`Deny ${t.label} connecting`}
                          disabled={busy}
                          onClick={() =>
                            respondM.mutate({ grant_id: g.id, allow: false })
                          }
                        >
                          Deny
                        </Button>
                      </div>
                    </div>;
                  }
                </div>;
              }
            }
          }
          @if (t.lifecycle === "connected") {
            <div className="mt-3 border-t border-border pt-3">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                aria-label={`Disconnect ${t.label}`}
                onClick={() => setConfirm("disconnect")}
              >
                Disconnect
              </Button>
            </div>;
          }
        </div>;
      }
    </ListRow>
    <OxpilotConfirmDialog
      open={confirm === "disconnect"}
      title={`Disconnect ${t.label}?`}
      body="The connector link drops until you pair again; its pending requests cancel."
      confirmLabel="Disconnect"
      danger
      onConfirm={runConfirm}
      onClose={() => setConfirm(null)}
    />
    <OxpilotConfirmDialog
      open={confirm === "revoke"}
      title={`Revoke ${t.label}?`}
      body="This permanently ends the target and every pending request on it. Pair again to reconnect."
      confirmLabel="Revoke"
      danger
      onConfirm={runConfirm}
      onClose={() => setConfirm(null)}
    />
  </>;
}
