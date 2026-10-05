/**
 * Run-list filter bar for the repo Actions page (issue #44 parity).
 *
 * Controlled entirely by the route's URL search params: Selects apply
 * immediately, the text inputs (branch / actor / search) apply on submit —
 * mirroring GitHub's "type + Enter / dropdown pick" behaviour.
 */
import { useEffect, useState } from "octane";
import { Check, ChevronDown, Search, X } from "@octanejs/lucide";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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

export type ActionsFilterDraft = {
  status: string;
  event: string;
  branch: string;
  actor: string;
  q: string;
};

export const EMPTY_FILTERS: ActionsFilterDraft = {
  status: "",
  event: "",
  branch: "",
  actor: "",
  q: "",
};

export function filtersActive(f: ActionsFilterDraft): boolean {
  return !!(f.status || f.event || f.branch || f.actor || f.q);
}

const STATUS_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "All statuses" },
  { value: "success", label: "Success" },
  { value: "failure", label: "Failure" },
  { value: "cancelled", label: "Cancelled" },
  { value: "in_progress", label: "In progress" },
  { value: "queued", label: "Queued" },
];

const EVENT_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "All events" },
  { value: "push", label: "push" },
  { value: "pull_request", label: "pull_request" },
  { value: "workflow_dispatch", label: "workflow_dispatch" },
  { value: "schedule", label: "schedule" },
];

function optionLabel(options: { value: string; label: string }[], value: string): string {
  return options.find((o) => o.value === value)?.label ?? options[0]?.label ?? "";
}

export function ActionsFiltersBar(props: {
  filters: ActionsFilterDraft;
  onApply: (next: ActionsFilterDraft) => void;
}) @{
  // Text inputs are drafts committed on submit; Selects apply immediately.
  const [draft, setDraft] = useState<ActionsFilterDraft>(props.filters);
  useEffect(() => {
    setDraft(props.filters);
  }, [
    props.filters.status,
    props.filters.event,
    props.filters.branch,
    props.filters.actor,
    props.filters.q,
  ]);

  const active = filtersActive(props.filters);

  function applySelect(key: "status" | "event", v: string) {
    const next = { ...props.filters, [key]: v === "all" ? "" : v };
    setDraft(next);
    props.onApply(next);
  }

  function submitText() {
    props.onApply({
      ...props.filters,
      branch: draft.branch.trim(),
      actor: draft.actor.trim(),
      q: draft.q.trim(),
    });
  }

  <form
    method="post"
    action="#"
    data-testid="actions-filters"
    className="flex flex-wrap items-center gap-2"
    onSubmit={(e) => {
      e.preventDefault();
      submitText();
    }}
  >
    <SelectRoot
      value={props.filters.status || "all"}
      onValueChange={(v) => {
        if (typeof v === "string") applySelect("status", v);
      }}
    >
      <SelectTrigger
        data-testid="actions-filter-status"
        aria-label="Filter by status"
        className="h-8 w-auto min-w-[8.5rem] text-[13px]"
      >
        <SelectValue>{optionLabel(STATUS_OPTIONS, props.filters.status || "all")}</SelectValue>
        <SelectIcon>
          <ChevronDown aria-hidden size={14} className="text-muted-foreground" />
        </SelectIcon>
      </SelectTrigger>
      {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
          hatch; popup is anchor-scoped under the Select root. */}
      <SelectPortal>
        <SelectPositioner sideOffset={4}>
          <SelectPopup>
            <SelectList>
              @for (const opt of STATUS_OPTIONS; key opt.value) {
                <SelectItem
                  value={opt.value}
                  data-testid={`actions-filter-status-${opt.value}`}
                >
                  <SelectItemIndicator>
                    <Check aria-hidden size={14} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>{opt.label as string}</SelectItemText>
                </SelectItem>;
              }
            </SelectList>
          </SelectPopup>
        </SelectPositioner>
      </SelectPortal>
    </SelectRoot>

    <SelectRoot
      value={props.filters.event || "all"}
      onValueChange={(v) => {
        if (typeof v === "string") applySelect("event", v);
      }}
    >
      <SelectTrigger
        data-testid="actions-filter-event"
        aria-label="Filter by event"
        className="h-8 w-auto min-w-[8.5rem] text-[13px]"
      >
        <SelectValue>{optionLabel(EVENT_OPTIONS, props.filters.event || "all")}</SelectValue>
        <SelectIcon>
          <ChevronDown aria-hidden size={14} className="text-muted-foreground" />
        </SelectIcon>
      </SelectTrigger>
      {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
          hatch; popup is anchor-scoped under the Select root. */}
      <SelectPortal>
        <SelectPositioner sideOffset={4}>
          <SelectPopup>
            <SelectList>
              @for (const opt of EVENT_OPTIONS; key opt.value) {
                <SelectItem
                  value={opt.value}
                  data-testid={`actions-filter-event-${opt.value}`}
                >
                  <SelectItemIndicator>
                    <Check aria-hidden size={14} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>{opt.label as string}</SelectItemText>
                </SelectItem>;
              }
            </SelectList>
          </SelectPopup>
        </SelectPositioner>
      </SelectPortal>
    </SelectRoot>

    <Input
      value={draft.branch}
      onInput={(e) => setDraft({ ...draft, branch: (e.target as HTMLInputElement).value })}
      placeholder="Branch"
      aria-label="Filter by branch"
      data-testid="actions-filter-branch"
      className="h-8 w-28 text-[13px]"
    />
    <Input
      value={draft.actor}
      onInput={(e) => setDraft({ ...draft, actor: (e.target as HTMLInputElement).value })}
      placeholder="Actor"
      aria-label="Filter by actor"
      data-testid="actions-filter-actor"
      className="h-8 w-28 text-[13px]"
    />
    <div className="min-w-[10rem] flex-1 sm:max-w-56">
      <Input
        value={draft.q}
        onInput={(e) => setDraft({ ...draft, q: (e.target as HTMLInputElement).value })}
        placeholder="Search run titles"
        aria-label="Search run titles"
        data-testid="actions-filter-query"
        className="h-8 text-[13px]"
      />
    </div>
    <Button type="submit" variant="outline" size="sm" data-testid="actions-filter-apply">
      <Search aria-hidden size={14} />
      Filter
    </Button>
    @if (active) {
      <Button
        type="button"
        variant="ghost"
        size="sm"
        data-testid="actions-filter-clear"
        onClick={() => {
          setDraft(EMPTY_FILTERS);
          props.onApply(EMPTY_FILTERS);
        }}
      >
        <X aria-hidden size={14} />
        Clear
      </Button>;
    }
  </form>;
}
