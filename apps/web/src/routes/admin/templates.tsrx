import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type {
  InstanceTemplatePackPublic,
  RepoCreateDefaults,
  RepoTemplateOption,
  UserPublic,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FileDropzone } from "@/components/ui/file-dropzone";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";

type AdminGateLoader =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | { kind: "ready"; me: UserPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage templates.";
const packsQueryKey = ["admin", "templates", "list"] as const;
const defaultsQueryKey = ["admin", "templates", "defaults"] as const;
const GROUP_DATALIST_ID = "admin-tpl-groups";
const GITIGNORE_DATALIST_ID = "admin-tpl-gitignores";

function isBuiltinStack(opt: RepoTemplateOption): boolean {
  return !opt.provenance || opt.provenance === "builtin";
}

function uniqueSorted(values: string[]): string[] {
  return [...new Set(values.map((v) => v.trim()).filter(Boolean))].sort((a, b) =>
    a.localeCompare(b),
  );
}

function adminTemplatesQueryOptions() {
  return queryOptions({
    queryKey: ["admin", "adminTemplates"],
    queryFn: async (): Promise<AdminGateLoader> => {
      try {
        const meRes = await fetchSessionMe();
        if (!meRes.ok) {
          if (meRes.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return { kind: "error", message: meRes.error.message || "Could not load session." };
        }
        if (meRes.data.role !== "sys-admin") {
          return { kind: "forbidden" };
        }
        return { kind: "ready", me: meRes.data };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** Built-in + instance template packs (issue #18). */
export function AdminTemplatesPage() @{
  <AppPage>
    <AdminTemplatesContent />
  </AppPage>;
}

export function AdminTemplatesContent() @{
  const qc = useQueryClient();
  const loaderData = useQuery(adminTemplatesQueryOptions()).data;
  const [slug, setSlug] = useState("");
  const [label, setLabel] = useState("");
  const [group, setGroup] = useState("Custom");
  const [description, setDescription] = useState("");
  const [defaultGitignore, setDefaultGitignore] = useState("");
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/templates");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(qc, loaderData.me);
    }
  }, [loaderData, qc]);

  const me = useQuery({
    ...authSessionQueryOptions(),
    ...(loaderData?.kind === "ready"
      ? { initialData: loaderData.me }
      : loaderData?.kind === "unauthenticated"
        ? { initialData: null as UserPublic | null }
        : {}),
  });

  const packsQ = useQuery({
    queryKey: packsQueryKey,
    queryFn: async (): Promise<InstanceTemplatePackPublic[]> => {
      const res = await apiClient.admin.templates.list();
      if (!res.ok) throw new Error(res.error.message || "Could not load templates.");
      return res.data.packs;
    },
    enabled: me.data?.role === "sys-admin",
  });

  const defaultsQ = useQuery({
    queryKey: defaultsQueryKey,
    queryFn: async (): Promise<RepoCreateDefaults> => {
      const res = await apiClient.repo.createDefaults();
      if (!res.ok) throw new Error(res.error.message || "Could not load built-in packs.");
      return res.data;
    },
    enabled: me.data?.role === "sys-admin",
  });

  const builtinPacks = (defaultsQ.data?.stacks ?? []).filter(isBuiltinStack);
  const gitignores = defaultsQ.data?.gitignores ?? [];
  const instancePacks = packsQ.data ?? [];

  const groupSuggestions = uniqueSorted([
    "Custom",
    ...builtinPacks.map((p) => p.group),
    ...instancePacks.map((p) => p.group),
  ]);

  async function uploadPack(files: File[]) {
    const file = files[0];
    if (!file) return;
    if (!slug.trim() || !label.trim()) {
      setError("Slug and label are required before uploading.");
      return;
    }
    setError("");
    setUploading(true);
    try {
      const fd = new FormData();
      fd.append("pack", file);
      fd.append("slug", slug.trim());
      fd.append("label", label.trim());
      fd.append("group", group.trim() || "Custom");
      fd.append("description", description);
      if (defaultGitignore.trim()) {
        fd.append("default_gitignore", defaultGitignore.trim());
      }
      const res = await fetch("/api/admin/templates", {
        method: "POST",
        body: fd,
        credentials: "include",
      });
      const body = (await res.json()) as {
        ok?: boolean;
        error?: { message?: string };
      };
      if (!res.ok || !body.ok) {
        setError(body.error?.message || "Upload failed.");
        setUploading(false);
        return;
      }
      toastSuccess("Template pack uploaded.");
      setSlug("");
      setLabel("");
      setDescription("");
      setDefaultGitignore("");
      await qc.invalidateQueries({ queryKey: packsQueryKey });
      setUploading(false);
    } catch {
      setError(NETWORK_ERROR);
      setUploading(false);
    }
  }

  async function setEnabled(id: string, enabled: boolean) {
    const res = await apiClient.admin.templates.setEnabled({ id, enabled });
    if (!res.ok) {
      toastError(res.error.message || "Could not update pack.");
      return;
    }
    toastSuccess(enabled ? "Pack enabled." : "Pack disabled.");
    await qc.invalidateQueries({ queryKey: packsQueryKey });
  }

  async function removePack(id: string) {
    const res = await apiClient.admin.templates.delete({ id });
    if (!res.ok) {
      toastError(res.error.message || "Could not delete pack.");
      return;
    }
    toastSuccess("Pack deleted.");
    await qc.invalidateQueries({ queryKey: packsQueryKey });
  }

  @if (loaderData?.kind === "forbidden" || (me.data && me.data.role !== "sys-admin")) {
    <SettingsShell>
      <p className="text-[14px] text-destructive" role="alert">
        {FORBIDDEN as string}
      </p>
    </SettingsShell>;
  } @else if (loaderData?.kind === "error") {
    <SettingsShell>
      <p className="text-[14px] text-destructive" role="alert">
        {loaderData.message as string}
      </p>
    </SettingsShell>;
  } @else {
    <AdminSettingsLayout active="templates">
      <datalist id={GROUP_DATALIST_ID}>
        @for (const g of groupSuggestions; key g) {
          <option value={g} />;
        }
      </datalist>
      <datalist id={GITIGNORE_DATALIST_ID}>
        @for (const g of gitignores; key g.id) {
          <option value={g.id as string} label={g.label as string} />;
        }
      </datalist>

      <div className="flex flex-col gap-8">
        <div>
          <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold text-foreground">
            Templates
          </h1>
          <p className="mt-1 text-[14px] text-muted-foreground">
            Built-in starters ship with Oxidean. Upload zip packs to add instance-specific options
            on /new. Max 10 MiB; no node_modules.
          </p>
        </div>

        <section className="flex flex-col gap-4" aria-labelledby="upload-heading">
          <h2 id="upload-heading" className="text-[18px] font-semibold text-foreground">
            Upload instance pack
          </h2>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="tpl-slug">Slug</Label>
              <Input
                id="tpl-slug"
                value={slug}
                onInput={(e) => setSlug((e.currentTarget as HTMLInputElement).value)}
                placeholder="acme-node"
                disabled={uploading}
                autoComplete="off"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="tpl-label">Label</Label>
              <Input
                id="tpl-label"
                value={label}
                onInput={(e) => setLabel((e.currentTarget as HTMLInputElement).value)}
                placeholder="Acme Node starter"
                disabled={uploading}
                autoComplete="off"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="tpl-group">Group</Label>
              <Input
                id="tpl-group"
                list={GROUP_DATALIST_ID}
                value={group}
                onInput={(e) => setGroup((e.currentTarget as HTMLInputElement).value)}
                disabled={uploading}
                autoComplete="off"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="tpl-gitignore">Default .gitignore</Label>
              <Input
                id="tpl-gitignore"
                list={GITIGNORE_DATALIST_ID}
                value={defaultGitignore}
                onInput={(e) => setDefaultGitignore((e.currentTarget as HTMLInputElement).value)}
                placeholder="Optional catalog id (e.g. Node)"
                disabled={uploading}
                autoComplete="off"
              />
            </div>
            <div className="flex flex-col gap-1 sm:col-span-2">
              <Label htmlFor="tpl-desc">Description</Label>
              <Input
                id="tpl-desc"
                value={description}
                onInput={(e) => setDescription((e.currentTarget as HTMLInputElement).value)}
                disabled={uploading}
                autoComplete="off"
              />
            </div>
          </div>
          <FileDropzone
            accept={{ "application/zip": [".zip"] }}
            maxFiles={1}
            maxSize={10 * 1024 * 1024}
            disabled={uploading}
            hint="Drop a .zip of the starter tree"
            onFiles={(files) => void uploadPack(files)}
          />
          @if (error) {
            <p className="text-[14px] text-destructive" role="alert">
              {error as string}
            </p>;
          }
        </section>

        <section className="flex flex-col gap-3" aria-labelledby="builtin-heading">
          <h2 id="builtin-heading" className="text-[18px] font-semibold text-foreground">
            Official packs
          </h2>
          <p className="text-[13px] text-muted-foreground">
            Embedded in the API binary. Read-only here — refresh via stack-presets tooling.
          </p>
          @if (defaultsQ.isLoading) {
            <p className="text-[14px] text-muted-foreground">Loading…</p>;
          } @else if (defaultsQ.isError) {
            <p className="text-[14px] text-destructive" role="alert">
              {(defaultsQ.error as Error).message as string}
            </p>;
          } @else if (builtinPacks.length === 0) {
            <p className="text-[14px] text-muted-foreground">No built-in packs found.</p>;
          } @else {
            <ul className="flex flex-col gap-3">
              @for (const pack of builtinPacks; key pack.id) {
                <li className="flex flex-col gap-2 rounded-md border border-border p-3 sm:flex-row sm:items-center sm:justify-between">
                  <div className="min-w-0">
                    <p className="flex flex-wrap items-center gap-2 text-[14px] font-semibold text-foreground">
                      <span>{pack.label as string}</span>
                      <Badge variant="outline">Official</Badge>
                      <span className="font-normal text-muted-foreground">
                        ({pack.id as string})
                      </span>
                    </p>
                    <p className="text-[13px] text-muted-foreground">
                      {(pack.group as string) +
                        (pack.default_gitignore
                          ? " · .gitignore: " + pack.default_gitignore
                          : "") +
                        (pack.description ? " — " + pack.description : "")}
                    </p>
                  </div>
                </li>;
              }
            </ul>;
          }
        </section>

        <section className="flex flex-col gap-3" aria-labelledby="packs-heading">
          <h2 id="packs-heading" className="text-[18px] font-semibold text-foreground">
            Instance packs
          </h2>
          @if (packsQ.isLoading) {
            <p className="text-[14px] text-muted-foreground">Loading…</p>;
          } @else if (packsQ.isError) {
            <p className="text-[14px] text-destructive" role="alert">
              {(packsQ.error as Error).message as string}
            </p>;
          } @else if (instancePacks.length === 0) {
            <p className="text-[14px] text-muted-foreground">No instance packs yet.</p>;
          } @else {
            <ul className="flex flex-col gap-3">
              @for (const pack of instancePacks; key pack.id) {
                <li className="flex flex-col gap-2 rounded-md border border-border p-3 sm:flex-row sm:items-center sm:justify-between">
                  <div className="min-w-0">
                    <p className="flex flex-wrap items-center gap-2 text-[14px] font-semibold text-foreground">
                      <span>{pack.label as string}</span>
                      <Badge variant="outline">Instance</Badge>
                      <span className="font-normal text-muted-foreground">
                        ({pack.slug as string})
                      </span>
                    </p>
                    <p className="text-[13px] text-muted-foreground">
                      {(pack.group as string) +
                        " · " +
                        String(pack.byte_size) +
                        " bytes" +
                        (pack.default_gitignore
                          ? " · .gitignore: " + pack.default_gitignore
                          : "") +
                        (pack.description ? " — " + pack.description : "")}
                    </p>
                  </div>
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2">
                      <Label htmlFor={`tpl-en-${pack.id}`} className="text-[13px]">
                        Enabled
                      </Label>
                      <Switch
                        id={`tpl-en-${pack.id}`}
                        checked={pack.enabled}
                        onCheckedChange={(v) => void setEnabled(pack.id, !!v)}
                      />
                    </div>
                    <Button
                      type="button"
                      variant="destructive"
                      onClick={() => void removePack(pack.id)}
                    >
                      Delete
                    </Button>
                  </div>
                </li>;
              }
            </ul>;
          }
        </section>
      </div>
    </AdminSettingsLayout>;
  }
}
