import type { WebhookPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { WebhookDeliveries } from "@/components/repo/webhook-deliveries";
import { WebhookForm } from "@/components/repo/webhook-form";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function webhooksQueryKey(owner: string, name: string) {
  return ["webhook", "list", owner, name] as const;
}

/** Repo Settings Webhooks section — Admin-only CRUD + deliveries (HOOK-01/03, D-HOOK-19..21). */
export function WebhooksPanel(props: {
  owner: string;
  name: string;
  /** Server Capability Admin — parent should only mount when true (D-HOOK-02). */
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const listQ = useQuery({
    queryKey: webhooksQueryKey(owner, name),
    queryFn: async () => {
      const res = await apiClient.webhook.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load webhooks.");
      return res.data;
    },
    enabled: canAdmin,
  });

  const [mode, setMode] = useState<"list" | "create" | "edit">("list");
  const [editing, setEditing] = useState<WebhookPublic | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<WebhookPublic | null>(null);
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  const [rowBusy, setRowBusy] = useState("");
  const [rowError, setRowError] = useState("");

  const hooks: WebhookPublic[] = listQ.data?.webhooks ?? [];

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: webhooksQueryKey(owner, name) });
  }

  async function toggleActive(hook: WebhookPublic, next: boolean) {
    if (!canAdmin || next === hook.active) return;
    setRowError("");
    setRowBusy(hook.id);
    try {
      const res = await apiClient.webhook.update({
        owner,
        name,
        id: hook.id,
        active: next,
      });
      if (!res.ok) {
        setRowError(res.error.message || "Could not update webhook.");
        setRowBusy("");
        return;
      }
      await refresh();
      toastSuccess(next ? "Webhook activated." : "Webhook deactivated.");
      setRowBusy("");
    } catch {
      setRowError(NETWORK_ERROR);
      setRowBusy("");
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleteError("");
    setDeletePending(true);
    try {
      const res = await apiClient.webhook.delete({
        owner,
        name,
        id: deleteTarget.id,
      });
      if (!res.ok) {
        setDeleteError(res.error.message || "Could not delete webhook.");
        setDeletePending(false);
        return;
      }
      if (selectedId === deleteTarget.id) setSelectedId(null);
      setDeleteTarget(null);
      setDeletePending(false);
      toastSuccess("Webhook deleted.");
      await refresh();
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  function onSaved(_hook: WebhookPublic) {
    setMode("list");
    setEditing(null);
    void refresh();
  }

  @if (!canAdmin) {
    <></>;
  } @else {
    <>
      <section
        className="mt-12 flex flex-col gap-4"
        aria-labelledby="webhooks-heading"
        data-testid="repo-webhooks-settings"
      >
        <h2
          id="webhooks-heading"
          className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
        >
          Webhooks
        </h2>
        <p className="text-[14px] text-muted-foreground">
          Deliver signed HTTPS payloads for repository events (push, pull requests, issues, issue
          comments, pull request review comments, releases, stars, forks, ref changes, workflow
          runs, packages).
        </p>

        @if (mode === "create") {
          <WebhookForm
            owner={owner}
            name={name}
            onCancel={() => setMode("list")}
            onSaved={onSaved}
          />;
        } @else {
          @if (mode === "edit" && editing) {
            <WebhookForm
              owner={owner}
              name={name}
              initial={editing}
              onCancel={() => {
                setMode("list");
                setEditing(null);
              }}
              onSaved={onSaved}
            />;
          } @else {
            <>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-[14px] text-muted-foreground">
                  {
                    (hooks.length === 0
                      ? "No webhooks yet."
                      : `${hooks.length} webhook${hooks.length === 1 ? "" : "s"}`) as string
                  }
                </p>
                <Button
                  type="button"
                  data-testid="webhook-add"
                  onClick={() => {
                    setMode("create");
                    setEditing(null);
                  }}
                >
                  Add webhook
                </Button>
              </div>

              @if (rowError) {
                <p role="alert" className="text-[14px] text-destructive">
                  {rowError as string}
                </p>;
              }

              @if (listQ.isPending) {
                <p className="text-[14px] text-muted-foreground">Loading webhooks…</p>;
              } @else {
                @if (listQ.isError) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {
                      (listQ.error instanceof Error
                        ? listQ.error.message
                        : "Could not load webhooks.") as string
                    }
                  </p>;
                } @else {
                  @if (hooks.length > 0) {
                    <ul className="flex flex-col gap-3" data-testid="webhook-list">
                      @for (const hook of hooks; key hook.id) {
                        <li className="rounded-md border border-border p-4">
                          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                            <button
                              type="button"
                              className="min-w-0 flex-1 text-left"
                              onClick={() =>
                                setSelectedId(
                                  selectedId === hook.id ? null : hook.id,
                                )
                              }
                              aria-expanded={selectedId === hook.id}
                            >
                              <p className="truncate font-mono text-[14px] text-foreground">
                                {hook.url as string}
                              </p>
                              <p className="mt-1 text-[13px] text-muted-foreground">
                                {
                                  (hook.name
                                    ? `${hook.name} · ${hook.events.join(", ")}`
                                    : hook.events.join(", ")) as string
                                }
                              </p>
                              <p className="mt-1 text-[12px] text-muted-foreground">
                                Secret: {hook.secret_masked as string}
                              </p>
                            </button>
                            <div className="flex flex-wrap items-center gap-2">
                              <div className="flex items-center gap-2">
                                <Switch
                                  checked={hook.active}
                                  disabled={rowBusy === hook.id}
                                  aria-label="Active"
                                  onCheckedChange={(v) =>
                                    void toggleActive(hook, !!v)
                                  }
                                />
                                <span className="text-[13px] text-muted-foreground">
                                  {
                                    (hook.active
                                      ? "Active"
                                      : "Inactive") as string
                                  }
                                </span>
                              </div>
                              <Button
                                type="button"
                                variant="secondary"
                                onClick={() => {
                                  setEditing(hook);
                                  setMode("edit");
                                }}
                              >
                                Edit
                              </Button>
                              <Button
                                type="button"
                                variant="destructive"
                                onClick={() => {
                                  setDeleteError("");
                                  setDeleteTarget(hook);
                                }}
                              >
                                Delete
                              </Button>
                            </div>
                          </div>
                          @if (
                            selectedId === hook.id
                          ) {
                            <WebhookDeliveries
                              owner={owner}
                              name={name}
                              webhookId={hook.id}
                            />;
                          }
                        </li>;
                      }
                    </ul>;
                  }
                }
              }
            </>;
          }
        }
      </section>

      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(open) => {
          if (deletePending) return;
          if (!open) {
            setDeleteTarget(null);
            setDeleteError("");
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete webhook?</AlertDialogTitle>
            <AlertDialogDescription>
              Stops new deliveries for this endpoint. In-flight attempts may still finish.
            </AlertDialogDescription>
          </AlertDialogHeader>
          @if (deleteTarget) {
            <p className="truncate font-mono text-[13px] text-muted-foreground">
              {deleteTarget.url as string}
            </p>;
          }
          @if (deleteError) {
            <p role="alert" className="text-[14px] text-destructive">
              {deleteError as string}
            </p>;
          }
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deletePending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={deletePending}
              onClick={() => void confirmDelete()}
            >
              {(deletePending ? "Deleting…" : "Delete webhook") as string}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>;
  }
}
