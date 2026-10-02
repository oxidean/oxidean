import {
  Activity,
  BookOpen,
  Eye,
  GitFork,
  Link2,
  Scale,
  Settings,
  Star,
  User,
} from "@octanejs/lucide";
import type {
  PackagePublic,
  ReleasePublic,
  RepoContributorPublic,
  RepoLanguageStat,
  RepoPublic,
} from "@oxidean/api-client";
import { useState } from "octane";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { TopicChipsInput } from "@/components/repo/topics-input";
import { blobHref, activityHref } from "@/lib/repo-browse";
import { languagePercent } from "@/lib/language-bar";
import { useRepoStore } from "@/lib/repo-store";
import { safeExternalHttpUrl } from "@/lib/safe-external-url";
import { toastError, toastSuccess } from "@/lib/toast";

type RepoAboutSidebarProps = {
  repo: RepoPublic;
  latestRelease: ReleasePublic | null;
  packages: PackagePublic[];
  contributors: RepoContributorPublic[];
  languages?: RepoLanguageStat[];
  /** When true, show a Readme meta row linking to `#readme`. */
  hasReadme?: boolean;
  /** Ref used for License / Contributing blob links. */
  refName?: string;
  /** Detected root LICENSE path + sidebar label (e.g. "MIT license"). */
  license?: { path: string; label: string } | null;
  /** Detected root CONTRIBUTING path. */
  contributingPath?: string | null;
  /**
   * When true, show Activity linking to `/$owner/$repo/activity`.
   * Enable for non-empty repos (GitHub always surfaces the row when history exists).
   */
  showActivity?: boolean;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const META_ROW =
  "inline-flex w-full items-center gap-2 py-1 text-[12px] leading-5 text-muted-foreground no-underline hover:text-foreground";
const META_ROW_PLAIN =
  "inline-flex w-full items-center gap-2 py-1 text-[12px] leading-5 text-muted-foreground";

/** Right-rail About panel for Code home / tree (issue #23). */
export function RepoAboutSidebar({
  repo,
  latestRelease,
  packages,
  contributors,
  languages = [],
  hasReadme = false,
  refName,
  license = null,
  contributingPath = null,
  showActivity = false,
}: RepoAboutSidebarProps) @{
  const storeRepo = useRepoStore((s) => s.repo);
  const setRepo = useRepoStore((s) => s.setRepo);
  // Prefer store so star/watch/fork counts and metadata live-update with chrome.
  const live = storeRepo && storeRepo.id === repo.id ? storeRepo : repo;
  const owner = live.owner_username;
  const name = live.name;
  const base = `/${owner}/${name}`;
  const canAdmin = !!live.can_admin;
  const canWrite = !!live.can_write;
  const browseRef = (refName || live.default_branch || "main").trim() || "main";
  const licenseHref =
    license && license.path ? blobHref(owner, name, browseRef, license.path) : null;
  const licenseText = (license?.label || "License").trim() || "License";
  const contributingHref = contributingPath
    ? blobHref(owner, name, browseRef, contributingPath)
    : null;
  const activityHrefResolved = activityHref(owner, name);

  const [editing, setEditing] = useState(false);
  const [description, setDescription] = useState(live.description ?? "");
  const [homepage, setHomepage] = useState(live.homepage ?? "");
  const [editTopics, setEditTopics] = useState<string[]>(() => live.topics ?? []);
  const [pending, setPending] = useState(false);

  const topics = live.topics ?? [];
  const desc = (live.description ?? "").trim();
  const home = (live.homepage ?? "").trim();
  const homeHref = safeExternalHttpUrl(home);
  const starCount = live.star_count ?? 0;
  const watchCount = live.watch_count ?? 0;
  const forkCount = live.fork_count ?? 0;
  const starLabel = starCount === 1 ? "1 star" : `${starCount} stars`;
  const watchLabel = watchCount === 1 ? "1 watching" : `${watchCount} watching`;
  const forkLabel = forkCount === 1 ? "1 fork" : `${forkCount} forks`;
  const langTotal = languages.reduce((sum, l) => sum + Number(l.bytes || 0), 0);
  const contributorCount = contributors.length;

  async function saveAbout() {
    setPending(true);
    try {
      const res = await apiClient.repo.updateMetadata({
        owner,
        name,
        description,
        homepage: homepage.trim(),
        topics: editTopics,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not update About.");
        setPending(false);
        return;
      }
      setRepo(res.data);
      setDescription(res.data.description ?? "");
      setHomepage(res.data.homepage ?? "");
      setEditTopics(res.data.topics ?? []);
      setEditing(false);
      toastSuccess("About updated.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  function beginEdit() {
    setDescription(live.description ?? "");
    setHomepage(live.homepage ?? "");
    setEditTopics(live.topics ?? []);
    setEditing(true);
  }

  <aside
    className="flex flex-col gap-0"
    aria-label="About this repository"
    data-testid="repo-about"
  >
    <section className="flex flex-col gap-3 pb-4" aria-labelledby="about-heading">
      <div className="flex items-center justify-between gap-2">
        <h2 id="about-heading" className="text-[14px] font-semibold text-foreground">
          About
        </h2>
        @if (canAdmin && !editing) {
          <button
            type="button"
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Edit About"
            onClick={() => beginEdit()}
          >
            <Settings aria-hidden size={16} />
          </button>;
        }
      </div>

      @if (editing) {
        <div className="flex flex-col gap-3">
          <Textarea
            value={description}
            onInput={(e) => setDescription((e.currentTarget as HTMLTextAreaElement).value)}
            disabled={pending}
            rows={3}
            maxLength={350}
            aria-label="Description"
          />
          <Input
            value={homepage}
            onInput={(e) => setHomepage((e.currentTarget as HTMLInputElement).value)}
            disabled={pending}
            placeholder="Homepage URL"
            aria-label="Homepage"
            autoComplete="off"
          />
          <TopicChipsInput
            id="about-topics"
            topics={editTopics}
            onTopicsChange={setEditTopics}
            disabled={pending}
            placeholder="Add topics…"
            ariaLabel="Topics"
          />
          <div className="flex flex-wrap gap-2">
            <Button type="button" disabled={pending} onClick={() => void saveAbout()}>
              {(pending ? "Saving…" : "Save") as string}
            </Button>
            <Button
              type="button"
              variant="secondary"
              disabled={pending}
              onClick={() => {
                setDescription(live.description ?? "");
                setHomepage(live.homepage ?? "");
                setEditTopics(live.topics ?? []);
                setEditing(false);
              }}
            >
              Cancel
            </Button>
          </div>
        </div>;
      } @else {
        <>
          @if (desc) {
            <p className="text-[14px] leading-[1.5] text-foreground">
              {desc as string}
            </p>;
          } @else {
            <p className="text-[14px] text-muted-foreground">
              No description provided.
            </p>;
          }
          @if (homeHref) {
            <a
              href={homeHref as string}
              className="inline-flex max-w-full items-center gap-1.5 truncate text-[12px] text-primary no-underline hover:underline"
              rel="noopener noreferrer"
              target="_blank"
            >
              <Link2 aria-hidden size={14} className="shrink-0" />
              <span className="truncate">{home as string}</span>
            </a>;
          } @else {
            @if (home) {
              <span className="inline-flex max-w-full items-center gap-1.5 truncate text-[12px] text-muted-foreground">
                <Link2 aria-hidden size={14} className="shrink-0" />
                <span className="truncate">{home as string}</span>
              </span>;
            }
          }
          @if (topics.length > 0) {
            <ul className="flex flex-wrap gap-1.5" aria-label="Topics">
              @for (const topic of topics; key topic) {
                <li className="rounded-full border border-border bg-primary/10 px-2.5 py-0.5 text-[12px] text-primary">
                  {topic as string}
                </li>;
              }
            </ul>;
          }
        </>;
      }

      <div
        className="mt-1 flex flex-col"
        aria-label="Repository metadata"
        data-testid="repo-about-social"
      >
        @if (hasReadme) {
          <a href="#readme" className={META_ROW as string}>
            <BookOpen aria-hidden size={16} className="shrink-0" />
            <span>Readme</span>
          </a>;
        }
        @if (licenseHref) {
          <a href={licenseHref as string} className={META_ROW as string}>
            <Scale aria-hidden size={16} className="shrink-0" />
            <span>{licenseText as string}</span>
          </a>;
        }
        @if (contributingHref) {
          <a href={contributingHref as string} className={META_ROW as string}>
            <User aria-hidden size={16} className="shrink-0" />
            <span>Contributing</span>
          </a>;
        }
        @if (showActivity) {
          <a href={activityHrefResolved as string} className={META_ROW as string}>
            <Activity aria-hidden size={16} className="shrink-0" />
            <span>Activity</span>
          </a>;
        }
        @if (canWrite) {
          <a href={(base + "/stargazers") as string} className={META_ROW as string}>
            <Star aria-hidden size={16} className="shrink-0" />
            <span>{starLabel as string}</span>
          </a>;
        } @else {
          <span className={META_ROW_PLAIN as string}>
            <Star aria-hidden size={16} className="shrink-0" />
            <span>{starLabel as string}</span>
          </span>;
        }
        <a href={(base + "/watchers") as string} className={META_ROW as string}>
          <Eye aria-hidden size={16} className="shrink-0" />
          <span>{watchLabel as string}</span>
        </a>
        <a href={(base + "/forks") as string} className={META_ROW as string}>
          <GitFork aria-hidden size={16} className="shrink-0" />
          <span>{forkLabel as string}</span>
        </a>
      </div>
    </section>

    <hr className="border-border" />

    <section className="flex flex-col gap-2 py-4" aria-labelledby="releases-heading">
      <h2 id="releases-heading" className="text-[14px] font-semibold text-foreground">
        Releases
      </h2>
      @if (latestRelease) {
        <>
          <a
            href={
              (base + "/releases/" + encodeURIComponent(latestRelease.tag_name)) as string
            }
            className="text-[14px] text-primary no-underline hover:underline"
          >
            {(latestRelease.title || latestRelease.tag_name) as string}
          </a>
          <a
            href={(base + "/releases") as string}
            className="text-[12px] text-muted-foreground no-underline hover:text-foreground hover:underline"
          >
            View all releases
          </a>
        </>;
      } @else {
        <>
          <p className="text-[12px] text-muted-foreground">No releases published</p>
          @if (canWrite) {
            <a
              href={(base + "/releases/new") as string}
              className="text-[12px] text-primary no-underline hover:underline"
            >
              Create a new release
            </a>;
          }
        </>;
      }
    </section>

    <hr className="border-border" />

    <section className="flex flex-col gap-2 py-4" aria-labelledby="packages-heading">
      <h2 id="packages-heading" className="text-[14px] font-semibold text-foreground">
        Packages
      </h2>
      @if (packages.length > 0) {
        <>
          <ul className="flex flex-col gap-1">
            @for (const pkg of packages.slice(0, 5); key pkg.id) {
              <li className="truncate text-[14px] text-foreground">
                {pkg.name as string}
              </li>;
            }
          </ul>
          <a
            href={(base + "/packages") as string}
            className="text-[12px] text-muted-foreground no-underline hover:text-foreground hover:underline"
          >
            View packages
          </a>
        </>;
      } @else {
        <>
          <p className="text-[12px] text-muted-foreground">No packages published</p>
          @if (canWrite) {
            <a
              href={(base + "/packages") as string}
              className="text-[12px] text-primary no-underline hover:underline"
            >
              Publish your first package
            </a>;
          } @else {
            <a
              href={(base + "/packages") as string}
              className="text-[12px] text-muted-foreground no-underline hover:text-foreground hover:underline"
            >
              View packages
            </a>;
          }
        </>;
      }
    </section>

    <hr className="border-border" />

    <section className="flex flex-col gap-2 py-4" aria-labelledby="contributors-heading">
      <div className="flex items-center gap-2">
        <h2 id="contributors-heading" className="text-[14px] font-semibold text-foreground">
          Contributors
        </h2>
        @if (contributorCount > 0) {
          <Badge
            variant="outline"
            className="rounded-full px-1.5 py-0 text-[11px] font-medium"
          >
            {String(contributorCount)}
          </Badge>;
        }
      </div>
      @if (contributors.length > 0) {
        <ul className="flex flex-col gap-2" aria-label="Contributors">
          @for (const c of contributors.slice(0, 8); key (c.username || c.display_name) +
                String(c.commit_count)) {
            <li className="flex min-w-0 items-center gap-2">
              @if (c.username) {
                <>
                  <a
                    href={("/" + c.username) as string}
                    className="inline-flex h-7 w-7 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[11px] font-semibold text-foreground no-underline"
                    title={
                      (c.display_name +
                        " · " +
                        String(c.commit_count) +
                        " commits") as string
                    }
                  >
                    @if (c.avatar_url) {
                      <img
                        src={c.avatar_url as string}
                        alt=""
                        className="block h-full w-full object-cover"
                      />;
                    } @else {
                      <span
                        aria-hidden
                        className="flex size-full items-center justify-center leading-none"
                      >
                        {c.display_name.slice(0, 1).toUpperCase() as string}
                      </span>;
                    }
                  </a>
                  <div className="min-w-0 truncate text-[12px] leading-5">
                    <a
                      href={("/" + c.username) as string}
                      className="font-semibold text-foreground no-underline hover:underline"
                    >
                      {c.username as string}
                    </a>
                    @if (
                      c.display_name &&
                      c.display_name !== c.username
                    ) {
                      <span className="text-muted-foreground">
                        {" " + c.display_name}
                      </span>;
                    }
                  </div>
                </>;
              } @else {
                <>
                  <span
                    className="inline-flex h-7 w-7 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[11px] font-semibold text-foreground"
                    title={
                      (c.display_name +
                        " · " +
                        String(c.commit_count) +
                        " commits") as string
                    }
                  >
                    <span
                      aria-hidden
                      className="flex size-full items-center justify-center leading-none"
                    >
                      {c.display_name.slice(0, 1).toUpperCase() as string}
                    </span>
                  </span>
                  <span className="truncate text-[12px] font-semibold text-foreground">
                    {c.display_name as string}
                  </span>
                </>;
              }
            </li>;
          }
        </ul>;
      } @else {
        <p className="text-[12px] text-muted-foreground">No contributors yet</p>;
      }
    </section>

    @if (languages.length > 0 && langTotal > 0) {
      <>
        <hr className="border-border" />
        <section
          className="flex flex-col gap-2 py-4"
          aria-labelledby="languages-heading"
          data-testid="repo-about-languages"
        >
          <h2 id="languages-heading" className="text-[14px] font-semibold text-foreground">
            Languages
          </h2>
          <div
            className="flex h-2 w-full overflow-hidden rounded-full bg-muted"
            role="img"
            aria-label="Language breakdown"
          >
            @for (const lang of languages; key lang.name) {
              <span
                className="h-full min-w-0"
                style={{
                  width: `${Math.max((Number(lang.bytes) / langTotal) * 100, 0.5)}%`,
                  backgroundColor: (lang.color || "var(--muted-foreground)") as string,
                }}
                title={
                  (lang.name +
                    " " +
                    languagePercent(Number(lang.bytes), langTotal)) as string
                }
              />;
            }
          </div>
          <ul className="grid grid-cols-1 gap-x-3 gap-y-1 sm:grid-cols-2">
            @for (const lang of languages; key lang.name + "-legend") {
              <li className="flex min-w-0 items-center gap-1.5 text-[12px] leading-5">
                <span
                  className="inline-block h-2.5 w-2.5 shrink-0 rounded-full"
                  style={{
                    backgroundColor: (lang.color ||
                      "var(--muted-foreground)") as string,
                  }}
                  aria-hidden
                />
                @if (lang.name === "Other") {
                  <span className="truncate font-semibold text-foreground">
                    {lang.name as string}
                  </span>;
                } @else {
                  <a
                    href={
                      (base +
                        "/search?q=" +
                        encodeURIComponent("language:" + lang.name) +
                        "&type=code") as string
                    }
                    className="truncate font-semibold text-foreground no-underline hover:text-link hover:underline"
                  >
                    {lang.name as string}
                  </a>;
                }
                <span className="shrink-0 text-muted-foreground">
                  {languagePercent(Number(lang.bytes), langTotal) as string}
                </span>
              </li>;
            }
          </ul>
        </section>
      </>;
    }
  </aside>;
}
