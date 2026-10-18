import type { ActionSecretMetaPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Check, ChevronDown } from "@octanejs/lucide";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectList,
  SelectPopup,
  SelectPortal,
  SelectPositioner,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { ActionsDeployments } from "@/components/repo/actions-deployments";
import { ActionsEnvironmentsPanel } from "@/components/repo/actions-environments-panel";
import { apiClient } from "@/lib/api-client";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function secretsQueryKey(owner: string, name: string) {
  return ["repo", "actions", "secrets", owner, name] as const;
}

function enabledQueryKey(owner: string, name: string) {
  return ["repo", "actions", "getEnabled", owner, name] as const;
}

const FORK_PR_OPTIONS = [
  { value: "never", label: "Never require approval" },
  { value: "first-time", label: "Require approval for first-time contributors" },
  { value: "always", label: "Require approval for all outside collaborators" },
] as const;

const DEFAULT_PERM_OPTIONS = [
  { value: "read", label: "Read repository contents and packages" },
  { value: "write", label: "Read and write permissions" },
] as const;

/** Repo Settings Actions enable + secrets CRUD (D-ACT-06 / D-ACT-17 / D-ACT-10). */
export function ActionsSettingsPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const enabledQ = useQuery({
    queryKey: enabledQueryKey(owner, name),
    queryFn: async () => {
      const res = await apiClient.repo.actions.getEnabled({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load Actions status.");
      return res.data.enabled;
    },
  });

  const secretsQ = useQuery({
    queryKey: secretsQueryKey(owner, name),
    queryFn: async (): Promise<ActionSecretMetaPublic[]> => {
      const res = await apiClient.repo.actions.secrets.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load secrets.");
      return res.data.secrets;
    },
    enabled: canAdmin,
  });

  const [togglePending, setTogglePending] = useState(false);
  const [secretName, setSecretName] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [secretPending, setSecretPending] = useState(false);
  const [error, setError] = useState("");

  const forkPrQ = useQuery({
    queryKey: ["repo", "actions", "forkPrApproval", owner, name] as const,
    queryFn: async () => {
      const res = await apiClient.repo.actions.getForkPrApproval({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load fork approval policy.");
      return res.data.policy;
    },
    enabled: canAdmin,
  });
  const prTargetQ = useQuery({
    queryKey: ["repo", "actions", "pullRequestTarget", owner, name] as const,
    queryFn: async () => {
      const res = await apiClient.repo.actions.getPullRequestTarget({ owner, name });
      if (!res.ok)
        throw new Error(res.error.message || "Could not load pull_request_target setting.");
      return res.data.enabled;
    },
    enabled: canAdmin,
  });
  const defaultPermsQ = useQuery({
    queryKey: ["repo", "actions", "defaultPermissions", owner, name] as const,
    queryFn: async () => {
      const res = await apiClient.repo.actions.getDefaultPermissions({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load default permissions.");
      return res.data.default_permissions;
    },
    enabled: canAdmin,
  });

  type RpcResult = { ok: boolean; error?: { message?: string } };
  async function saveSetting(
    call: () => Promise<RpcResult>,
    okMsg: string,
    key: readonly unknown[],
  ) {
    if (!canAdmin) return;
    setError("");
    try {
      const res = await call();
      if (!res.ok) {
        setError(res.error?.message || "Could not update Actions setting.");
        return;
      }
      toastSuccess(okMsg);
      await queryClient.invalidateQueries({ queryKey: key });
    } catch {
      setError(NETWORK_ERROR);
    }
  }

  async function toggleEnabled(next: boolean) {
    if (!canAdmin) return;
    setError("");
    setTogglePending(true);
    try {
      const res = await apiClient.repo.actions.setEnabled({ owner, name, enabled: next });
      if (!res.ok) {
        setError(res.error.message || "Could not update Actions setting.");
        setTogglePending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: enabledQueryKey(owner, name) });
      toastSuccess(next ? "Actions enabled." : "Actions disabled.");
      setTogglePending(false);
    } catch {
      setError(NETWORK_ERROR);
      setTogglePending(false);
    }
  }

  async function putSecret() {
    if (!canAdmin) return;
    const sn = secretName.trim();
    if (!sn || !secretValue) return;
    setError("");
    setSecretPending(true);
    try {
      const res = await apiClient.repo.actions.secrets.put({
        owner,
        name,
        secret_name: sn,
        value: secretValue,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not save secret.");
        setSecretPending(false);
        return;
      }
      setSecretName("");
      setSecretValue("");
      await queryClient.invalidateQueries({ queryKey: secretsQueryKey(owner, name) });
      toastSuccess("Secret saved. Value cannot be viewed again.");
      setSecretPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setSecretPending(false);
    }
  }

  async function deleteSecret(sn: string) {
    if (!canAdmin) return;
    setError("");
    try {
      const res = await apiClient.repo.actions.secrets.delete({
        owner,
        name,
        secret_name: sn,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete secret.");
        return;
      }
      await queryClient.invalidateQueries({ queryKey: secretsQueryKey(owner, name) });
      toastSuccess("Secret deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  const enabled = enabledQ.data ?? true;
  const secrets = secretsQ.data ?? [];

  <section
    id="actions"
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="actions-heading"
    data-testid="repo-actions-settings"
  >
    <h2
      id="actions-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Actions
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Workflows under{" "}
      <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px]">.github/workflows/</code>{" "}
      run on registered self-hosted runners. Secret values are write-only after create. Instance
      admins manage runners under Admin → Actions runners.
    </p>

    @if (enabledQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading Actions status…</p>;
    } @else {
      <div className="flex items-center gap-3">
        <Switch
          checked={enabled}
          disabled={!canAdmin || togglePending}
          onCheckedChange={(v) => void toggleEnabled(!!v)}
          data-testid="repo-actions-enabled"
        />
        <Label className="text-[14px]">Enable Actions for this repository</Label>
      </div>;
    }

    @if (canAdmin) {
      <>
        <div className="mt-4 space-y-3">
          <h3 className="text-[15px] font-semibold text-foreground">Repository secrets</h3>
          <form
            className="flex flex-wrap items-end gap-3"
            method="post"
            action="#"
            onSubmit={(e) => {
              e.preventDefault();
              void putSecret();
            }}
          >
            <label className="flex flex-col gap-1 text-[14px]">
              <span className="font-medium">Name</span>
              <Input
                data-testid="repo-actions-secret-name"
                value={secretName}
                onInput={(e) => setSecretName((e.target as HTMLInputElement).value)}
                autocomplete="off"
              />
            </label>
            <label className="flex flex-col gap-1 text-[14px]">
              <span className="font-medium">Value</span>
              <Input
                type="password"
                data-testid="repo-actions-secret-value"
                value={secretValue}
                onInput={(e) => setSecretValue((e.target as HTMLInputElement).value)}
                autocomplete="new-password"
              />
            </label>
            <Button
              type="submit"
              disabled={secretPending}
              data-testid="repo-actions-secret-save"
            >
              Save secret
            </Button>
          </form>
          <ul className="space-y-2">
            @for (const s of secrets; key s.name) {
              <li className="flex items-center justify-between gap-3 text-[14px]">
                <span className="font-mono">{s.name as string}</span>
                <Button
                  type="button"
                  variant="ghost"
                  onClick={() => void deleteSecret(s.name)}
                  data-testid={"repo-actions-secret-delete-" + s.name}
                >
                  Delete
                </Button>
              </li>;
            } @empty {
              <li className="text-muted-foreground">No secrets yet.</li>;
            }
          </ul>
        </div>
        <div className="mt-6 flex flex-col gap-4" data-testid="actions-permissions-section">
          <h3 className="text-[15px] font-semibold text-foreground">Workflow permissions</h3>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="actions-default-perms">Default GITHUB_TOKEN permissions</Label>
            <SelectRoot
              value={defaultPermsQ.data ?? "read"}
              onValueChange={(next) => {
                if (typeof next === "string") {
                  void saveSetting(
                    () =>
                      apiClient.repo.actions.setDefaultPermissions({
                        owner,
                        name,
                        default_permissions: next,
                      }),
                    "Default workflow permissions saved.",
                    ["repo", "actions", "defaultPermissions", owner, name],
                  );
                }
              }}
            >
              <SelectTrigger
                id="actions-default-perms"
                className="w-full max-w-md"
                data-testid="actions-default-perms"
              >
                <SelectValue>
                  {
                    (DEFAULT_PERM_OPTIONS.find(
                      (o) => o.value === (defaultPermsQ.data ?? "read"),
                    )?.label ?? "Read repository contents and packages") as string
                  }
                </SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={14} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
escape hatch; popup is anchor-scoped under the Select root. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      @for (const opt of DEFAULT_PERM_OPTIONS; key opt.value) {
                        <SelectItem
                          value={opt.value}
                          data-testid={"actions-default-perms-option-" + opt.value}
                        >
                          <SelectItemIndicator>
                            <Check aria-hidden size={14} className="text-primary" />
                          </SelectItemIndicator>
                          <SelectItemText>{opt.label as string}</SelectItemText>
                        </SelectItem>;
                      }
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="actions-fork-pr">Fork pull request workflows</Label>
            <SelectRoot
              value={forkPrQ.data ?? "never"}
              onValueChange={(next) => {
                if (typeof next === "string") {
                  void saveSetting(
                    () =>
                      apiClient.repo.actions.setForkPrApproval({ owner, name, policy: next }),
                    "Fork PR approval policy saved.",
                    ["repo", "actions", "forkPrApproval", owner, name],
                  );
                }
              }}
            >
              <SelectTrigger
                id="actions-fork-pr"
                className="w-full max-w-md"
                data-testid="actions-fork-pr"
              >
                <SelectValue>
                  {
                    (FORK_PR_OPTIONS.find((o) => o.value === (forkPrQ.data ?? "never"))
                      ?.label ?? "Never require approval") as string
                  }
                </SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={14} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: same SelectPortal constraint. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      @for (const opt of FORK_PR_OPTIONS; key opt.value) {
                        <SelectItem
                          value={opt.value}
                          data-testid={"actions-fork-pr-option-" + opt.value}
                        >
                          <SelectItemIndicator>
                            <Check aria-hidden size={14} className="text-primary" />
                          </SelectItemIndicator>
                          <SelectItemText>{opt.label as string}</SelectItemText>
                        </SelectItem>;
                      }
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
            <p className="text-[12px] text-muted-foreground">
              Held runs show an approval banner on the run page.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <Switch
              checked={prTargetQ.data ?? false}
              disabled={!canAdmin}
              data-testid="actions-pr-target"
              onCheckedChange={(v) =>
                void saveSetting(
                  () =>
                    apiClient.repo.actions.setPullRequestTarget({
                      owner,
                      name,
                      enabled: !!v,
                    }),
                  "pull_request_target setting saved.",
                  ["repo", "actions", "pullRequestTarget", owner, name],
                )
              }
            />
            <div className="flex flex-col">
              <Label className="text-[14px]">
                Allow workflows triggered by pull_request_target
              </Label>
              <span className="text-[12px] text-muted-foreground">
                Runs in the base-repo context for fork PRs — only enable when workflows handle
                untrusted code safely.
              </span>
            </div>
          </div>
        </div>
        <ActionsEnvironmentsPanel owner={owner} name={name} can_admin={canAdmin} />
        <div className="mt-2">
          <h3 className="mb-1 text-[15px] font-semibold text-foreground">Deployments</h3>
          <ActionsDeployments owner={owner} name={name} />
        </div>
      </>;
    }

    @if (error) {
      <p className="text-destructive" role="alert">
        {error as string}
      </p>;
    }
  </section>;
}
