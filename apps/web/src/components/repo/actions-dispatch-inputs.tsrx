import type { ActionDispatchInputPublic } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { Check, ChevronDown } from "@octanejs/lucide";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { actionsEnvironmentsQuery } from "@/lib/actions-queries";

export type DispatchInputValues = Record<string, unknown>;

/** Default value for one declared input, coerced to its type. */
function defaultFor(input: ActionDispatchInputPublic): unknown {
  if (input.default !== undefined && input.default !== null) return input.default;
  switch (input.type) {
    case "boolean":
      return false;
    case "number":
      return undefined;
    case "choice":
      return input.options?.[0] ?? "";
    default:
      return "";
  }
}

export function initialDispatchInputs(inputs: ActionDispatchInputPublic[]): DispatchInputValues {
  const out: DispatchInputValues = {};
  for (const i of inputs) out[i.name] = defaultFor(i);
  return out;
}

/** Typed `workflow_dispatch` inputs — string/number/boolean/choice/
 * environment controls rendered from the workflow's declarations. */
export function ActionsDispatchInputs(props: {
  owner: string;
  name: string;
  inputs: ActionDispatchInputPublic[];
  values: DispatchInputValues;
  onChange: (name: string, value: unknown) => void;
  disabled?: boolean;
}) @{
  // `environment` inputs offer the repo's known environments as options.
  const envsQ = useQuery({
    ...actionsEnvironmentsQuery(props.owner, props.name),
    enabled: props.inputs.some((i) => i.type === "environment"),
  });
  const envNames = (envsQ.data?.environments ?? []).map((e) => e.name);

  @for (const input of props.inputs; key input.name) {
    <div className="flex flex-col gap-1.5" data-testid={`dispatch-input-${input.name}`}>
      <Label htmlFor={`dispatch-input-field-${input.name}`}>
        {(input.name + (input.required ? " *" : "")) as string}
      </Label>
      @if (input.description) {
        <p className="text-[12px] text-muted-foreground">{input.description as string}</p>;
      }
      @if (input.type === "boolean") {
        <div className="flex items-center gap-2">
          <Checkbox
            id={`dispatch-input-field-${input.name}`}
            checked={props.values[input.name] === true}
            disabled={props.disabled}
            onCheckedChange={(v) => props.onChange(input.name, v === true)}
            data-testid={`dispatch-input-field-${input.name}`}
          />
          <span className="text-[13px] text-muted-foreground">{"Enabled"}</span>
        </div>;
      } @else {
        @if (input.type === "choice") {
          <SelectRoot
            value={String(
              (props.values[input.name] as string | number | boolean | undefined) ?? "",
            )}
            disabled={props.disabled}
            onValueChange={(next) => {
              if (typeof next === "string") props.onChange(input.name, next);
            }}
          >
            <SelectTrigger
              id={`dispatch-input-field-${input.name}`}
              className="w-full"
              data-testid={`dispatch-input-field-${input.name}`}
            >
              <SelectValue>
                {String(
                  (props.values[input.name] as string | number | boolean | undefined) ?? "",
                )}
              </SelectValue>
              <SelectIcon>
                <ChevronDown aria-hidden size={14} />
              </SelectIcon>
            </SelectTrigger>
            {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
escape hatch; popup is anchor-scoped under the Select root. */}
            <SelectPortal>
              <SelectPositioner sideOffset={4}>
                <SelectPopup>
                  <SelectList>
                    @for (const opt of input.options ?? []; key opt) {
                      <SelectItem value={opt}>
                        <SelectItemIndicator>
                          <Check aria-hidden size={14} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>{opt as string}</SelectItemText>
                      </SelectItem>;
                    }
                  </SelectList>
                </SelectPopup>
              </SelectPositioner>
            </SelectPortal>
          </SelectRoot>;
        } @else {
          @if (input.type === "environment" && envNames.length > 0) {
            <SelectRoot
              value={String(
                (props.values[input.name] as string | number | boolean | undefined) ?? "",
              )}
              disabled={props.disabled}
              onValueChange={(next) => {
                if (typeof next === "string") props.onChange(input.name, next);
              }}
            >
              <SelectTrigger
                id={`dispatch-input-field-${input.name}`}
                className="w-full"
                data-testid={`dispatch-input-field-${input.name}`}
              >
                <SelectValue>
                  {String(
                    (props.values[input.name] as string | number | boolean | undefined) ?? "",
                  )}
                </SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={14} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: same SelectPortal constraint as above. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      @for (const opt of envNames; key opt) {
                        <SelectItem value={opt}>
                          <SelectItemIndicator>
                            <Check aria-hidden size={14} className="text-primary" />
                          </SelectItemIndicator>
                          <SelectItemText>{opt as string}</SelectItemText>
                        </SelectItem>;
                      }
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>;
          } @else {
            <Input
              id={`dispatch-input-field-${input.name}`}
              type={input.type === "number" ? "number" : "text"}
              value={String(
                (props.values[input.name] as string | number | boolean | undefined) ?? "",
              )}
              disabled={props.disabled}
              onInput={(e) => {
                const raw = (e.target as HTMLInputElement).value;
                props.onChange(
                  input.name,
                  input.type === "number" ? (raw === "" ? undefined : Number(raw)) : raw,
                );
              }}
              data-testid={`dispatch-input-field-${input.name}`}
            />;
          }
        }
      }
    </div>;
  }
}
