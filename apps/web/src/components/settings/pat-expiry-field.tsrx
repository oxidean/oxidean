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

export type PatExpiryFieldProps = {
  preset: PatExpiryPreset;
  customDate: string;
  disabled?: boolean;
  onPresetChange: (preset: PatExpiryPreset) => void;
  onCustomDateChange: (date: string) => void;
  /** Extra note under “No expiration” (e.g. FG GitHub parity hint). */
  noneHint?: string;
};

/** Expiration preset Select + always-mounted custom date (toggled via `hidden`). */
export function PatExpiryField({
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
