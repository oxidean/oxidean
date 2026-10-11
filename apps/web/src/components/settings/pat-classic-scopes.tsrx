import { Checkbox } from "@/components/ui/checkbox";
import {
  CLASSIC_SCOPE_GROUPS,
  isImplied,
  type ClassicScopeValues,
} from "@/components/settings/pat-scopes";
import type { ClassicPatScope } from "@oxidean/api-client";

type ClassicScopeGroupsProps = {
  values: ClassicScopeValues;
  pending: boolean;
  onToggle: (scope: ClassicPatScope, checked: boolean) => void;
};

/** Grouped classic-scope checkboxes; implied scopes render checked + disabled. */
export function ClassicScopeGroups({
  values,
  pending,
  onToggle,
}: ClassicScopeGroupsProps) @{
  <div className="flex flex-col gap-4" data-testid="pat-classic-scopes">
    <p className="text-[14px] font-medium text-foreground">Scopes</p>
    @for (const group of CLASSIC_SCOPE_GROUPS; key group.title) {
      <div className="flex flex-col gap-2">
        <p className="text-[13px] font-medium uppercase tracking-wide text-muted-foreground">
          {group.title as string}
        </p>
        @for (const row of group.scopes; key row.scope) {
          <label className="flex cursor-pointer items-start gap-3">
            <Checkbox
              checked={values[row.scope] === true || isImplied(values, row.scope)}
              disabled={pending || isImplied(values, row.scope)}
              aria-label={row.scope as string}
              data-testid={row.testid as string}
              onCheckedChange={(checked) => onToggle(row.scope, checked === true)}
            />
            <span className="flex flex-col gap-0.5">
              <span className="text-[14px] text-foreground">{row.label as string}</span>
              <span className="text-[13px] text-muted-foreground">
                {row.hint as string}
              </span>
            </span>
          </label>;
        }
      </div>;
    }
  </div>;
}
