import {
  AppWindow,
  Boxes,
  Check,
  Cloud,
  Code2,
  Container,
  Cpu,
  FileText,
  HardDrive,
  LayoutTemplate,
  Monitor,
  Scale,
  Search,
  Server,
  Smartphone,
  Sparkles,
  Wrench,
  X,
} from "@octanejs/lucide";
import type { RepoTemplateOption } from "@oxidean/api-client";
import { memo, useCallback, useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

type TemplatePickerProps = {
  id: string;
  label: string;
  description: string;
  value: string;
  options: RepoTemplateOption[];
  noneLabel?: string;
  noneDescription?: string;
  /** When set and search is empty, only these groups are listed (e.g. Popular licenses). */
  browseGroups?: string[];
  searchPlaceholder?: string;
  onChange: (next: string) => void;
};

type TemplateCardProps = {
  option: RepoTemplateOption | null;
  noneLabel: string;
  noneDescription: string;
  active: boolean;
  onSelect: (id: string) => void;
};

function iconForGroup(group: string) {
  switch (group) {
    case "Systems":
      return Cpu;
    case "Backend":
      return Server;
    case "Frontend":
      return LayoutTemplate;
    case "Desktop":
      return Monitor;
    case "Mobile":
      return Smartphone;
    case "Infra":
      return Cloud;
    case "From template":
      return LayoutTemplate;
    case "Languages":
      return Code2;
    case "Tools":
      return Wrench;
    case "IDEs":
      return AppWindow;
    case "OS":
      return HardDrive;
    case "General":
      return FileText;
    case "Popular":
      return Sparkles;
    case "All SPDX":
      return Scale;
    default:
      return Boxes;
  }
}

function accentForGroup(group: string): string {
  switch (group) {
    case "Systems":
      return "bg-orange-500/15 text-orange-600 dark:text-orange-300";
    case "Backend":
      return "bg-sky-500/15 text-sky-700 dark:text-sky-300";
    case "Frontend":
      return "bg-violet-500/15 text-violet-700 dark:text-violet-300";
    case "Desktop":
      return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300";
    case "Mobile":
      return "bg-pink-500/15 text-pink-700 dark:text-pink-300";
    case "Infra":
      return "bg-cyan-500/15 text-cyan-700 dark:text-cyan-300";
    case "Languages":
      return "bg-amber-500/15 text-amber-800 dark:text-amber-300";
    case "Tools":
      return "bg-lime-500/15 text-lime-800 dark:text-lime-300";
    case "IDEs":
      return "bg-indigo-500/15 text-indigo-700 dark:text-indigo-300";
    case "OS":
      return "bg-slate-500/15 text-slate-700 dark:text-slate-300";
    case "Popular":
      return "bg-primary/15 text-primary";
    case "All SPDX":
      return "bg-muted text-muted-foreground";
    default:
      return "bg-muted text-muted-foreground";
  }
}

const TemplateCard = memo(function TemplateCard({
  option,
  noneLabel,
  noneDescription,
  active,
  onSelect,
}: TemplateCardProps) @{
  const group = option?.group ?? "General";
  const Icon = option ? iconForGroup(group) : Container;
  const title = option ? option.label : noneLabel;
  const provenance =
    option?.provenance === "instance"
      ? "Instance"
      : option?.provenance === "user"
        ? option.source_label
          ? `Template · ${option.source_label}`
          : "Template"
        : option?.provenance === "builtin"
          ? "Official"
          : null;
  const body = option
    ? [provenance, option.description || option.group].filter(Boolean).join(" — ")
    : noneDescription;
  const accent = option ? accentForGroup(group) : "bg-muted text-muted-foreground";

  <button
    type="button"
    data-template-id={option ? option.id : "none"}
    className={cn(
      "flex w-full items-start gap-3 rounded-lg border px-3 py-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
      active ? "border-primary bg-primary/5" : "border-border hover:bg-muted/50",
    )}
    onClick={() => onSelect(option ? option.id : "none")}
  >
    <span
      className={cn(
        "mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-md",
        accent as string,
      )}
    >
      <Icon aria-hidden size={20} />
    </span>
    <span className="min-w-0 flex-1">
      <span className="flex items-center gap-2 text-[14px] font-semibold text-foreground">
        {title as string}
        @if (active) {
          <Check aria-hidden size={16} className="text-primary" />;
        }
      </span>
      <span className="mt-0.5 block text-[13px] leading-snug text-muted-foreground">
        {body as string}
      </span>
    </span>
  </button>;
});

function closeDetails(root: HTMLElement | null) {
  const details = root?.closest("details") as HTMLDetailsElement | null;
  if (details) details.open = false;
}

/**
 * Field trigger + searchable card modal for stack / gitignore / license catalogs.
 *
 * Uses native `<details>`/`<summary>` so the overlay opens without Octane
 * hydration (Playwright e2e). No Dialog portal — portals remounting against
 * Octane DOM updates caused insertBefore crashes when selecting a card also
 * updated sibling form fields (e.g. stack → default .gitignore).
 */
export function TemplatePicker({
  id,
  label,
  description,
  value,
  options,
  noneLabel = "None",
  noneDescription = "Skip this template — leave the tree unchanged for this file.",
  browseGroups,
  searchPlaceholder = "Search templates…",
  onChange,
}: TemplatePickerProps) @{
  const [query, setQuery] = useState("");
  // The overlay and its card list mount only once the picker opens — ~hundreds
  // of nodes skipped per picker on first render.
  const [open, setOpen] = useState(false);

  const selected = value === "none" ? null : (options.find((o) => o.id === value) ?? null);

  const q = query.trim().toLowerCase();
  const filtered = !q
    ? browseGroups && browseGroups.length > 0
      ? options.filter((o) => browseGroups.includes(o.group))
      : options
    : options.filter(
        (o) =>
          o.label.toLowerCase().includes(q) ||
          o.group.toLowerCase().includes(q) ||
          o.description.toLowerCase().includes(q) ||
          o.id.toLowerCase().includes(q),
      );

  // Prefer Popular before lexicographic for license-style catalogs.
  const groups = [...new Set(filtered.map((o) => o.group))].sort((a, b) => {
    if (a === "Popular") return -1;
    if (b === "Popular") return 1;
    return a.localeCompare(b);
  });

  const grouped = groups.map((group) => ({
    group,
    options: filtered.filter((o) => o.group === group),
  }));

  // Stable across renders so memo(TemplateCard) can bail — callers pair this
  // with a useCallback'd onChange.
  const choose = useCallback(
    (next: string) => {
      // Apply selection then close. Native <details> teardown does not hit the
      // portal insertBefore race that required a rAF delay with Dialog.
      onChange(next);
      setQuery("");
      closeDetails(document.getElementById(id));
    },
    [onChange, id],
  );

  const triggerIconGroup = selected?.group ?? "General";
  const TriggerIcon = selected ? iconForGroup(triggerIconGroup) : Container;
  const triggerAccent = selected
    ? accentForGroup(triggerIconGroup)
    : "bg-muted text-muted-foreground";
  const triggerTitle = selected ? selected.label : noneLabel;
  const triggerBody = selected ? selected.description || selected.group : noneDescription;

  <div className="flex flex-col gap-2">
    <Label htmlFor={id}>{label as string}</Label>
    <details
      className="group/picker"
      onToggle={(ev) => {
        const el = ev.currentTarget as HTMLDetailsElement;
        setOpen(el.open);
        if (!el.open) setQuery("");
      }}
    >
      <summary
        id={id}
        aria-label={label}
        data-selected={value}
        className="flex w-full cursor-pointer list-none items-start gap-3 rounded-md border border-border bg-card px-3 py-3 text-left outline-none transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
      >
        <span
          className={cn(
            "mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-md",
            triggerAccent as string,
          )}
        >
          <TriggerIcon aria-hidden size={20} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-semibold text-foreground">
            {triggerTitle as string}
          </span>
          <span className="mt-0.5 block text-[13px] text-muted-foreground">
            {triggerBody as string}
          </span>
        </span>
        <span className="shrink-0 self-center text-[13px] font-semibold text-primary">Choose…</span>
      </summary>

      @if (open) {
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          role="presentation"
          data-testid={`${id}-overlay`}
        >
          <button
            type="button"
            aria-label="Dismiss"
            className="absolute inset-0 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]"
            onClick={(ev) => closeDetails(ev.currentTarget as HTMLElement)}
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby={`${id}-dialog-title`}
            className="relative z-10 flex max-h-[min(90vh,44rem)] w-full max-w-3xl flex-col overflow-hidden rounded-md border border-border bg-popover text-[14px] text-popover-foreground shadow-lg outline-none"
          >
            <div className="border-b border-border px-5 py-4 pr-12">
              <h2
                id={`${id}-dialog-title`}
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground"
              >
                Choose {label as string}
              </h2>
              <p className="mt-2 text-[14px] text-muted-foreground">
                {
                  (browseGroups && browseGroups.length > 0
                    ? "Browse popular picks or search the full catalog. Selecting a card applies it immediately."
                    : "Browse by category or search. Selecting a card applies it immediately.") as string
                }
              </p>
              <div className="relative mt-3">
                <Search
                  aria-hidden
                  size={16}
                  className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground"
                />
                <Input
                  type="search"
                  value={query}
                  placeholder={searchPlaceholder as string}
                  aria-label={`Search ${label}`}
                  className="pl-9"
                  onInput={(ev) => setQuery((ev.currentTarget as HTMLInputElement).value)}
                />
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="absolute top-2 right-2"
                aria-label="Close"
                onClick={(ev) => closeDetails(ev.currentTarget as HTMLElement)}
              >
                <X aria-hidden size={16} />
              </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
              <div className="mb-4">
                <TemplateCard
                  option={null}
                  noneLabel={noneLabel}
                  noneDescription={noneDescription}
                  active={value === "none"}
                  onSelect={choose}
                />
              </div>

              @if (filtered.length === 0) {
                <p className="py-8 text-center text-[14px] text-muted-foreground">
                  No templates match your search.
                </p>;
              } @else {
                <div className="flex flex-col gap-6">
                  @for (const section of grouped; key section.group) {
                    <section aria-labelledby={`${id}-group-${section.group}`}>
                      <h3
                        id={`${id}-group-${section.group}`}
                        className="mb-2 text-[12px] font-semibold tracking-wide text-muted-foreground uppercase"
                      >
                        {section.group as string}
                      </h3>
                      <div className="grid gap-2 sm:grid-cols-2">
                        @for (const opt of section.options; key opt.id) {
                          <TemplateCard
                            option={opt}
                            noneLabel={noneLabel}
                            noneDescription={noneDescription}
                            active={value === opt.id}
                            onSelect={choose}
                          />;
                        }
                      </div>
                    </section>;
                  }
                </div>;
              }
            </div>

            <div className="flex justify-end border-t border-border bg-muted/30 px-5 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={(ev) => closeDetails(ev.currentTarget as HTMLElement)}
              >
                Close
              </Button>
            </div>
          </div>
        </div>;
      }
    </details>
    <p className="text-[13px] text-muted-foreground">{description as string}</p>
  </div>;
}
