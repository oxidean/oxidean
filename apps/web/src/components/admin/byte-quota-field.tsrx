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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { BYTE_UNIT_OPTIONS, convertUnitDisplay, type ByteUnit } from "@/lib/byte-units";

type ByteQuotaFieldProps = {
  id: string;
  label: string;
  value: string;
  unit: ByteUnit;
  disabled?: boolean;
  overridden?: boolean;
  onValueChange: (next: string) => void;
  onUnitChange: (next: ByteUnit) => void;
  onBlur?: () => void;
};

/**
 * The `<amount>`/`<amountUnit>` field pair as one component. A `form.Field`
 * nested inside another Field's render-prop never receives its field api under
 * octane 0.10 (the lazy children resolver invokes function children eagerly) —
 * the pair must be split across two component scopes.
 */
export function ByteQuotaFields(props: {
  // The octane useForm return type carries .Field — keep it loose; call sites
  // all share the same <amount>/<amountUnit> string value shape.
  form: {
    Field: (props: { name: string; children?: unknown }) => unknown;
  };
  amountName: string;
  unitName: string;
  id: string;
  label: string;
  disabled?: boolean;
  overridden?: boolean;
}) @{
  <props.form.Field name={props.amountName}>
    {(amountField) => (
      <ByteQuotaUnitInner
        form={props.form}
        amountField={amountField}
        unitName={props.unitName}
        id={props.id}
        label={props.label}
        disabled={props.disabled}
        overridden={props.overridden}
      />
    )}
  </props.form.Field>;
}

function ByteQuotaUnitInner(props: {
  form: {
    Field: (props: { name: string; children?: unknown }) => unknown;
  };
  amountField: {
    state: { value: string };
    handleBlur: () => void;
    handleChange: (v: string) => void;
  };
  unitName: string;
  id: string;
  label: string;
  disabled?: boolean;
  overridden?: boolean;
}) @{
  <props.form.Field name={props.unitName}>
    {(unitField) => (
      <ByteQuotaField
        id={props.id}
        label={props.label}
        value={props.amountField.state.value}
        unit={unitField.state.value}
        overridden={props.overridden}
        disabled={props.disabled}
        onBlur={props.amountField.handleBlur}
        onValueChange={(next) => props.amountField.handleChange(next)}
        onUnitChange={(next) => {
          props.amountField.handleChange(
            convertUnitDisplay(props.amountField.state.value, unitField.state.value, next),
          );
          unitField.handleChange(next);
        }}
      />
    )}
  </props.form.Field>;
}

/** Numeric amount + binary unit select for admin quota overrides. */
export function ByteQuotaField({
  id,
  label,
  value,
  unit,
  disabled = false,
  overridden = false,
  onValueChange,
  onUnitChange,
  onBlur,
}: ByteQuotaFieldProps) @{
  const unitLabel = BYTE_UNIT_OPTIONS.find((o) => o.value === unit)?.label ?? unit;
  const unitSelectId = `${id}-unit`;

  <div className="flex flex-col gap-2">
    <Label htmlFor={id}>{label as string}</Label>
    <div className="flex flex-wrap items-stretch gap-2">
      <Input
        id={id}
        className="min-w-[8rem] flex-1"
        inputMode="decimal"
        value={value}
        onBlur={onBlur}
        onInput={(e) => onValueChange((e.currentTarget as HTMLInputElement).value)}
        disabled={disabled}
        data-testid={`${id}-amount`}
      />
      <SelectRoot
        value={unit}
        onValueChange={(next) => {
          if (typeof next === "string") onUnitChange(next as ByteUnit);
        }}
      >
        <SelectTrigger
          id={unitSelectId}
          className="w-[6.5rem] shrink-0"
          aria-label={`${label} unit`}
          disabled={disabled}
          data-testid={`${id}-unit`}
        >
          <SelectValue>{unitLabel as string}</SelectValue>
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
                @for (const opt of BYTE_UNIT_OPTIONS; key opt.value) {
                  <SelectItem value={opt.value}>
                    <SelectItemIndicator>
                      <Check aria-hidden size={16} className="text-primary" />
                    </SelectItemIndicator>
                    <SelectItemText>{opt.label as string}</SelectItemText>
                  </SelectItem>;
                }
              </SelectList>
            </SelectPopup>
          </SelectPositioner>
        </SelectPortal>
      </SelectRoot>
    </div>
    @if (overridden) {
      <span className="text-[13px] text-muted-foreground">Override active</span>;
    }
  </div>;
}
