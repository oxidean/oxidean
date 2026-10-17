import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type {
  InstanceTemplatePackPublic,
  InstanceWorkflowTemplatePublic,
  RepoCreateDefaults,
  RepoTemplateOption,
  UserPublic,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FileDropzone } from "@/components/ui/file-dropzone";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { adminWorkflowTemplatesQuery } from "@/lib/actions-queries";
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
const wfQueryKey = ["admin", "workflowTemplates"] as const;
const GROUP_DATALIST_ID = "admin-tpl-groups";
const GITIGNORE_DATALIST_ID = "admin-tpl-gitignores";
const WF_ICON_DATALIST_ID = "admin-wf-icons";

// Icon-name suggestions for instance workflow templates: stems of
// crates/oxidean-api/assets/workflow-templates/icons/*.svg. The input is a
// plain HTML datalist, so non-listed names still save.
const BUILTIN_ICON_NAMES = [
  "ada",
  "alibabacloud",
  "android",
  "ant",
  "aws",
  "azure",
  "azure-staticwebapp",
  "blank",
  "c-cpp",
  "clojure",
  "cmake",
  "crystal",
  "d",
  "dart",
  "datadog",
  "deno",
  "django",
  "docker",
  "dotnet",
  "elixir",
  "erlang",
  "generator-generic-ossf-slsa3-publish",
  "go",
  "google-cloud",
  "go-ossf-slsa3-publish",
  "gradle",
  "haskell",
  "ibm",
  "jekyll",
  "makefile",
  "maven",
  "nodejs",
  "node-package-transparent",
  "octopusdeploy",
  "openshift",
  "php",
  "python",
  "r",
  "rails",
  "ruby",
  "ruby-gems",
  "rust",
  "scala",
  "swift",
  "tencentcloud",
  "terraform",
  "webpack",
  "xcode",
];

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

