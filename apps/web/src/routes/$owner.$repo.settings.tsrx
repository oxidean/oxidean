import { navigate as astroNavigate } from "astro:transitions/client";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePathname } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect } from "octane";
import { CollaboratorsPanel } from "@/components/repo/collaborators-panel";
import { BranchProtectionPanel } from "@/components/repo/branch-protection-panel";
import { DeployKeysPanel } from "@/components/repo/deploy-keys-panel";
import { LfsBrowser } from "@/components/repo/lfs-browser";
import { ActionsSettingsPanel } from "@/components/repo/actions-settings-panel";
import { GitQuotaSettingsPanel } from "@/components/repo/git-quota-settings-panel";
import { LfsSettingsPanel } from "@/components/repo/lfs-settings-panel";
import { TemplateRepoSettingsPanel } from "@/components/repo/template-repo-settings-panel";
import { MergeSettingsPanel } from "@/components/repo/merge-settings-panel";
import { PagesSettingsPanel } from "@/components/repo/pages-settings-panel";
import { GeneralSettingsPanel } from "@/components/repo/general-settings-panel";
import { DangerZonePanel } from "@/components/repo/danger-zone-panel";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoFeaturesPanel } from "@/components/repo/repo-features-panel";
import { WebhooksPanel } from "@/components/repo/webhooks-panel";
import { MirrorSettingsPanel } from "@/components/repo/mirror-settings-panel";
import { VcsSettingsPanel } from "@/components/repo/vcs-settings-panel";
import { RepoSettingsLayout } from "@/components/settings/settings-shell";
import type { RepoSettingsNavActive } from "@/components/repo/repo-settings-nav";

const SECTION_IDS = new Set<RepoSettingsNavActive>([
  "general",
  "access",
  "branches",
  "actions",
  "storage",
  "features",
  "integrations",
  "danger",
]);

/** Old in-page `#repo-settings-*` anchors → the sub-page that now owns them. */
const LEGACY_HASH: Record<string, RepoSettingsNavActive> = {
  "repo-settings-general": "general",
  "repo-settings-visibility": "general",
  "repo-settings-collaborators": "access",
  "repo-settings-deploy-keys": "access",
  "repo-settings-branch-protection": "branches",
  "repo-settings-merge": "branches",
  "repo-settings-actions": "actions",
  "repo-settings-storage": "storage",
  "repo-settings-lfs": "storage",
  "repo-settings-lfs-objects": "storage",
  "repo-settings-vcs": "features",
  "repo-settings-template": "features",
  "repo-settings-features": "features",
  "repo-settings-pages": "features",
  "repo-settings-webhooks": "integrations",
  "repo-settings-mirror": "integrations",
  "repo-settings-danger": "danger",
};

/** First panel on each page sits flush under the chrome — strip its self margin. */
const FLUSH_FIRST = "[&>*:first-child]:mt-0";

/** Admin settings sub-pages — `/{owner}/{repo}/settings/{section}` (D-ORG-04 / GIT-16/17). */
function RepoSettingsPageContent() @{
  const pathname = usePathname();
  const params = (matchPath("/$owner/$repo/settings/$section", pathname) ??
    matchPath("/$owner/$repo/settings", pathname) ??
    {}) as { owner?: string; repo?: string; section?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const rawSection = params.section;
  const section = SECTION_IDS.has(rawSection as RepoSettingsNavActive)
    ? (rawSection as RepoSettingsNavActive)
    : "general";

  const layout = useRepoLayoutData();
  const repo = layout?.repo ?? null;
  const layoutError = layout?.status === "error" ? layout.message : "";
  // Gate on API can_admin — not me.id === owner_id (D-ORG-04 / T-10-02).
  const canAdmin = !!repo?.can_admin;
  const showNotFound =
    !layout || layout.status === "not_found" || (layout.status === "ok" && !!repo && !canAdmin);

  useEffect(() => {
    if (!owner || !repoName) return;
    // Bookmarks to the monolith's `#repo-settings-*` anchors land on the
    // sub-page that owns the section now.
    const legacy = LEGACY_HASH[window.location.hash.replace(/^#/, "")];
    if (legacy) {
      if (legacy !== section) {
        void astroNavigate(`/${owner}/${repoName}/settings/${legacy}`, {
          history: "replace",
        });
      }
      return;
    }
    // Bare `/settings` (client-side) and unknown sections canonicalize.
    if (rawSection !== section) {
      history.replaceState(null, "", `/${owner}/${repoName}/settings/${section}`);
    }
  }, [pathname, section, rawSection, owner, repoName]);

  @if (showNotFound) {
    <RepoNotFound />;
  } @else if (layoutError) {
    <div className="max-w-lg py-6 text-center">
      <p role="alert" className="text-[16px] text-destructive">
        {layoutError as string}
      </p>
    </div>;
  } @else if (repo && canAdmin) {
    <RepoSettingsLayout owner={owner} repo={repoName} active={section}>
      @if (section === "general") {
        <GeneralSettingsPanel owner={owner} name={repoName} repo={repo} />;
      } @else if (section === "access") {
        <>
          <div className={FLUSH_FIRST}>
            <CollaboratorsPanel owner={owner} name={repoName} />
          </div>
          <DeployKeysPanel owner={owner} name={repoName} can_admin={canAdmin} />
        </>;
      } @else if (section === "branches") {
        <>
          <div className={FLUSH_FIRST}>
            <BranchProtectionPanel owner={owner} name={repoName} can_admin={canAdmin} />
          </div>
          <MergeSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
        </>;
      } @else if (section === "actions") {
        <div className={FLUSH_FIRST}>
          <ActionsSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
        </div>;
      } @else if (section === "storage") {
        <>
          <div className={FLUSH_FIRST}>
            <GitQuotaSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
          </div>
          <LfsSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
          <LfsBrowser owner={owner} name={repoName} />
        </>;
      } @else if (section === "features") {
        <>
          <div className={FLUSH_FIRST}>
            <VcsSettingsPanel
              owner={owner}
              name={repoName}
              can_admin={canAdmin}
              archived={!!repo.archived}
            />
          </div>
          <TemplateRepoSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
          <RepoFeaturesPanel owner={owner} name={repoName} can_admin={canAdmin} repo={repo} />
          <div className="mt-12">
            <PagesSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
          </div>
        </>;
      } @else if (section === "integrations") {
        <>
          <div className={FLUSH_FIRST}>
            <WebhooksPanel owner={owner} name={repoName} can_admin={canAdmin} />
          </div>
          <MirrorSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
        </>;
      } @else {
        <DangerZonePanel owner={owner} name={repoName} repo={repo} />;
      }
    </RepoSettingsLayout>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoSettingsPage() @{
  <AppPage>
    <RepoLayout>
      <RepoSettingsPageContent />
    </RepoLayout>
  </AppPage>;
}
