import { navigate } from "astro:transitions/client";
import {
  Building2,
  Code2,
  GitCommitHorizontal,
  GitPullRequest,
  CircleDot,
  Lock,
  Search,
  Users,
} from "@octanejs/lucide";
import { useEffect, useEffectEvent, useState } from "octane";
import { Input } from "@/components/ui/input";
import {
  fetchSearchSuggestions,
  SEARCH_ENTITY_TYPES,
  searchResultsHref,
  type SearchActionSuggestion,
  type SearchEntityType,
  type SearchOrgSuggestion,
  type SearchRepoSuggestion,
  type SearchSuggestions,
  type SearchUserSuggestion,
} from "@/lib/global-search-suggest";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import {
  WEBMCP_SEARCH_FORM_ATTRS,
  WEBMCP_SEARCH_QUERY_ATTRS,
  WEBMCP_SEARCH_TYPE_ATTRS,
} from "@/lib/webmcp";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

/** Header omnibar — multi-entity suggest + full /search results. */
export const GLOBAL_SEARCH_LABEL = "Search Oxidean";

export const GLOBAL_SEARCH_PLACEHOLDER = "Search repositories, users, orgs…";

type GlobalSearchProps = {
  className?: string;
};

type FlatItem =
  | { key: string; kind: "repo"; item: SearchRepoSuggestion }
  | { key: string; kind: "user"; item: SearchUserSuggestion }
  | { key: string; kind: "org"; item: SearchOrgSuggestion }
  | { key: string; kind: "action"; item: SearchActionSuggestion };

function flatten(s: SearchSuggestions | null): FlatItem[] {
  if (!s) return [];
  const out: FlatItem[] = [];
  for (const r of s.repositories) out.push({ key: `repo:${r.id}`, kind: "repo", item: r });
  for (const u of s.users) out.push({ key: `user:${u.username}`, kind: "user", item: u });
  for (const o of s.organizations) out.push({ key: `org:${o.slug}`, kind: "org", item: o });
  for (const a of s.actions) out.push({ key: `action:${a.type}`, kind: "action", item: a });
  return out;
}

function actionIcon(type: SearchActionSuggestion["type"]) {
  switch (type) {
    case "users":
      return Users;
    case "organizations":
      return Building2;
    case "code":
      return Code2;
    case "commits":
      return GitCommitHorizontal;
    case "issues":
      return CircleDot;
    case "pulls":
      return GitPullRequest;
    default:
      return Search;
  }
}