function AdminTemplatesContent() @{
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

  const wfQ = useQuery({
    ...adminWorkflowTemplatesQuery(),
    enabled: me.data?.role === "sys-admin",
  });
  const wfTemplates = wfQ.data?.templates ?? [];

  const [wfFormOpen, setWfFormOpen] = useState(false);
  const [wfEditing, setWfEditing] = useState<InstanceWorkflowTemplatePublic | null>(null);
  const [wfSlug, setWfSlug] = useState("");
  const [wfName, setWfName] = useState("");
  const [wfDesc, setWfDesc] = useState("");
  const [wfIcon, setWfIcon] = useState("");
  const [wfCats, setWfCats] = useState("");
  const [wfPatterns, setWfPatterns] = useState("");
  const [wfFilename, setWfFilename] = useState("");
  const [wfContent, setWfContent] = useState("");
  const [wfError, setWfError] = useState("");
  const [wfSaving, setWfSaving] = useState(false);
  const [wfDelete, setWfDelete] = useState<InstanceWorkflowTemplatePublic | null>(null);
  const [wfDeleting, setWfDeleting] = useState(false);

  function wfReset() {
    setWfFormOpen(false);
    setWfEditing(null);
    setWfSlug("");
    setWfName("");
    setWfDesc("");
    setWfIcon("");
    setWfCats("");
    setWfPatterns("");
    setWfFilename("");
    setWfContent("");
    setWfError("");
  }

  function wfStartEdit(t: InstanceWorkflowTemplatePublic) {
    setWfEditing(t);
    setWfSlug(t.slug);
    setWfName(t.name);
    setWfDesc(t.description);
    setWfIcon(t.icon_name ?? "");
    setWfCats(t.categories.join(", "));
    setWfPatterns(t.file_patterns.join(", "));
    setWfFilename(t.filename);
    setWfContent(t.content);
    setWfError("");
    setWfFormOpen(true);
  }

  function wfCommaList(raw: string): string[] {
    return raw
      .split(",")
      .map((v) => v.trim())
      .filter(Boolean);
  }

  async function wfSubmit() {
    if (wfSaving) return;
    if (!wfName.trim() || !wfContent.trim()) {
      setWfError("Name and workflow content are required.");
      return;
    }
    if (!wfEditing && !wfSlug.trim()) {
      setWfError("Slug is required.");
      return;
    }
    setWfError("");
    setWfSaving(true);
    try {
      const categories = wfCommaList(wfCats);
      const file_patterns = wfCommaList(wfPatterns);
      const icon_name = wfIcon.trim() || null;
      const baseSlug = wfEditing ? wfEditing.slug : wfSlug.trim();
      const filename = wfFilename.trim() || `${baseSlug}.yml`;
      if (wfEditing) {
        const res = await apiClient.admin.workflowTemplates.update({
          id: wfEditing.id,
          name: wfName.trim(),
          description: wfDesc.trim(),
          icon_name,
          categories,
          file_patterns,
          filename,
          content: wfContent,
        });
        if (!res.ok) {
          setWfError(res.error.message || "Could not update workflow template.");
          return;
        }
        toastSuccess("Workflow template updated.");
      } else {
        const res = await apiClient.admin.workflowTemplates.create({
          slug: baseSlug,
          name: wfName.trim(),
          description: wfDesc.trim(),
          icon_name,
          categories,
          file_patterns,
          filename,
          content: wfContent,
        });
        if (!res.ok) {
          setWfError(res.error.message || "Could not create workflow template.");
          return;
        }
        toastSuccess("Workflow template created.");
      }
      wfReset();
      await qc.invalidateQueries({ queryKey: wfQueryKey });
    } catch {
      setWfError(NETWORK_ERROR);
    } finally {
      setWfSaving(false);
    }
  }

  async function wfToggle(t: InstanceWorkflowTemplatePublic, enabled: boolean) {
    const res = await apiClient.admin.workflowTemplates.setEnabled({ id: t.id, enabled });
    if (!res.ok) {
      toastError(res.error.message || "Could not update workflow template.");
      return;
    }
    toastSuccess(enabled ? "Template enabled." : "Template disabled.");
    await qc.invalidateQueries({ queryKey: wfQueryKey });
  }

  async function wfConfirmDelete() {
    const t = wfDelete;
    if (!t || wfDeleting) return;
    setWfDeleting(true);
    const res = await apiClient.admin.workflowTemplates.delete({ id: t.id });
    setWfDeleting(false);
    if (!res.ok) {
      toastError(res.error.message || "Could not delete workflow template.");
      return;
    }
    toastSuccess("Workflow template deleted.");
    setWfDelete(null);
    await qc.invalidateQueries({ queryKey: wfQueryKey });
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
      <datalist id={WF_ICON_DATALIST_ID}>
        @for (const iconName of BUILTIN_ICON_NAMES; key iconName) {
          <option value={iconName} />;
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

        <section className="flex flex-col gap-3" aria-labelledby="wf-heading">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 id="wf-heading" className="text-[18px] font-semibold text-foreground">
                Workflow templates
              </h2>
              <p className="mt-1 text-[13px] text-muted-foreground">
                Instance entries in the Actions “New workflow” catalog, shown alongside the vendored
                starters.
              </p>
            </div>
            @if (!wfFormOpen) {
              <Button
                type="button"
                variant="secondary"
                size="sm"
                data-testid="admin-wf-new"
                onClick={() => {
                  setWfEditing(null);
                  setWfError("");
                  setWfFormOpen(true);
                }}
              >
                New workflow template
              </Button>;
            }
          </div>

          @if (wfFormOpen) {
            <div
              className="flex flex-col gap-3 rounded-md border border-border p-4"
              data-testid="admin-wf-form"
            >
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-slug">Slug</Label>
                  <Input
                    id="admin-wf-slug"
                    data-testid="admin-wf-slug"
                    value={wfSlug}
                    onInput={(e) => setWfSlug((e.currentTarget as HTMLInputElement).value)}
                    placeholder="acme-ci"
                    disabled={wfSaving || wfEditing !== null}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-name">Name</Label>
                  <Input
                    id="admin-wf-name"
                    data-testid="admin-wf-name"
                    value={wfName}
                    onInput={(e) => setWfName((e.currentTarget as HTMLInputElement).value)}
                    placeholder="Acme CI"
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1 sm:col-span-2">
                  <Label htmlFor="admin-wf-desc">Description</Label>
                  <Input
                    id="admin-wf-desc"
                    data-testid="admin-wf-desc"
                    value={wfDesc}
                    onInput={(e) => setWfDesc((e.currentTarget as HTMLInputElement).value)}
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-icon">Icon name</Label>
                  <Input
                    id="admin-wf-icon"
                    data-testid="admin-wf-icon"
                    list={WF_ICON_DATALIST_ID}
                    value={wfIcon}
                    onInput={(e) => setWfIcon((e.currentTarget as HTMLInputElement).value)}
                    placeholder="rust"
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-filename">Filename</Label>
                  <Input
                    id="admin-wf-filename"
                    data-testid="admin-wf-filename"
                    value={wfFilename}
                    onInput={(e) =>
                      setWfFilename((e.currentTarget as HTMLInputElement).value)
                    }
                    placeholder={(wfSlug.trim() || "slug") + ".yml"}
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-categories">Categories</Label>
                  <Input
                    id="admin-wf-categories"
                    data-testid="admin-wf-categories"
                    value={wfCats}
                    onInput={(e) => setWfCats((e.currentTarget as HTMLInputElement).value)}
                    placeholder="Continuous integration, Rust"
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label htmlFor="admin-wf-patterns">File patterns</Label>
                  <Input
                    id="admin-wf-patterns"
                    data-testid="admin-wf-patterns"
                    value={wfPatterns}
                    onInput={(e) =>
                      setWfPatterns((e.currentTarget as HTMLInputElement).value)
                    }
                    placeholder="Cargo.toml, **/go.mod"
                    disabled={wfSaving}
                    autoComplete="off"
                  />
                </div>
                <div className="flex flex-col gap-1 sm:col-span-2">
                  <Label htmlFor="admin-wf-content">Workflow YAML</Label>
                  <Textarea
                    id="admin-wf-content"
                    data-testid="admin-wf-content"
                    className="min-h-64 font-[family-name:var(--font-mono)] text-[13px]"
                    rows={16}
                    value={wfContent}
                    onInput={(e) =>
                      setWfContent((e.currentTarget as HTMLTextAreaElement).value)
                    }
                    placeholder="name: ci&#10;on: [push]&#10;jobs: …"
                    disabled={wfSaving}
                    spellcheck={false}
                  />
                </div>
              </div>
              @if (wfError) {
                <p
                  className="text-[14px] text-destructive"
                  role="alert"
                  data-testid="admin-wf-error"
                >
                  {wfError as string}
                </p>;
              }
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  data-testid="admin-wf-save"
                  disabled={wfSaving}
                  onClick={() => void wfSubmit()}
                >
                  {wfEditing ? "Save changes" : "Create template"}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="admin-wf-cancel"
                  disabled={wfSaving}
                  onClick={() => wfReset()}
                >
                  Cancel
                </Button>
              </div>
            </div>;
          }

          @if (wfQ.isLoading) {
            <p className="text-[14px] text-muted-foreground">Loading…</p>;
          } @else if (wfQ.isError) {
            <p className="text-[14px] text-destructive" role="alert">
              {(wfQ.error as Error).message as string}
            </p>;
          } @else if (wfTemplates.length === 0) {
            <p className="text-[14px] text-muted-foreground">No workflow templates yet.</p>;
          } @else {
            <ul className="flex flex-col gap-3" data-testid="admin-wf-list">
              @for (const t of wfTemplates; key t.id) {
                <li
                  className="flex flex-col gap-2 rounded-md border border-border p-3 sm:flex-row sm:items-center sm:justify-between"
                  data-testid={`admin-wf-row-${t.id}`}
                >
                  <div className="min-w-0">
                    <p className="flex flex-wrap items-center gap-2 text-[14px] font-semibold text-foreground">
                      <span>{t.name as string}</span>
                      <span className="font-normal text-muted-foreground">
                        ({t.slug as string} · {t.filename as string})
                      </span>
                      @if (!t.enabled) {
                        <Badge variant="outline">Disabled</Badge>;
                      }
                    </p>
                    <p className="text-[13px] text-muted-foreground">
                      {t.description as string}
                    </p>
                    @if (t.categories.length > 0) {
                      <p className="mt-1 flex flex-wrap gap-1">
                        @for (const c of t.categories; key c) {
                          <Badge variant="outline">{c as string}</Badge>;
                        }
                      </p>;
                    }
                  </div>
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2">
                      <Label htmlFor={`admin-wf-en-${t.id}`} className="text-[13px]">
                        Enabled
                      </Label>
                      <Switch
                        id={`admin-wf-en-${t.id}`}
                        data-testid={`admin-wf-toggle-${t.id}`}
                        checked={t.enabled}
                        onCheckedChange={(v) => void wfToggle(t, !!v)}
                      />
                    </div>
                    <Button
                      type="button"
                      variant="secondary"
                      data-testid={`admin-wf-edit-${t.id}`}
                      onClick={() => wfStartEdit(t)}
                    >
                      Edit
                    </Button>
                    <Button
                      type="button"
                      variant="destructive"
                      data-testid={`admin-wf-delete-${t.id}`}
                      onClick={() => setWfDelete(t)}
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

      <AlertDialog
        open={wfDelete !== null}
        onOpenChange={(next) => {
          if (!next && !wfDeleting) setWfDelete(null);
        }}
      >
        <AlertDialogContent data-testid="admin-wf-delete-dialog">
          <AlertDialogHeader>
            <AlertDialogTitle>Delete workflow template?</AlertDialogTitle>
            <AlertDialogDescription className="break-words whitespace-normal">
              {
                `Removes “${wfDelete?.name ?? ""}” from the catalog for every repository. This can’t be undone.` as string
              }
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={wfDeleting}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              data-testid="admin-wf-delete-confirm"
              disabled={wfDeleting}
              onClick={() => void wfConfirmDelete()}
            >
              Delete template
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AdminSettingsLayout>;
  }
}
