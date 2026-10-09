import { Check, ChevronDown } from "@octanejs/lucide";
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
import {
  expiryPresetLabel,
  PAT_EXPIRY_PRESETS,
  tomorrowDateValue,
  type PatExpiryPreset,
} from "@/components/settings/pat-expiry";
import { cn } from "@/lib/utils";

type PatExpiryFieldProps = {
  preset: PatExpiryPreset;
  customDate: string;
  disabled?: boolean;
  onPresetChange: (preset: PatExpiryPreset) => void;
  onCustomDateChange: (date: string) => void;
  /** Extra note under “No expiration” (e.g. FG GitHub parity hint). */
  noneHint?: string;
};

/**
 * The `expiryPreset`/`expiryDate` field pair as one component. A `form.Field`
 * nested inside another Field's render-prop never receives its field api under
 * octane 0.10 (the lazy children resolver invokes function children eagerly) —
 * the pair must be split across two component scopes.
 */
export function PatExpiryFields(props: {
  // The octane useForm return type carries .Field — keep it loose; call sites
  // all share the same expiryPreset/expiryDate value shape.
  form: {
    Field: (props: { name: string; children?: unknown }) => unknown;
  };
  disabled?: boolean;
  noneHint?: string;
}) @{
  <props.form.Field name="expiryPreset">
    {(presetField) => (
      <PatExpiryDateInner
        form={props.form}
        presetField={presetField}
        disabled={props.disabled}
        noneHint={props.noneHint}
      />
    )}
  </props.form.Field>;
}

function PatExpiryDateInner(props: {
  form: {
    Field: (props: { name: string; children?: unknown }) => unknown;
  };
  presetField: {
    state: { value: PatExpiryPreset };
    handleChange: (v: PatExpiryPreset) => void;
  };
  disabled?: boolean;
  noneHint?: string;
}) @{
  <props.form.Field name="expiryDate">
    {(dateField) => (
      <PatExpiryField
        preset={props.presetField.state.value}
        customDate={dateField.state.value as string}
        disabled={props.disabled}
        onPresetChange={(p) => props.presetField.handleChange(p)}
        onCustomDateChange={(d) => dateField.handleChange(d)}
        noneHint={props.noneHint}
      />
    )}
  </props.form.Field>;
}

/** Expiration preset Select + always-mounted custom date (toggled via `hidden`). */
function PatExpiryField({
  preset,
  customDate,
  disabled,
  onPresetChange,
  onCustomDateChange,
  noneHint,
}: PatExpiryFieldProps) @{
  const showCustom = preset === "custom";
  const showNoneWarn = preset === "none";

  <div className="flex flex-col gap-3">
    <p className="text-[14px] font-medium text-foreground">Expiration</p>
    <SelectRoot
      value={preset}
      onValueChange={(next) => {
        if (typeof next !== "string") return;
        if (PAT_EXPIRY_PRESETS.some((p) => p.value === next)) {
          onPresetChange(next as PatExpiryPreset);
        }
      }}
    >
      <SelectTrigger
        className="w-full max-w-xs"
        aria-label="Expiration"
        disabled={disabled}
        data-testid="pat-expiry-preset"
      >
        <SelectValue>{expiryPresetLabel(preset) as string}</SelectValue>
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
              @for (const opt of PAT_EXPIRY_PRESETS; key opt.value) {
                <SelectItem value={opt.value} data-testid={`pat-expiry-option-${opt.value}`}>
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

    <div
      className={cn("flex flex-col gap-2", !showCustom && "hidden")}
      data-testid="pat-expiry-custom"
      aria-hidden={!showCustom}
    >
      <Input
        type="date"
        value={customDate}
        min={tomorrowDateValue() as string}
        aria-label="Expiration date"
        disabled={disabled || !showCustom}
        className="max-w-xs"
        onInput={(e) => onCustomDateChange((e.currentTarget as HTMLInputElement).value)}
      />
    </div>

    <p
      role="status"
      className={cn("text-[14px] text-amber-700 dark:text-amber-400", !showNoneWarn && "hidden")}
      data-testid="pat-expiry-none-warn"
    >
      {
        (noneHint ??
          "This token will never expire. Tokens that never expire can be long-lived risks if leaked.") as string
      }
    </p>
  </div>;
}
