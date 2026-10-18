import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { KeyRound, SlidersHorizontal } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { AppPage } from "@/components/app-page";
import {
  OrgSettingsShell,
  orgSettingsAccessQueryOptions,
  orgSettingsOwner,
} from "@/components/org/org-settings-shell.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import {
  orgActionsSecretReposQuery,
  orgActionsSecretsQuery,
  orgActionsVariableReposQuery,
  orgActionsVariablesQuery,
} from "@/lib/actions-queries";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need organization admin access to manage Actions secrets.";

const VISIBILITY_OPTIONS = [
  { value: "all", label: "All repositories" },
  { value: "private", label: "Private repositories" },
  { value: "selected", label: "Selected repositories" },
] as const;

/** `/{owner}/settings/actions` — org Actions secrets (title via shell). */
export function OrgActionsPage() @{
  <AppPage>
    <OrgSettingsShell active="actions">
      <OrgActionsContent />
    </OrgSettingsShell>
  </AppPage>;
}

/** Selected-visibility repo coverage for one org secret. */
function SecretReposLine(props: { slug: string; name: string }) @{
  const reposQ = useQuery(orgActionsSecretReposQuery(props.slug, props.name));
  const ids = reposQ.data?.repository_ids ?? [];
  @if (ids.length > 0) {
    <span className="block text-[12px] text-muted-foreground">
      {"Repositories: " + ids.join(", ")}
    </span>;
  }
}

/** Selected-visibility repo coverage for one org variable. */
function VariableReposLine(props: { slug: string; name: string }) @{
  const reposQ = useQuery(orgActionsVariableReposQuery(props.slug, props.name));
  const ids = reposQ.data?.repository_ids ?? [];
  @if (ids.length > 0) {
    <span className="block text-[12px] text-muted-foreground">
      {"Repositories: " + ids.join(", ")}
    </span>;
  }
}

