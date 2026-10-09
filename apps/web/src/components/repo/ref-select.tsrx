import { Check, ChevronDown } from "@octanejs/lucide";
import type { RepoRefEntry } from "@oxidean/api-client";
import {
  SelectGroup,
  SelectGroupLabel,
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
import { shortRefName, treeHref } from "@/lib/repo-browse";

type RefSelectProps = {
  owner: string;
  repo: string;
  refs: RepoRefEntry[];
  value: string;
  /** When set, changing ref navigates to this path under the new ref (tree or blob). */
  pathUnderRef?: string;
  mode?: "tree" | "blob" | "home";
};

type RefOption = { value: string; label: string };

function renderItems(options: RefOption[]) @{
  @for (const opt of options; key opt.value) {
    <SelectItem value={opt.value}>
      <SelectItemIndicator>
        <Check aria-hidden size={16} className="text-primary" />
      </SelectItemIndicator>
      <SelectItemText>{opt.label as string}</SelectItemText>
    </SelectItem>;
  }
}

/** Branch/tag Select — grouped sections, navigates to matching tree/blob URL (D-17). */
export function RefSelect({
  owner,
  repo,
  refs,
  value,
  pathUnderRef = "",
  mode = "tree",
}: RefSelectProps) @{
  const options = refs.map((r) => ({
    value: shortRefName(r.name),
    label: shortRefName(r.name),
    isTag: r.name.startsWith("refs/tags/"),
  }));
  const unique = options.filter((o, i, arr) => arr.findIndex((x) => x.value === o.value) === i);
  const branches = unique.filter((o) => !o.isTag);
  const tags = unique.filter((o) => o.isTag);
  const current =
    unique.find((o) => o.value === value) ?? ({ value, label: value, isTag: false } as const);

  function navigate(next: string) {
    if (mode === "home" || !pathUnderRef) {
      window.location.assign(treeHref(owner, repo, next, ""));
      return;
    }
    if (mode === "blob") {
      window.location.assign(
        `/${owner}/${repo}/blob/${encodeURIComponent(next)}/${pathUnderRef
          .split("/")
          .map(encodeURIComponent)
          .join("/")}`,
      );
      return;
    }
    window.location.assign(treeHref(owner, repo, next, pathUnderRef));
  }

  <SelectRoot
    value={value}
    onValueChange={(next) => {
      if (typeof next === "string" && next !== value) navigate(next);
    }}
  >
    <SelectTrigger className="min-w-[10rem]" aria-label="Switch branch or tag">
      <SelectValue>{current.label as string}</SelectValue>
      <SelectIcon>
        <ChevronDown aria-hidden size={16} />
      </SelectIcon>
    </SelectTrigger>
    {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
        hatch; popup is anchor-scoped under the Select root. */}
    <SelectPortal>
      {/* collisionPadding keeps the popup off the viewport edge on mobile. */}
      <SelectPositioner sideOffset={4} collisionPadding={12}>
        <SelectPopup className="max-h-72 w-[min(20rem,calc(100vw-2rem))] overflow-y-auto">
          <SelectList>
            @if (branches.length > 0) {
              <SelectGroup>
                <SelectGroupLabel>Branches</SelectGroupLabel>
                {renderItems(branches)}
              </SelectGroup>;
            }
            @if (tags.length > 0) {
              <SelectGroup>
                <SelectGroupLabel>Tags</SelectGroupLabel>
                {renderItems(tags)}
              </SelectGroup>;
            }
          </SelectList>
        </SelectPopup>
      </SelectPositioner>
    </SelectPortal>
  </SelectRoot>;
}
