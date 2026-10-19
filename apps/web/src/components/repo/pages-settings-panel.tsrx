import type { RepoPagesPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
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
import { apiClient } from "@/lib/api-client";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function pagesQueryKey(owner: string, name: string) {
  return ["repo", "pages", owner, name] as const;
}

function buildsQueryKey(owner: string, name: string) {
  return ["repo", "pages", "builds", owner, name] as const;
}

const BUILD_TYPE_OPTIONS = [
  { value: "legacy", label: "Deploy from a branch" },
  { value: "workflow", label: "GitHub Actions (workflow)" },
] as const;

const PATH_OPTIONS = [
  { value: "/", label: "/ (root)" },
  { value: "/docs", label: "/docs" },
] as const;

function formatBuildDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

/** Repo Settings — Pages site config + build history (GitHub Settings→Pages). */
export function PagesSettingsPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const siteQ = useQuery({
    queryKey: pagesQueryKey(owner, name),
    queryFn: async (): Promise<RepoPagesPublic | null> => {
      const res = await apiClient.pages.get({ owner, name });
      if (!res.ok) {
        if (res.error.code === "pages.not_found" || res.error.code === "repo.not_found") {
          return null;
        }
        throw new Error(res.error.message || "Could not load Pages settings.");
      }
      return res.data;
    },
    enabled: canAdmin,
  });
  const site = siteQ.data ?? null;

  const buildsQ = useQuery({
    queryKey: buildsQueryKey(owner, name),
    queryFn: async () => {
      const res = await apiClient.pages.builds.list({ owner, name, limit: 10 });
      if (!res.ok) throw new Error(res.error.message || "Could not load builds.");
      return res.data.builds;
    },
    enabled: canAdmin && !!site,
  });
  const builds = buildsQ.data ?? [];

  const [buildType, setBuildType] = useState<"legacy" | "workflow">("legacy");
  const [branch, setBranch] = useState("");
  const [sourcePath, setSourcePath] = useState<"/" | "/docs">("/");
  const [cname, setCname] = useState("");
  const [httpsEnforced, setHttpsEnforced] = useState(true);
  const [pending, setPending] = useState(false);
  const [buildPending, setBuildPending] = useState(false);
  const [disableOpen, setDisableOpen] = useState(false);
  const [disablePending, setDisablePending] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!site) return;
    setBuildType(site.build_type === "workflow" ? "workflow" : "legacy");
    setBranch(site.source.branch);
    setSourcePath(site.source.path === "/docs" ? "/docs" : "/");
    setCname(site.cname);
    setHttpsEnforced(site.https_enforced);
  }, [site?.updated_at]);

  async function save() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    const source = buildType === "legacy" ? { branch: branch.trim(), path: sourcePath } : undefined;
    try {
      const res = site
        ? await apiClient.pages.update({
            owner,
            name,
            build_type: buildType,
            source,
            cname: cname.trim(),
            https_enforced: httpsEnforced,
          })
        : await apiClient.pages.enable({ owner, name, build_type: buildType, source });
      if (!res.ok) {
        setError(res.error.message || "Could not save Pages settings.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: pagesQueryKey(owner, name) });
      toastSuccess(site ? "Pages settings saved." : "Pages site created.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function requestBuild() {
    setBuildPending(true);
    const res = await apiClient.pages.builds.request({ owner, name });
    if (!res.ok) {
      toastError(res.error.message || "Could not request a build.");
      setBuildPending(false);
      return;
    }
    await queryClient.invalidateQueries({ queryKey: pagesQueryKey(owner, name) });
    await queryClient.invalidateQueries({ queryKey: buildsQueryKey(owner, name) });
    toastSuccess("Pages build finished.");
    setBuildPending(false);
  }

  async function disable() {
    setDisablePending(true);
    const res = await apiClient.pages.disable({ owner, name });
    if (!res.ok) {
      toastError(res.error.message || "Could not unpublish the site.");
      setDisablePending(false);
      return;
    }
    await queryClient.invalidateQueries({ queryKey: pagesQueryKey(owner, name) });
    await queryClient.invalidateQueries({ queryKey: buildsQueryKey(owner, name) });
    setDisablePending(false);
    setDisableOpen(false);
    toastSuccess("Pages site unpublished.");
  }

  <section className="rounded-md border border-border bg-card" data-testid="pages-settings-panel">
    <div className="border-b border-border px-5 py-4">
      <h2 id="pages-heading" className="text-[16px] font-semibold text-foreground">
        Pages
      </h2>
      <p className="mt-1 text-[14px] text-muted-foreground">
        Publish a static site from this repository — a branch subtree or an Actions workflow deploy.
      </p>
    </div>
    <div className="flex flex-col gap-5 px-5 py-5">
      @if (!canAdmin) {
        <p className="text-[14px] text-muted-foreground">
          Only repository admins can manage Pages settings.
        </p>;
      } @else if (siteQ.isPending) {
        <p className="text-[14px] text-muted-foreground">Loading…</p>;
      } @else if (siteQ.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          {siteQ.error instanceof Error ? siteQ.error.message : NETWORK_ERROR}
        </p>;
      } @else {
        <div className="flex flex-col gap-5">
          @if (site && site.status === "built") {
            <div
              className="flex flex-wrap items-baseline gap-x-1 text-[14px] text-foreground"
              data-testid="pages-site-url"
            >
              <span>Your site is live at</span>
              {/* raw-anchor-ok — served Pages site, opens in new tab */}
              <a
                href={site.html_url}
                target="_blank"
                rel="noreferrer"
                className="font-medium text-primary underline underline-offset-2"
              >
                {site.html_url}
              </a>
            </div>;
          }
          <div className="flex flex-col gap-2">
            <Label>Source</Label>
            <SelectRoot
              value={buildType}
              onValueChange={(v) => setBuildType(v === "workflow" ? "workflow" : "legacy")}
            >
              <SelectTrigger className="w-full sm:w-72" aria-label="Build source">
                <SelectValue placeholder="Select a source" />
                <SelectIcon />
              </SelectTrigger>
              {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
escape hatch; popup is anchor-scoped under the Select root. */}
              <SelectPortal>
                <SelectPositioner>
                  <SelectPopup>
                    <SelectList>
                      @for (const opt of BUILD_TYPE_OPTIONS; key opt.value) {
                        <SelectItem value={opt.value}>
                          <SelectItemText>{opt.label as string}</SelectItemText>
                          <SelectItemIndicator />
                        </SelectItem>;
                      }
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
          </div>
          @if (buildType === "legacy") {
            <div className="flex flex-col gap-4 sm:flex-row">
              <div className="flex flex-1 flex-col gap-2">
                <Label htmlFor="pages-source-branch">Branch</Label>
                <Input
                  id="pages-source-branch"
                  value={branch}
                  placeholder="main"
                  onInput={(e) =>
                    setBranch((e.currentTarget as HTMLInputElement).value)
                  }
                />
              </div>
              <div className="flex flex-1 flex-col gap-2">
                <Label>Path</Label>
                <SelectRoot
                  value={sourcePath}
                  onValueChange={(v) => setSourcePath(v === "/docs" ? "/docs" : "/")}
                >
                  <SelectTrigger className="w-full" aria-label="Source path">
                    <SelectValue placeholder="Select a path" />
                    <SelectIcon />
                  </SelectTrigger>
                  {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
escape hatch; popup is anchor-scoped under the Select root. */}
                  <SelectPortal>
                    <SelectPositioner>
                      <SelectPopup>
                        <SelectList>
                          @for (const opt of PATH_OPTIONS; key opt.value) {
                            <SelectItem value={opt.value}>
                              <SelectItemText>
                                {opt.label as string}
                              </SelectItemText>
                              <SelectItemIndicator />
                            </SelectItem>;
                          }
                        </SelectList>
                      </SelectPopup>
                    </SelectPositioner>
                  </SelectPortal>
                </SelectRoot>
              </div>
            </div>;
          } @else {
            <p className="text-[14px] text-muted-foreground">
              Publish with{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[13px]">
                actions/deploy-pages
              </code>{" "}
              from a job running in the{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[13px]">
                github-pages
              </code>{" "}
              environment with{" "}
              <code className="rounded bg-muted px-1 py-0.5 text-[13px]">
                pages: write
              </code>{" "}
              permission.
            </p>;
          }
          @if (site) {
            <>
              <div className="flex flex-col gap-2">
                <Label htmlFor="pages-cname">Custom domain</Label>
                <Input
                  id="pages-cname"
                  value={cname}
                  placeholder="docs.example.com"
                  onInput={(e) => setCname((e.currentTarget as HTMLInputElement).value)}
                />
                <p className="text-[13px] text-muted-foreground">
                  Stored on the site record; serving still happens at the site URL
                  above.
                </p>
              </div>
              <label className="flex items-center gap-2 text-[14px] text-foreground">
                <Switch
                  checked={httpsEnforced}
                  onCheckedChange={(v) => setHttpsEnforced(!!v)}
                />
                Enforce HTTPS
              </label>
            </>;
          }
          @if (error) {
            <p role="alert" className="text-[14px] text-destructive">
              {error as string}
            </p>;
          }
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              disabled={pending}
              onClick={() => void save()}
              data-testid="pages-save"
            >
              {(pending ? "Saving…" : site ? "Save changes" : "Enable Pages") as string}
            </Button>
            @if (site && site.build_type === "legacy") {
              <Button
                type="button"
                variant="secondary"
                disabled={buildPending}
                onClick={() => void requestBuild()}
                data-testid="pages-request-build"
              >
                {(buildPending ? "Building…" : "Rebuild site") as string}
              </Button>;
            }
            @if (site) {
              <Button
                type="button"
                variant="destructive"
                onClick={() => setDisableOpen(true)}
                data-testid="pages-disable"
              >
                Unpublish site
              </Button>;
            }
          </div>
          @if (site) {
            <div className="flex flex-col gap-2" data-testid="pages-builds">
              <h3 className="text-[15px] font-semibold text-foreground">Builds</h3>
              @if (buildsQ.isPending) {
                <p className="text-[14px] text-muted-foreground">Loading…</p>;
              } @else if (builds.length === 0) {
                <p className="text-[14px] text-muted-foreground">
                  No builds yet.
                </p>;
              } @else {
                <ul className="divide-y divide-border rounded-md border border-border">
                  @for (const b of builds; key b.id) {
                    <li
                      className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-3"
                      data-testid={`pages-build-${b.id}`}
                    >
                      <span
                        className={
                          b.status === "built"
                            ? "rounded-full bg-emerald-500/15 px-2 py-0.5 text-[12px] font-medium text-emerald-600 dark:text-emerald-400"
                            : b.status === "errored"
                              ? "rounded-full bg-destructive/15 px-2 py-0.5 text-[12px] font-medium text-destructive"
                              : "rounded-full bg-muted px-2 py-0.5 text-[12px] font-medium text-muted-foreground"
                        }
                      >
                        {b.status as string}
                      </span>
                      <code className="text-[13px] text-foreground">
                        {b.commit.slice(0, 8) as string}
                      </code>
                      <span className="text-[13px] text-muted-foreground">
                        {b.branch as string}
                      </span>
                      <span className="text-[13px] text-muted-foreground">
                        {b.build_type as string}
                      </span>
                      <span className="text-[13px] text-muted-foreground">
                        {formatBuildDuration(b.duration_ms) as string}
                      </span>
                      <span className="text-[13px] text-muted-foreground">
                        {b.created_at as string}
                      </span>
                      @if (b.error_message) {
                        <span className="w-full text-[13px] text-destructive">
                          {b.error_message as string}
                        </span>;
                      }
                    </li>;
                  }
                </ul>;
              }
            </div>;
          }
        </div>;
      }
    </div>
    @if (disableOpen) {
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4"
        role="dialog"
        aria-modal="true"
        aria-label="Unpublish Pages site"
      >
        <div className="w-full max-w-md rounded-md border border-border bg-card p-5 shadow-lg">
          <h3 className="text-[16px] font-semibold text-foreground">Unpublish Pages site?</h3>
          <p className="mt-2 text-[14px] text-muted-foreground">
            The site stops serving immediately. Build history is kept and you can re-enable
            Pages at any time.
          </p>
          <div className="mt-5 flex justify-end gap-3">
            <Button type="button" variant="secondary" onClick={() => setDisableOpen(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={disablePending}
              onClick={() => void disable()}
              data-testid="pages-disable-confirm"
            >
              {(disablePending ? "Unpublishing…" : "Unpublish site") as string}
            </Button>
          </div>
        </div>
      </div>;
    }
  </section>;
}