/** Org Actions secrets — Admin only; values are write-only. */
function OrgActionsContent() @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const accessQ = useQuery(orgSettingsAccessQueryOptions(owner));
  const org = accessQ.data?.kind === "ready" ? accessQ.data.org : null;
  const slug = org?.slug ?? owner;
  const qc = useQueryClient();
  const secretsQ = useQuery({
    ...orgActionsSecretsQuery(slug),
    enabled: Boolean(org),
  });
  const secrets = secretsQ.data?.secrets ?? [];
  const variablesQ = useQuery({
    ...orgActionsVariablesQuery(slug),
    enabled: Boolean(org),
  });
  const variables = variablesQ.data?.variables ?? [];

  const [secretName, setSecretName] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [visibility, setVisibility] = useState<string>("all");
  const [repoIds, setRepoIds] = useState("");
  const [pending, setPending] = useState(false);
  const [varName, setVarName] = useState("");
  const [varValue, setVarValue] = useState("");
  const [varVisibility, setVarVisibility] = useState<string>("all");
  const [varRepoIds, setVarRepoIds] = useState("");
  const [varPending, setVarPending] = useState(false);

  async function put() {
    const sn = secretName.trim();
    if (!sn || !secretValue) return;
    setPending(true);
    try {
      const res = await apiClient.org.actions.secrets.put({
        slug,
        secret_name: sn,
        value: secretValue,
        visibility,
        repository_ids:
          visibility === "selected"
            ? repoIds
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean)
            : undefined,
      });
      if (!res.ok) {
        toastError(
          res.error.code === "org.forbidden" ? FORBIDDEN : res.error.message || NETWORK_ERROR,
        );
        setPending(false);
        return;
      }
      setSecretName("");
      setSecretValue("");
      setRepoIds("");
      await qc.invalidateQueries({ queryKey: ["org", "actions", "secrets", slug] });
      toastSuccess("Organization secret saved. Value cannot be viewed again.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function del(sn: string) {
    try {
      const res = await apiClient.org.actions.secrets.delete({ slug, secret_name: sn });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete secret.");
        return;
      }
      await qc.invalidateQueries({ queryKey: ["org", "actions", "secrets", slug] });
      toastSuccess("Organization secret deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  async function putVariable() {
    const vn = varName.trim();
    if (!vn) return;
    setVarPending(true);
    try {
      const res = await apiClient.org.actions.variables.put({
        slug,
        variable_name: vn,
        value: varValue,
        visibility: varVisibility,
        repository_ids:
          varVisibility === "selected"
            ? varRepoIds
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean)
            : undefined,
      });
      if (!res.ok) {
        toastError(
          res.error.code === "org.forbidden" ? FORBIDDEN : res.error.message || NETWORK_ERROR,
        );
        setVarPending(false);
        return;
      }
      setVarName("");
      setVarValue("");
      setVarRepoIds("");
      await qc.invalidateQueries({ queryKey: ["org", "actions", "variables", slug] });
      toastSuccess("Organization variable saved.");
      setVarPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setVarPending(false);
    }
  }

  async function delVariable(vn: string) {
    try {
      const res = await apiClient.org.actions.variables.delete({ slug, variable_name: vn });
      if (!res.ok) {
        toastError(res.error.message || "Could not delete variable.");
        return;
      }
      await qc.invalidateQueries({ queryKey: ["org", "actions", "variables", slug] });
      toastSuccess("Organization variable deleted.");
    } catch {
      toastError(NETWORK_ERROR);
    }
  }

  @if (!org) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else {
    <div data-testid="org-actions-settings">
      <h2 className="flex items-center gap-2 text-[18px] font-semibold text-foreground">
        <KeyRound aria-hidden size={18} />
        Actions secrets
      </h2>
      <p className="mt-1 text-[14px] text-muted-foreground">
        Organization-level secrets shared with Actions workflows. Visibility controls which
        repositories may resolve them.
      </p>

      <form
        className="mt-6 flex flex-col gap-3"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void put();
        }}
      >
        <div className="grid gap-3 sm:grid-cols-3">
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-secret-name">Name</Label>
            <Input
              id="org-secret-name"
              value={secretName}
              onInput={(e) => setSecretName((e.target as HTMLInputElement).value)}
              placeholder="DEPLOY_TOKEN"
              data-testid="org-secret-name"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-secret-value">Value</Label>
            <Input
              id="org-secret-value"
              type="password"
              value={secretValue}
              onInput={(e) => setSecretValue((e.target as HTMLInputElement).value)}
              data-testid="org-secret-value"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-secret-visibility">Visibility</Label>
            <select
              id="org-secret-visibility"
              className="h-9 rounded-md border border-border bg-background px-3 text-[14px] text-foreground"
              value={visibility}
              data-testid="org-secret-visibility"
              suppressNativeChangeWarning
              onChange={(e) => setVisibility((e.target as HTMLSelectElement).value)}
            >
              @for (const o of VISIBILITY_OPTIONS; key o.value) {
                <option value={o.value}>{o.label as string}</option>;
              }
            </select>
          </div>
        </div>
        @if (visibility === "selected") {
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-secret-repos">Repository ids (comma-separated)</Label>
            <Input
              id="org-secret-repos"
              value={repoIds}
              onInput={(e) => setRepoIds((e.target as HTMLInputElement).value)}
              placeholder="r-abc, r-def"
              data-testid="org-secret-repos"
            />
          </div>;
        }
        <div>
          <Button
            type="submit"
            disabled={pending || !secretName.trim() || !secretValue}
            data-testid="org-secret-save"
          >
            Save secret
          </Button>
        </div>
      </form>

      <List className="mt-6">
        @for (const s of secrets; key s.name) {
          <ListRow className="items-center justify-between gap-3">
            <span className="flex flex-col gap-0.5 font-mono text-[14px]">
              <span className="flex items-center gap-2">
                {s.name as string}
                <Badge variant="default" className="text-[10px]">
                  {
                    (VISIBILITY_OPTIONS.find((o) => o.value === s.visibility)?.label ??
                      s.visibility) as string
                  }
                </Badge>
              </span>
              @if (s.visibility === "selected") {
                <SecretReposLine slug={slug} name={s.name} />;
              }
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-testid={"org-secret-delete-" + s.name}
              onClick={() => void del(s.name)}
            >
              Delete
            </Button>
          </ListRow>;
        } @empty {
          <ListRow>
            <span className="text-[14px] text-muted-foreground">
              No organization secrets yet.
            </span>
          </ListRow>;
        }
      </List>

      <h2 className="mt-10 flex items-center gap-2 text-[18px] font-semibold text-foreground">
        <SlidersHorizontal aria-hidden size={18} />
        Actions variables
      </h2>
      <p className="mt-1 text-[14px] text-muted-foreground">
        Plaintext configuration exposed as{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px]">vars.*</code> in
        workflow expressions. Visibility controls which repositories resolve them — a repository
        variable of the same name takes precedence.
      </p>

      <form
        className="mt-6 flex flex-col gap-3"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void putVariable();
        }}
      >
        <div className="grid gap-3 sm:grid-cols-3">
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-var-name">Name</Label>
            <Input
              id="org-var-name"
              value={varName}
              onInput={(e) => setVarName((e.target as HTMLInputElement).value)}
              placeholder="DEPLOY_TARGET"
              data-testid="org-var-name"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-var-value">Value</Label>
            <Input
              id="org-var-value"
              value={varValue}
              onInput={(e) => setVarValue((e.target as HTMLInputElement).value)}
              data-testid="org-var-value"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-var-visibility">Visibility</Label>
            <select
              id="org-var-visibility"
              className="h-9 rounded-md border border-border bg-background px-3 text-[14px] text-foreground"
              value={varVisibility}
              data-testid="org-var-visibility"
              suppressNativeChangeWarning
              onChange={(e) => setVarVisibility((e.target as HTMLSelectElement).value)}
            >
              @for (const o of VISIBILITY_OPTIONS; key o.value) {
                <option value={o.value}>{o.label as string}</option>;
              }
            </select>
          </div>
        </div>
        @if (varVisibility === "selected") {
          <div className="flex flex-col gap-1">
            <Label htmlFor="org-var-repos">Repository ids (comma-separated)</Label>
            <Input
              id="org-var-repos"
              value={varRepoIds}
              onInput={(e) => setVarRepoIds((e.target as HTMLInputElement).value)}
              placeholder="r-abc, r-def"
              data-testid="org-var-repos"
            />
          </div>;
        }
        <div>
          <Button type="submit" disabled={varPending || !varName.trim()} data-testid="org-var-save">
            Save variable
          </Button>
        </div>
      </form>

      <List className="mt-6">
        @for (const v of variables; key v.name) {
          <ListRow className="items-center justify-between gap-3">
            <span className="flex flex-col gap-0.5 font-mono text-[14px]">
              <span className="flex items-center gap-2">
                {v.name as string}
                <Badge variant="default" className="text-[10px]">
                  {
                    (VISIBILITY_OPTIONS.find((o) => o.value === v.visibility)?.label ??
                      v.visibility) as string
                  }
                </Badge>
              </span>
              <span className="font-normal text-muted-foreground">{v.value as string}</span>
              @if (v.visibility === "selected") {
                <VariableReposLine slug={slug} name={v.name} />;
              }
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-testid={"org-var-delete-" + v.name}
              onClick={() => void delVariable(v.name)}
            >
              Delete
            </Button>
          </ListRow>;
        } @empty {
          <ListRow>
            <span className="text-[14px] text-muted-foreground">
              No organization variables yet.
            </span>
          </ListRow>;
        }
      </List>
    </div>;
  }
}
