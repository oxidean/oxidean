import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { OxpilotSession, OxpilotTargetView } from "@oxidean/api-client";
import { useState } from "octane";
import { Bot, Cable, MessageSquarePlus } from "@octanejs/lucide";
import { AppPage } from "@/components/app-page";
import { ConnectDialog } from "@/components/oxpilot/connect-dialog";
import { OxpilotConfirmDialog } from "@/components/oxpilot/confirm-dialog";
import { OxpilotFlagOff } from "@/components/oxpilot/flag-off";
import { NewSessionDialog } from "@/components/oxpilot/new-session-dialog";
import { OxpilotSessionRow } from "@/components/oxpilot/session-row";
import { OxpilotTargetRow } from "@/components/oxpilot/target-row";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { usePageRedirect } from "@/lib/nav.tsrx";
import {
  invalidateOxpilotQueries,
  oxpilotSessionListQueryOptions,
  oxpilotStatusQueryOptions,
  oxpilotTargetDisconnectMutation,
  oxpilotTargetListQuery,
  oxpilotTargetRevokeMutation,
} from "@/lib/oxpilot-queries";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { toastSuccess } from "@/lib/toast";

/** `/oxpilot` — session list + computer-use targets; flag-off explanatory state. */
export function OxpilotPage() @{
  <AppPage>
    <OxpilotContent />
  </AppPage>;
}

/** Section-scoped query failure — typed inline notice + `[Retry]` (M8). */
function SectionError(props: { message: string; onRetry: () => void }) @{
  <div
    className="mt-3 flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3"
    role="alert"
  >
    <p className="text-[14px] text-destructive">{props.message as string}</p>
    <Button type="button" variant="outline" size="sm" onClick={props.onRetry}>
      Retry
    </Button>
  </div>;
}

function OxpilotContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/oxpilot" : null);
  const qc = useQueryClient();
  const statusQ = useQuery(oxpilotStatusQueryOptions());
  const enabled = statusQ.data?.enabled === true;
  const sessionsQ = useQuery(oxpilotSessionListQueryOptions(enabled));
  const targetsQ = useQuery(oxpilotTargetListQuery(enabled));
  const sessions: OxpilotSession[] = sessionsQ.data ?? [];
  const targets: OxpilotTargetView[] = targetsQ.data ?? [];

  const [newOpen, setNewOpen] = useState(false);
  const [connectOpen, setConnectOpen] = useState(false);

  // Target rows hand `[Disconnect]`/`[Revoke]` back to the route — the
  // shared M5 confirm + the procs live here (one success toast each;
  // failures get a typed notice and NO toast).
  const [confirm, setConfirm] = useState<{
    kind: "disconnect" | "revoke";
    target: OxpilotTargetView;
  } | null>(null);
  const [targetError, setTargetError] = useState<string | null>(null);

  function refresh() {
    invalidateOxpilotQueries(qc);
  }

  const disconnectM = useMutation({
    ...oxpilotTargetDisconnectMutation(),
    onSuccess: () => {
      setTargetError(null);
      refresh();
      toastSuccess("Target disconnected");
    },
    onError: (e) =>
      setTargetError(e instanceof Error ? e.message : "Could not disconnect the target."),
  });
  const revokeM = useMutation({
    ...oxpilotTargetRevokeMutation(),
    onSuccess: () => {
      setTargetError(null);
      refresh();
      toastSuccess("Target revoked");
    },
    onError: (e) => setTargetError(e instanceof Error ? e.message : "Could not revoke the target."),
  });

  /** Confirm-accept arm — a reject leaves the dialog open over the notice. */
  async function confirmTargetAction() {
    if (!confirm) return;
    const m = confirm.kind === "disconnect" ? disconnectM : revokeM;
    await m.mutateAsync({ target_id: confirm.target.id });
  }

  <main className="mx-auto w-full max-w-4xl px-4 py-8" data-testid="oxpilot-page">
    <PageHeader
      title="OxPilot"
      description="Agentic chat, builder, and debugger sessions bound to your repositories."
      actions={
        <Button
          type="button"
          onClick={() => setNewOpen(true)}
          data-testid="oxpilot-new-session-open"
        >
          <MessageSquarePlus size={14} aria-hidden />
          New session
        </Button>
      }
    />
    @if (statusQ.isPending || meQ.isPending) {
      <div className="mt-6 flex flex-col gap-3">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-24 w-full" />
      </div>;
    } @else {
      @if (!enabled) {
        <OxpilotFlagOff reason={statusQ.data?.reason} />;
      } @else {
        <>
          <section className="mt-8">
            <h2 className="font-[family-name:var(--font-display)] text-[16px] font-semibold text-foreground">
              Sessions
            </h2>
            @if (sessionsQ.isPending) {
              <Skeleton className="mt-3 h-32 w-full" />;
            } @else {
              @if (sessionsQ.isError) {
                <SectionError
                  message={sessionsQ.error?.message || "Could not load sessions."}
                  onRetry={() => void sessionsQ.refetch()}
                />;
              } @else {
                @if (sessions.length === 0) {
                  <div className="mt-3">
                    <EmptyState
                      icon={<Bot size={22} />}
                      title="No sessions yet"
                      description="Start a session — OxPilot streams a reply and keeps the transcript here."
                      action={
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onClick={() => setNewOpen(true)}
                        >
                          <MessageSquarePlus size={14} aria-hidden />
                          New session
                        </Button>
                      }
                    />
                  </div>;
                } @else {
                  <List className="mt-3" data-testid="oxpilot-session-list">
                    @for (const s of sessions; key s.id) {
                      <OxpilotSessionRow
                        session={s}
                        onDelete={() => {
                          refresh();
                          toastSuccess("Session deleted");
                        }}
                      />;
                    }
                  </List>;
                }
              }
            }
          </section>
          <section className="mt-8" id="targets" data-testid="oxpilot-targets">
            <div className="flex items-center justify-between gap-3">
              <h2 className="font-[family-name:var(--font-display)] text-[16px] font-semibold text-foreground">
                Computer use
              </h2>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setConnectOpen(true)}
                data-testid="oxpilot-connect-open"
              >
                <Cable size={14} aria-hidden />
                Connect a system
              </Button>
            </div>
            @if (targetError) {
              <div
                className="mt-3 flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3"
                role="alert"
              >
                <p className="text-[14px] text-destructive">{targetError as string}</p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setTargetError(null)}
                >
                  Dismiss
                </Button>
              </div>;
            }
            @if (targetsQ.isPending) {
              <Skeleton className="mt-3 h-24 w-full" />;
            } @else {
              @if (targetsQ.isError) {
                <SectionError
                  message={
                    targetsQ.error?.message || "Could not load computer-use targets."
                  }
                  onRetry={() => void targetsQ.refetch()}
                />;
              } @else {
                @if (targets.length === 0) {
                  <div className="mt-3">
                    <EmptyState
                      icon={<Cable size={22} />}
                      title="No systems connected"
                      description="Nothing is installed until you run it — pair a connector on a machine you control to grant computer-use capabilities."
                      action={
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onClick={() => setConnectOpen(true)}
                        >
                          Connect a system
                        </Button>
                      }
                    />
                  </div>;
                } @else {
                  <List className="mt-3" data-testid="oxpilot-target-list">
                    @for (const t of targets; key t.id) {
                      <OxpilotTargetRow
                        target={t}
                        onDisconnect={() =>
                          setConfirm({ kind: "disconnect", target: t })
                        }
                        onRevoke={() => setConfirm({ kind: "revoke", target: t })}
                      />;
                    }
                  </List>;
                }
              }
            }
          </section>
          <NewSessionDialog
            open={newOpen}
            onClose={() => setNewOpen(false)}
            onConnectRequest={() => {
              setNewOpen(false);
              setConnectOpen(true);
            }}
            onCreated={(created) => {
              window.location.href = `/oxpilot/${created.id}`;
            }}
          />
          <ConnectDialog
            open={connectOpen}
            onClose={() => {
              setConnectOpen(false);
              refresh();
            }}
          />
          <OxpilotConfirmDialog
            open={confirm !== null}
            title={
              (confirm?.kind === "disconnect"
                ? `Disconnect ${confirm.target.label}?`
                : `Revoke ${confirm?.target.label ?? "this target"}?`) as string
            }
            body={
              (confirm?.kind === "disconnect"
                ? "The connector goes offline; pending consent requests are cancelled."
                : "Access ends for this system — every pending grant is cancelled, including in-flight ones. This cannot be undone.") as string
            }
            confirmLabel={confirm?.kind === "disconnect" ? "Disconnect" : "Revoke"}
            danger
            onConfirm={confirmTargetAction}
            onClose={() => setConfirm(null)}
          />
        </>;
      }
    }
  </main>;
}
