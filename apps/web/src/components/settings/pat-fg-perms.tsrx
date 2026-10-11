import { Check, ChevronDown } from "@octanejs/lucide";
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
import type { FgPermRow, FgPermValues, PermChoice } from "@/components/settings/pat-scopes";

type FgPermRowsProps = {
  rows: FgPermRow[];
  values: FgPermValues;
  pending: boolean;
  onChange: (key: string, choice: PermChoice) => void;
};

function permLabel(v: string): string {
  if (v === "write") return "Read and write";
  if (v === "read") return "Read-only";
  return "No access";
}

/** One three-way Select per FG permission family — repo or account level. */
export function FgPermRows({ rows, values, pending, onChange }: FgPermRowsProps) @{
  @for (const row of rows; key row.key) {
    <div
      className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between"
      data-testid={`fg-perm-${row.key}`}
    >
      <div className="flex flex-col gap-0.5">
        <span className="text-[14px] text-foreground">{row.label as string}</span>
        <span className="text-[13px] text-muted-foreground">{row.hint as string}</span>
      </div>
      <SelectRoot
        value={values[row.key] ?? "none"}
        onValueChange={(next) => {
          if (next === "none" || next === "read" || next === "write") {
            onChange(row.key, next);
          }
        }}
      >
        <SelectTrigger
          className="w-full max-w-[12rem]"
          aria-label={`${row.label} permission`}
          disabled={pending}
          data-testid={`fg-perm-${row.key}-select`}
        >
          <SelectValue>{permLabel(values[row.key] ?? "none") as string}</SelectValue>
          <SelectIcon>
            <ChevronDown aria-hidden size={16} />
          </SelectIcon>
        </SelectTrigger>
        {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
            hatch; popup is anchor-scoped under the Select root. */}
        <SelectPortal>
          <SelectPositioner sideOffset={4}>
            <SelectPopup>
              <SelectList>
                <SelectItem value="none" data-testid={`fg-perm-${row.key}-option-none`}>
                  <SelectItemIndicator>
                    <Check aria-hidden size={16} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>No access</SelectItemText>
                </SelectItem>
                <SelectItem value="read" data-testid={`fg-perm-${row.key}-option-read`}>
                  <SelectItemIndicator>
                    <Check aria-hidden size={16} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>Read-only</SelectItemText>
                </SelectItem>
                <SelectItem value="write" data-testid={`fg-perm-${row.key}-option-write`}>
                  <SelectItemIndicator>
                    <Check aria-hidden size={16} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>Read and write</SelectItemText>
                </SelectItem>
              </SelectList>
            </SelectPopup>
          </SelectPositioner>
        </SelectPortal>
      </SelectRoot>
    </div>;
  }
}