export function GlobalSearch({ className }: GlobalSearchProps) @{
  const { user } = useChromeAccountState();
  const signedIn = !!user;
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [suggest, setSuggest] = useState<SearchSuggestions | null>(null);
  const [loading, setLoading] = useState(false);

  const items = flatten(suggest);

  useEffect(() => {
    const trimmed = q.trim();
    if (!trimmed) {
      setSuggest(null);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    const t = window.setTimeout(() => {
      void fetchSearchSuggestions(trimmed, { signedIn }).then((next) => {
        if (cancelled) return;
        setSuggest(next);
        setLoading(false);
        setActiveIndex(-1);
      });
    }, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [q, signedIn]);

  const onDocPointer = useEffectEvent((event: PointerEvent) => {
    const t = event.target;
    if (!(t instanceof Element)) return;
    if (t.closest("[data-global-search]")) return;
    setOpen(false);
  });

  useEffect(() => {
    if (!open) return;
    document.addEventListener("pointerdown", onDocPointer);
    return () => document.removeEventListener("pointerdown", onDocPointer);
  }, [open]);

  function submitSearch(form: HTMLFormElement) {
    const fd = new FormData(form);
    const qEntry = fd.get("q");
    const typeEntry = fd.get("type");
    const nextQ = (typeof qEntry === "string" ? qEntry : q).trim();
    const rawType = (typeof typeEntry === "string" ? typeEntry : "repositories").trim();
    const nextType: SearchEntityType = (SEARCH_ENTITY_TYPES as readonly string[]).includes(rawType)
      ? (rawType as SearchEntityType)
      : "repositories";
    void navigate(searchResultsHref(nextQ, nextType));
  }

  function activate(item: FlatItem) {
    void navigate(item.item.href);
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (ev.key === "Escape") {
      setOpen(false);
      return;
    }
    if (!open || items.length === 0) return;
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      setActiveIndex((i) => (i + 1) % items.length);
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      setActiveIndex((i) => (i <= 0 ? items.length - 1 : i - 1));
    } else if (ev.key === "Enter" && activeIndex >= 0) {
      ev.preventDefault();
      const hit = items[activeIndex];
      if (hit) activate(hit);
    }
  }

  const showPanel = open && q.trim().length > 0;

  <div className={cn("relative min-w-0", className)} data-global-search data-testid="global-search">
    <form
      method="get"
      action="/search"
      className="relative"
      {...WEBMCP_SEARCH_FORM_ATTRS}
      onSubmit={(e) => {
        e.preventDefault();
        if (activeIndex >= 0 && items[activeIndex]) {
          activate(items[activeIndex]!);
          return;
        }
        submitSearch(e.currentTarget as HTMLFormElement);
      }}
    >
      <Search
        aria-hidden
        size={16}
        className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground sm:left-3"
      />
      <Input
        name="q"
        role="combobox"
        aria-expanded={showPanel}
        aria-controls="global-search-listbox"
        aria-autocomplete="list"
        aria-activedescendant={
          activeIndex >= 0 && items[activeIndex]
            ? (`global-search-opt-${items[activeIndex]!.key}` as string)
            : undefined
        }
        className="h-9 w-full border-border/80 bg-muted/50 pl-8 text-[13px] shadow-none sm:h-10 sm:pl-9 sm:text-[14px]"
        placeholder={GLOBAL_SEARCH_PLACEHOLDER}
        value={q as string}
        onInput={(e) => {
          setQ((e.currentTarget as HTMLInputElement).value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
        title="Search repositories, users, organizations, and more"
        aria-label={GLOBAL_SEARCH_LABEL}
        data-testid="global-search-input"
        {...WEBMCP_SEARCH_QUERY_ATTRS}
      />
      <input type="hidden" name="type" value="repositories" {...WEBMCP_SEARCH_TYPE_ATTRS} />
    </form>

    @if (showPanel) {
      <div
        id="global-search-listbox"
        role="listbox"
        aria-label="Search suggestions"
        className="absolute inset-x-0 top-[calc(100%+0.35rem)] z-50 max-h-[min(70vh,28rem)] overflow-y-auto rounded-md border border-border bg-popover text-popover-foreground shadow-lg"
        data-testid="global-search-dropdown"
      >
        @if (loading && !suggest) {
          <p className="px-3 py-3 text-[13px] text-muted-foreground">Searching…</p>;
        } @else {
          <>
            @if (
              suggest &&
              suggest.repositories.length > 0
            ) {
              <>
                <p className="px-3 pt-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Repositories
                </p>
                @for (const r of suggest.repositories; key r.id) {
                  const idx = items.findIndex(
                    (i) => i.key === `repo:${r.id}`,
                  );
                  const active = idx === activeIndex;
                  <AppLink
                    id={`global-search-opt-repo:${r.id}`}
                    role="option"
                    aria-selected={active}
                    href={r.href as string}
                    className={cn(
                      "flex items-center gap-2 px-3 py-2 text-[13px] no-underline",
                      active
                        ? "bg-accent text-accent-foreground"
                        : "text-foreground hover:bg-muted/80",
                    )}
                    onMouseEnter={() => setActiveIndex(idx)}
                  >
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {(r.owner + "/" + r.name) as string}
                    </span>
                    @if (
                      r.visibility === "private"
                    ) {
                      <Lock
                        aria-hidden
                        size={12}
                        className="shrink-0 text-muted-foreground"
                      />;
                    }
                  </AppLink>;
                }
              </>;
            }

            @if (suggest && suggest.users.length > 0) {
              <>
                <p className="px-3 pt-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Users
                </p>
                @for (const u of suggest.users; key u.username) {
                  const idx = items.findIndex(
                    (i) => i.key === `user:${u.username}`,
                  );
                  const active = idx === activeIndex;
                  <AppLink
                    id={`global-search-opt-user:${u.username}`}
                    role="option"
                    aria-selected={active}
                    href={u.href as string}
                    className={cn(
                      "flex items-center gap-2 px-3 py-2 text-[13px] no-underline",
                      active
                        ? "bg-accent text-accent-foreground"
                        : "text-foreground hover:bg-muted/80",
                    )}
                    onMouseEnter={() => setActiveIndex(idx)}
                  >
                    <Users
                      aria-hidden
                      size={14}
                      className="shrink-0 text-muted-foreground"
                    />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {`@${u.username}` as string}
                    </span>
                    @if (u.display_name) {
                      <span className="truncate text-muted-foreground">
                        {u.display_name as string}
                      </span>;
                    }
                  </AppLink>;
                }
              </>;
            }

            @if (
              suggest &&
              suggest.organizations.length > 0
            ) {
              <>
                <p className="px-3 pt-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Organizations
                </p>
                @for (const o of suggest.organizations; key o.slug) {
                  const idx = items.findIndex(
                    (i) => i.key === `org:${o.slug}`,
                  );
                  const active = idx === activeIndex;
                  <AppLink
                    id={`global-search-opt-org:${o.slug}`}
                    role="option"
                    aria-selected={active}
                    href={o.href as string}
                    className={cn(
                      "flex items-center gap-2 px-3 py-2 text-[13px] no-underline",
                      active
                        ? "bg-accent text-accent-foreground"
                        : "text-foreground hover:bg-muted/80",
                    )}
                    onMouseEnter={() => setActiveIndex(idx)}
                  >
                    <Building2
                      aria-hidden
                      size={14}
                      className="shrink-0 text-muted-foreground"
                    />
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {o.slug as string}
                    </span>
                  </AppLink>;
                }
              </>;
            }

            @if (suggest && suggest.actions.length > 0) {
              <div className="mt-1 border-t border-border pt-1">
                @for (const a of suggest.actions; key a.type) {
                  const idx = items.findIndex(
                    (i) => i.key === `action:${a.type}`,
                  );
                  const active = idx === activeIndex;
                  const Icon = actionIcon(a.type);
                  <AppLink
                    id={`global-search-opt-action:${a.type}`}
                    role="option"
                    aria-selected={active}
                    href={a.href as string}
                    className={cn(
                      "flex items-center gap-2 px-3 py-2 text-[13px] no-underline",
                      active
                        ? "bg-accent text-accent-foreground"
                        : "text-muted-foreground hover:bg-muted/80 hover:text-foreground",
                    )}
                    onMouseEnter={() => setActiveIndex(idx)}
                  >
                    <Icon aria-hidden size={14} className="shrink-0" />
                    <span className="min-w-0 flex-1 truncate">
                      {a.label as string}
                    </span>
                  </AppLink>;
                }
              </div>;
            }

            @if (
              suggest &&
              suggest.repositories.length === 0 &&
              suggest.users.length === 0 &&
              suggest.organizations.length === 0 &&
              !loading
            ) {
              <p className="px-3 py-2 text-[13px] text-muted-foreground">
                No direct matches — choose a search below.
              </p>;
            }
          </>;
        }
      </div>;
    }
  </div>;
}
