import type { ActionEnvironmentPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { ChevronDown, ChevronRight, Pencil, Plus, Trash2 } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import {
  actionsEnvironmentSecretsQuery,
  actionsEnvironmentVariablesQuery,
  actionsEnvironmentsQuery,
} from "@/lib/actions-queries";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Comma-separated text ↔ string list (reviewer usernames, branch names). */
function parseList(text: string): string[] {
  return text
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

/** Per-environment secret list (name + delete + create form). */
function EnvSecrets(props: { owner: string; name: string; env: string }) @{
  const qc = useQueryClient();
  const q = useQuery(actionsEnvironmentSecretsQuery(props.owner, props.name, props.env));
  const secrets = q.data?.secrets ?? [];
  const [secretName, setSecretName] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [pending, setPending] = useState(false);
  const qk = ["repo", "actions", "env-secrets", props.owner, props.name, props.env] as const;

  async function put() {
    const sn = secretName.trim();
    if (!sn || !secretValue) return;
    setPending(true);
    try {
      const res = await apiClient.repo.actions.environments.secretsPut({
        owner: props.owner,
        name: props.name,
        environment: props.env,
        secret_name: sn,
        value: secretValue,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not save secret.");
        setPending(false);
        return;
      }
      setSecretName("");
      setSecretValue("");
      await qc.invalidateQueries({ queryKey: qk });
      toastSuccess("Environment secret saved.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function del(sn: string) {
    try {
      const res = await apiClient.repo.actions.environments.secretsDelete({
        owner: props.owner,
        name: props.name,
        environment: props.env,
        secret_name: sn,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete secret.");
        return;
      }
      await qc.invalidateQueries({ queryKey: qk });
      toastSuccess("Environment secret deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  <div className="mt-2 flex flex-col gap-2" data-testid={`env-secrets-${props.env}`}>
    <form
      className="flex flex-wrap items-end gap-2"
      method="post"
      action="#"
      onSubmit={(e) => {
        e.preventDefault();
        void put();
      }}
    >
      <Input
        value={secretName}
        onInput={(e) => setSecretName((e.target as HTMLInputElement).value)}
        placeholder="SECRET_NAME"
        className="h-8 w-44 font-mono text-[12px]"
        data-testid={`env-secret-name-${props.env}`}
      />
      <Input
        type="password"
        value={secretValue}
        onInput={(e) => setSecretValue((e.target as HTMLInputElement).value)}
        placeholder="value"
        className="h-8 w-52 text-[12px]"
        data-testid={`env-secret-value-${props.env}`}
      />
      <Button
        type="submit"
        size="sm"
        variant="outline"
        disabled={pending || !secretName.trim() || !secretValue}
        data-testid={`env-secret-save-${props.env}`}
      >
        Add secret
      </Button>
    </form>
    <ul className="flex flex-col gap-1">
      @for (const s of secrets; key s.name) {
        <li className="flex items-center gap-3 text-[13px]">
          <span className="font-mono">{s.name as string}</span>
          <button
            type="button"
            className="text-[12px] text-muted-foreground underline-offset-2 hover:text-destructive hover:underline"
            data-testid={`env-secret-delete-${props.env}-${s.name}`}
            onClick={() => void del(s.name)}
          >
            Delete
          </button>
        </li>;
      } @empty {
        <li className="text-[12px] text-muted-foreground">No environment secrets.</li>;
      }
    </ul>
  </div>;
}

/** Per-environment variable list — plaintext values, `vars.*` context. */
function EnvVariables(props: { owner: string; name: string; env: string }) @{
  const qc = useQueryClient();
  const q = useQuery(actionsEnvironmentVariablesQuery(props.owner, props.name, props.env));
  const variables = q.data?.variables ?? [];
  const [varName, setVarName] = useState("");
  const [varValue, setVarValue] = useState("");
  const [pending, setPending] = useState(false);
  const qk = ["repo", "actions", "env-variables", props.owner, props.name, props.env] as const;

  async function put() {
    const vn = varName.trim();
    if (!vn) return;
    setPending(true);
    try {
      const res = await apiClient.repo.actions.environments.variablesPut({
        owner: props.owner,
        name: props.name,
        environment: props.env,
        variable_name: vn,
        value: varValue,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not save variable.");
        setPending(false);
        return;
      }
      setVarName("");
      setVarValue("");
      await qc.invalidateQueries({ queryKey: qk });
      toastSuccess("Environment variable saved.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function del(vn: string) {
    try {
      const res = await apiClient.repo.actions.environments.variablesDelete({
        owner: props.owner,
        name: props.name,
        environment: props.env,
        variable_name: vn,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete variable.");
        return;
      }
      await qc.invalidateQueries({ queryKey: qk });
      toastSuccess("Environment variable deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  <div className="mt-3 flex flex-col gap-2" data-testid={`env-vars-${props.env}`}>
    <p className="text-[12px] font-medium text-muted-foreground">Variables (plaintext)</p>
    <form
      className="flex flex-wrap items-end gap-2"
      method="post"
      action="#"
      onSubmit={(e) => {
        e.preventDefault();
        void put();
      }}
    >
      <Input
        value={varName}
        onInput={(e) => setVarName((e.target as HTMLInputElement).value)}
        placeholder="VAR_NAME"
        className="h-8 w-44 font-mono text-[12px]"
        data-testid={`env-var-name-${props.env}`}
      />
      <Input
        value={varValue}
        onInput={(e) => setVarValue((e.target as HTMLInputElement).value)}
        placeholder="value"
        className="h-8 w-52 text-[12px]"
        data-testid={`env-var-value-${props.env}`}
      />
      <Button
        type="submit"
        size="sm"
        variant="outline"
        disabled={pending || !varName.trim()}
        data-testid={`env-var-save-${props.env}`}
      >
        Add variable
      </Button>
    </form>
    <ul className="flex flex-col gap-1">
      @for (const v of variables; key v.name) {
        <li className="flex items-center gap-3 text-[13px]">
          <span className="font-mono">
            {v.name as string}
            <span className="ml-2 font-normal text-muted-foreground">
              {v.value as string}
            </span>
          </span>
          <button
            type="button"
            className="text-[12px] text-muted-foreground underline-offset-2 hover:text-destructive hover:underline"
            data-testid={`env-var-delete-${props.env}-${v.name}`}
            onClick={() => void del(v.name)}
          >
            Delete
          </button>
        </li>;
      } @empty {
        <li className="text-[12px] text-muted-foreground">No environment variables.</li>;
      }
    </ul>
  </div>;
}

/** Editor state for one environment's protection rules. */
type EnvDraft = {
  name: string;
  wait_timer_minutes: string;
  required_reviewers: string;
  prevent_self_review: boolean;
  deployment_branches: string;
};

function draftFor(env?: ActionEnvironmentPublic): EnvDraft {
  if (!env) {
    return {
      name: "",
      wait_timer_minutes: "",
      required_reviewers: "",
      prevent_self_review: false,
      deployment_branches: "",
    };
  }
  const p = env.protection;
  const reviewers =
    p.required_reviewer_usernames && p.required_reviewer_usernames.length > 0
      ? p.required_reviewer_usernames
      : (p.required_reviewers ?? []);
  return {
    name: env.name,
    wait_timer_minutes: p.wait_timer_minutes ? String(p.wait_timer_minutes) : "",
    required_reviewers: reviewers.join(", "),
    prevent_self_review: p.prevent_self_review === true,
    deployment_branches: (p.deployment_branches ?? []).join(", "),
  };
}

/** Environment protection editor form (create + update share the shape). */
function EnvEditor(props: {
  owner: string;
  name: string;
  initial: EnvDraft;
  fixedName?: string;
  busy: boolean;
  onSave: (d: EnvDraft) => void;
  onCancel: () => void;
}) @{
  const [draft, setDraft] = useState<EnvDraft>(props.initial);
  const [envErr, setEnvErr] = useState("");

  async function save() {
    if (!draft.name.trim()) {
      setEnvErr("Environment name is required.");
      return;
    }
    setEnvErr("");
    props.onSave(draft);
  }

  <form
    className="mt-2 flex flex-col gap-3 rounded-md border border-border bg-muted/20 p-3"
    method="post"
    action="#"
    data-testid={`env-editor-${props.fixedName ?? "new"}`}
    onSubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    @if (props.fixedName) {
      <></>;
    } @else {
      <div className="flex flex-col gap-1">
        <Label htmlFor="env-editor-name">Name</Label>
        <Input
          id="env-editor-name"
          value={draft.name}
          onInput={(e) => setDraft({ ...draft, name: (e.target as HTMLInputElement).value })}
          placeholder="production"
          data-testid="env-editor-name-input"
        />
      </div>;
    }
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="flex flex-col gap-1">
        <Label htmlFor={`env-wait-${props.fixedName ?? "new"}`}>Wait timer (minutes)</Label>
        <Input
          id={`env-wait-${props.fixedName ?? "new"}`}
          type="number"
          value={draft.wait_timer_minutes}
          onInput={(e) =>
            setDraft({ ...draft, wait_timer_minutes: (e.target as HTMLInputElement).value })
          }
          placeholder="0"
          data-testid="env-editor-wait"
        />
      </div>
      <div className="flex flex-col gap-1">
        <Label htmlFor={`env-reviewers-${props.fixedName ?? "new"}`}>Required reviewers</Label>
        <Input
          id={`env-reviewers-${props.fixedName ?? "new"}`}
          value={draft.required_reviewers}
          onInput={(e) =>
            setDraft({ ...draft, required_reviewers: (e.target as HTMLInputElement).value })
          }
          placeholder="alice, bob"
          data-testid="env-editor-reviewers"
        />
        <p className="text-[11px] text-muted-foreground">
          Comma-separated usernames — deployments hold until one approves.
        </p>
      </div>
    </div>
    <div className="flex items-center gap-2">
      <Checkbox
        id={`env-self-${props.fixedName ?? "new"}`}
        checked={draft.prevent_self_review}
        onCheckedChange={(v) => setDraft({ ...draft, prevent_self_review: v === true })}
        data-testid="env-editor-prevent-self"
      />
      <Label htmlFor={`env-self-${props.fixedName ?? "new"}`}>
        Prevent self-review — the run's actor may not approve
      </Label>
    </div>
    <div className="flex flex-col gap-1">
      <Label htmlFor={`env-branches-${props.fixedName ?? "new"}`}>Deployment branches</Label>
      <Input
        id={`env-branches-${props.fixedName ?? "new"}`}
        value={draft.deployment_branches}
        onInput={(e) =>
          setDraft({ ...draft, deployment_branches: (e.target as HTMLInputElement).value })
        }
        placeholder="main, release/*"
        data-testid="env-editor-branches"
      />
      <p className="text-[11px] text-muted-foreground">
        Optional branch allowlist — jobs on other branches fail loudly.
      </p>
    </div>
    @if (envErr) {
      <p className="text-[13px] text-destructive" role="alert">
        {envErr as string}
      </p>;
    }
    <div className="flex gap-2">
      <Button type="submit" size="sm" disabled={props.busy} data-testid="env-editor-save">
        {(props.busy ? "Saving…" : "Save environment") as string}
      </Button>
      <Button type="button" size="sm" variant="ghost" onClick={props.onCancel}>
        Cancel
      </Button>
    </div>
  </form>;
}

/** Environments list + protection editor + per-env secrets + deployment
 * history — repo Settings → Actions (D-ACT-18 parity). */
export function ActionsEnvironmentsPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const qc = useQueryClient();
  const envsQ = useQuery({
    ...actionsEnvironmentsQuery(props.owner, props.name),
    enabled: props.can_admin,
  });
  const envs = envsQ.data?.environments ?? [];
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [editing, setEditing] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(false);
  const envKey = ["repo", "actions", "environments", props.owner, props.name] as const;

  async function save(draft: EnvDraft) {
    setBusy(true);
    try {
      const res = await apiClient.repo.actions.environments.upsert({
        owner: props.owner,
        name: props.name,
        environment: draft.name.trim(),
        protection: {
          wait_timer_minutes: Number(draft.wait_timer_minutes) || 0,
          required_reviewers: parseList(draft.required_reviewers),
          prevent_self_review: draft.prevent_self_review,
          deployment_branches: parseList(draft.deployment_branches),
        },
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not save environment.");
        setBusy(false);
        return;
      }
      setEditing(null);
      setCreating(false);
      await qc.invalidateQueries({ queryKey: envKey });
      toastSuccess("Environment saved.");
      setBusy(false);
    } catch {
      toastError(NETWORK_ERROR);
      setBusy(false);
    }
  }

  async function remove(env: string) {
    try {
      const res = await apiClient.repo.actions.environments.delete({
        owner: props.owner,
        name: props.name,
        environment: env,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete environment.");
        return;
      }
      await qc.invalidateQueries({ queryKey: envKey });
      toastSuccess("Environment deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  <div className="mt-6 flex flex-col gap-3" data-testid="actions-environments-panel">
    <div className="flex items-center justify-between">
      <h3 className="text-[15px] font-semibold text-foreground">Environments</h3>
      @if (props.can_admin && !creating) {
        <Button
          type="button"
          size="sm"
          variant="outline"
          data-testid="env-new"
          onClick={() => setCreating(true)}
        >
          <Plus aria-hidden size={14} />
          New environment
        </Button>;
      }
    </div>
    @if (creating) {
      <EnvEditor
        owner={props.owner}
        name={props.name}
        initial={draftFor()}
        busy={busy}
        onSave={(d) => void save(d)}
        onCancel={() => setCreating(false)}
      />;
    }
    <List>
      @for (const env of envs; key env.name) {
        <ListRow className="flex-col items-stretch gap-0 p-0">
          <div className="flex items-center gap-2 px-3 py-2">
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-2 text-left text-[14px] outline-none"
              data-testid={`env-toggle-${env.name}`}
              aria-expanded={expanded[env.name] === true}
              onClick={() =>
                setExpanded((prev) => ({ ...prev, [env.name]: !prev[env.name] }))
              }
            >
              @if (expanded[env.name]) {
                <ChevronDown aria-hidden size={14} className="shrink-0" />;
              } @else {
                <ChevronRight aria-hidden size={14} className="shrink-0" />;
              }
              <span className="font-medium text-foreground">{env.name as string}</span>
              @if (
                (env.protection.required_reviewers ?? []).length > 0
              ) {
                <Badge variant="default" className="text-[10px]">
                  {
                    ((env.protection.required_reviewers ?? []).length +
                      " reviewer" +
                      ((env.protection.required_reviewers ?? []).length === 1
                        ? ""
                        : "s")) as string
                  }
                </Badge>;
              }
              @if (
                (env.protection.wait_timer_minutes ?? 0) > 0
              ) {
                <Badge variant="default" className="text-[10px]">
                  {(env.protection.wait_timer_minutes + "m wait") as string}
                </Badge>;
              }
            </button>
            @if (props.can_admin) {
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="px-1.5"
                  aria-label={"Edit " + env.name}
                  title="Edit protection rules"
                  data-testid={`env-edit-${env.name}`}
                  onClick={() => setEditing(editing === env.name ? null : env.name)}
                >
                  <Pencil aria-hidden size={13} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="px-1.5 text-muted-foreground hover:text-destructive"
                  aria-label={"Delete " + env.name}
                  title="Delete environment"
                  data-testid={`env-delete-${env.name}`}
                  onClick={() => void remove(env.name)}
                >
                  <Trash2 aria-hidden size={13} />
                </Button>
              </div>;
            }
          </div>
          @if (expanded[env.name]) {
            <div className="border-t border-border px-3 py-3">
              <EnvSecrets owner={props.owner} name={props.name} env={env.name} />
              <EnvVariables owner={props.owner} name={props.name} env={env.name} />
            </div>;
          }
          @if (editing === env.name) {
            <div className="border-t border-border px-3 py-2">
              <EnvEditor
                owner={props.owner}
                name={props.name}
                initial={draftFor(env)}
                fixedName={env.name}
                busy={busy}
                onSave={(d) => void save({ ...d, name: env.name })}
                onCancel={() => setEditing(null)}
              />
            </div>;
          }
        </ListRow>;
      } @empty {
        <ListRow>
          <span className="text-[14px] text-muted-foreground">
            No environments — jobs that name one auto-create it on first run.
          </span>
        </ListRow>;
      }
    </List>
  </div>;
}
