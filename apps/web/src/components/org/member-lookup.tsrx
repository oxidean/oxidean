import type { UserLookupContext, UserLookupHit } from "@oxidean/api-client";
import { userLookupQueryOptions } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

export type MemberLookupProps = {
  id?: string;
  name?: string;
  label?: string;
  value: string;
  onValueChange: (username: string) => void;
  onPick?: (hit: UserLookupHit) => void;
  disabled?: boolean;
  placeholder?: string;
  /** Tighter gap + filter-row sizing (issues/pulls toolbars). */
  compact?: boolean;
  className?: string;
  inputClassName?: string;
  /** Contextual exclude/rank for add-member / add-collaborator. */
  context?: UserLookupContext | null;
};

/**
 * Live username autocomplete via user.lookup (D-ORG-03 / T-10-03).
 * Renders username, display name, and avatar only — never email.
 */
export function MemberLookup({
  id = "member-lookup",
  name = "username",
  label = "Username",
  value,
  onValueChange,
  onPick,
  disabled,
  placeholder,
  compact = false,
  className,
  inputClassName,
  context = null,
}: MemberLookupProps) @{
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
  const enabled = prefix.length >= 2;

  const lookup = useQuery({
    ...userLookupQueryOptions(apiClient, {
      prefix,
      context: context ?? undefined,
    }),
    enabled,
  });

  const hits: UserLookupHit[] = enabled ? (lookup.data?.users ?? []) : [];

  useEffect(() => {
    setActiveIndex(-1);
    setOpen(enabled && (lookup.isFetching || hits.length > 0));
  }, [enabled, lookup.isFetching, hits.length, prefix]);

  function choose(hit: UserLookupHit) {
    onValueChange(hit.username);
    onPick?.(hit);
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

  <div className={cn("relative flex flex-col", compact ? "gap-1" : "gap-2", className)}>
    @if (compact) {
      <label htmlFor={id} className="text-sm">
        {label as string}
      </label>;
    } @else {
      <Label htmlFor={id}>{label as string}</Label>;
    }
    <Input
      id={id}
      name={name as string}
      autoComplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls={`${id}-listbox`}
      aria-autocomplete="list"
      disabled={disabled}
      value={value}
      placeholder={placeholder as string | undefined}
      className={
        cn(
          compact &&
            "h-auto min-w-[9rem] w-auto rounded-md border border-border bg-background px-2 py-1.5 text-sm shadow-none focus-visible:ring-1",
          inputClassName,
        ) as string
      }
      onInput={(ev) => {
        onValueChange((ev.currentTarget as HTMLInputElement).value);
      }}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (enabled && hits.length > 0) setOpen(true);
      }}
      onBlur={() => {
        // Delay so option click can fire.
        window.setTimeout(() => setOpen(false), 150);
      }}
    />
    @if (open && hits.length > 0) {
      <ul
        id={`${id}-listbox`}
        role="listbox"
        aria-label="Matching users"
        className="absolute top-full z-20 mt-1 max-h-60 w-full overflow-auto rounded-md border border-border bg-card py-1 shadow-md"
      >
        @for (const hit of hits; key hit.username) {
          <li
            role="option"
            aria-selected={hits[activeIndex]?.username === hit.username}
          >
            <button
              type="button"
              className={cn(
                "flex w-full items-center gap-3 px-3 py-2 text-left text-[14px] outline-none",
                "hover:bg-muted focus-visible:bg-muted",
                hits[activeIndex]?.username === hit.username && "bg-muted",
              )}
              onMouseDown={(ev) => {
                ev.preventDefault();
                choose(hit);
              }}
            >
              @if (hit.avatar_url) {
                <img
                  src={hit.avatar_url as string}
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
                  {hit.username.slice(0, 2).toUpperCase() as string}
                </span>;
              }
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium text-foreground">
                  {"@" + (hit.username as string)}
                </span>
                @if (hit.display_name?.trim()) {
                  <span className="block truncate text-[13px] text-muted-foreground">
                    {hit.display_name as string}
                  </span>;
                }
              </span>
            </button>
          </li>;
        }
      </ul>;
    }
  </div>;
}
