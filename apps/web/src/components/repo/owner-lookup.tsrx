import type { OrgMineEntry, OwnerType, UserLookupHit } from "@oxidean/api-client";
import { userLookupQueryOptions } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { orgListMineQueryOptions } from "@/lib/session-queries";
import { cn } from "@/lib/utils";

type OwnerLookupProps = {
  id?: string;
  label?: string;
  /** Destination kind — users via `user.lookup`, orgs via `org.listMine` (admin/owner). */
  ownerType: OwnerType;
  value: string;
  onValueChange: (slug: string) => void;
  onPick?: (
    hit: { kind: "user"; user: UserLookupHit } | { kind: "org"; org: OrgMineEntry },
  ) => void;
  disabled?: boolean;
  placeholder?: string;
};

type FlatHit =
  | {
      key: string;
      kind: "user";
      label: string;
      sub?: string;
      avatar?: string | null;
      user: UserLookupHit;
    }
  | {
      key: string;
      kind: "org";
      label: string;
      sub?: string;
      avatar?: string | null;
      org: OrgMineEntry;
    };

/**
 * Autocomplete for transfer / create destination owners.
 * Users: live `user.lookup` (prefix ≥ 2). Orgs: filter orgs you admin/own.
 */
export function OwnerLookup({
  id = "owner-lookup",
  label = "Destination owner",
  ownerType,
  value,
  onValueChange,
  onPick,
  disabled,
  placeholder,
}: OwnerLookupProps) @{
  const [debounced, setDebounced] = useState(value.trim());
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);

  useEffect(() => {
    const t = window.setTimeout(() => {
      setDebounced(value.trim());
    }, 200);
    return () => window.clearTimeout(t);
  }, [value]);

  const prefix = debounced;
  const userEnabled = ownerType === "user" && prefix.length >= 2;
  const orgEnabled = ownerType === "org";

  const usersQ = useQuery({
    ...userLookupQueryOptions(apiClient, { prefix }),
    enabled: userEnabled,
  });

  const orgsQ = useQuery({
    ...orgListMineQueryOptions(),
    enabled: orgEnabled,
  });

  const hits: FlatHit[] = (() => {
    if (ownerType === "user") {
      if (!userEnabled) return [];
      return (usersQ.data?.users ?? []).map((u) => ({
        key: `user:${u.username}`,
        kind: "user" as const,
        label: `@${u.username}`,
        sub: u.display_name?.trim() || undefined,
        avatar: u.avatar_url,
        user: u,
      }));
    }
    const needle = prefix.toLowerCase();
    const orgs = (orgsQ.data ?? []).filter((o) => {
      const role = (o.role ?? "").toLowerCase();
      if (role !== "owner" && role !== "admin") return false;
      if (!needle) return true;
      return (
        o.slug.toLowerCase().includes(needle) ||
        (o.display_name ?? "").toLowerCase().includes(needle)
      );
    });
    return orgs.map((o) => ({
      key: `org:${o.slug}`,
      kind: "org" as const,
      label: o.slug,
      sub: o.display_name?.trim() || "Organization",
      avatar: null,
      org: o,
    }));
  })();

  const fetching = ownerType === "user" ? usersQ.isFetching : orgsQ.isFetching;

  useEffect(() => {
    setActiveIndex(-1);
    const show =
      ownerType === "user"
        ? userEnabled && (fetching || hits.length > 0)
        : orgEnabled && (fetching || hits.length > 0 || prefix.length > 0);
    setOpen(show && hits.length > 0);
  }, [ownerType, userEnabled, orgEnabled, fetching, hits.length, prefix]);

  function choose(hit: FlatHit) {
    if (hit.kind === "user") {
      onValueChange(hit.user.username);
      onPick?.({ kind: "user", user: hit.user });
    } else {
      onValueChange(hit.org.slug);
      onPick?.({ kind: "org", org: hit.org });
    }
    setOpen(false);
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (!open || hits.length === 0) return;
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      setActiveIndex((i) => (i + 1) % hits.length);
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      setActiveIndex((i) => (i <= 0 ? hits.length - 1 : i - 1));
    } else if (ev.key === "Enter" && activeIndex >= 0) {
      ev.preventDefault();
      const hit = hits[activeIndex];
      if (hit) choose(hit);
    } else if (ev.key === "Escape") {
      setOpen(false);
    }
  }

  const listLabel = ownerType === "user" ? "Matching users" : "Matching organizations";
  const emptyHint =
    ownerType === "user"
      ? "Type at least 2 characters to search users"
      : "No organizations you admin match this query";

  <div className="relative flex flex-col gap-2" data-testid="owner-lookup">
    <Label htmlFor={id}>{label as string}</Label>
    <Input
      id={id}
      name="dest-owner"
      autoComplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls={`${id}-listbox`}
      aria-autocomplete="list"
      disabled={disabled}
      value={value}
      placeholder={
        (placeholder ??
          (ownerType === "user" ? "Search username…" : "Search organization…")) as string
      }
      onInput={(ev) => {
        onValueChange((ev.currentTarget as HTMLInputElement).value);
      }}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (hits.length > 0) setOpen(true);
      }}
      onBlur={() => {
        window.setTimeout(() => setOpen(false), 150);
      }}
    />
    @if (open && hits.length > 0) {
      <ul
        id={`${id}-listbox`}
        role="listbox"
        aria-label={listLabel as string}
        className="absolute top-full z-20 mt-1 max-h-60 w-full overflow-auto rounded-md border border-border bg-card py-1 shadow-md"
        data-testid="owner-lookup-listbox"
      >
        @for (const hit of hits; key hit.key) {
          <li role="option" aria-selected={hits[activeIndex]?.key === hit.key}>
            <button
              type="button"
              className={cn(
                "flex w-full items-center gap-3 px-3 py-2 text-left text-[14px] outline-none",
                "hover:bg-muted focus-visible:bg-muted",
                hits[activeIndex]?.key === hit.key && "bg-muted",
              )}
              onMouseDown={(ev) => {
                ev.preventDefault();
                choose(hit);
              }}
            >
              @if (hit.avatar) {
                <img
                  src={hit.avatar as string}
                  alt=""
                  width={28}
                  height={28}
                  className="size-7 shrink-0 rounded-md object-cover"
                />;
              } @else {
                <span
                  aria-hidden
                  className="inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-muted text-[11px] font-semibold"
                >
                  {
                    hit.label
                      .replace(/^@/, "")
                      .slice(0, 2)
                      .toUpperCase() as string
                  }
                </span>;
              }
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium text-foreground">
                  {hit.label as string}
                </span>
                @if (hit.sub) {
                  <span className="block truncate text-[13px] text-muted-foreground">
                    {hit.sub as string}
                  </span>;
                }
              </span>
              <span className="shrink-0 text-[12px] text-muted-foreground">
                {(hit.kind === "user" ? "User" : "Org") as string}
              </span>
            </button>
          </li>;
        }
      </ul>;
    } @else {
      @if (
        ownerType === "org" &&
        orgEnabled &&
        !fetching &&
        prefix.length > 0 &&
        hits.length === 0
      ) {
        <p className="text-[13px] text-muted-foreground" data-testid="owner-lookup-empty">
          {emptyHint as string}
        </p>;
      }
    }
  </div>;
}
