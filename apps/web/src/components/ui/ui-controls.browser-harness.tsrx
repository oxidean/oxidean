import { Check, ChevronDown } from "@octanejs/lucide";
import { useState } from "octane";
import { Checkbox } from "@/components/ui/checkbox";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
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
import { Switch } from "@/components/ui/switch";

/**
 * Minimal Chromium harness for Checkbox / Radio / Select / Switch races.
 * Not a product surface — excluded from browser-coverage discovery.
 */
export function UiControlsBrowserHarness() @{
  const [checked, setChecked] = useState(false);
  const [mode, setMode] = useState<"a" | "b">("a");
  const [choice, setChoice] = useState<"one" | "two">("one");
  const [switched, setSwitched] = useState(false);

  <div data-testid="ui-controls-harness" className="flex flex-col gap-4 p-4">
    <label className="flex cursor-pointer items-start gap-3">
      <Checkbox
        checked={checked}
        data-testid="ui-harness-checkbox"
        aria-label="Harness checkbox"
        onCheckedChange={(next) => setChecked(next === true)}
      />
      <span>Harness checkbox</span>
    </label>

    <RadioGroup
      value={mode}
      onValueChange={(next) => {
        if (next === "a" || next === "b") setMode(next);
      }}
      className="gap-2"
    >
      <label className="flex cursor-pointer items-start gap-3">
        <RadioGroupItem value="a" className="mt-0.5" data-testid="ui-harness-radio-a" />
        <span>Option A</span>
      </label>
      <label className="flex cursor-pointer items-start gap-3">
        <RadioGroupItem value="b" className="mt-0.5" data-testid="ui-harness-radio-b" />
        <span>Option B</span>
      </label>
    </RadioGroup>

    <p
      data-testid="ui-harness-panel-a"
      className={mode === "a" ? "" : "hidden"}
      aria-hidden={mode !== "a"}
    >
      Panel A
    </p>
    <p
      data-testid="ui-harness-panel-b"
      className={mode === "b" ? "" : "hidden"}
      aria-hidden={mode !== "b"}
    >
      Panel B
    </p>

    <SelectRoot
      value={choice}
      onValueChange={(next) => {
        if (next === "one" || next === "two") setChoice(next);
      }}
    >
      <SelectTrigger
        className="w-full max-w-xs"
        aria-label="Harness select"
        data-testid="ui-harness-select"
      >
        <SelectValue>{(choice === "one" ? "Choice one" : "Choice two") as string}</SelectValue>
        <SelectIcon>
          <ChevronDown aria-hidden size={16} />
        </SelectIcon>
      </SelectTrigger>
      {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape hatch. */}
      <SelectPortal>
        <SelectPositioner sideOffset={4}>
          <SelectPopup>
            <SelectList>
              <SelectItem value="one" data-testid="ui-harness-select-one">
                <SelectItemIndicator>
                  <Check aria-hidden size={16} className="text-primary" />
                </SelectItemIndicator>
                <SelectItemText>Choice one</SelectItemText>
              </SelectItem>
              <SelectItem value="two" data-testid="ui-harness-select-two">
                <SelectItemIndicator>
                  <Check aria-hidden size={16} className="text-primary" />
                </SelectItemIndicator>
                <SelectItemText>Choice two</SelectItemText>
              </SelectItem>
            </SelectList>
          </SelectPopup>
        </SelectPositioner>
      </SelectPortal>
    </SelectRoot>

    <p data-testid="ui-harness-select-value">{choice as string}</p>

    <label className="flex cursor-pointer items-center gap-3">
      <Switch
        checked={switched}
        data-testid="ui-harness-switch"
        aria-label="Harness switch"
        onCheckedChange={(next) => setSwitched(next === true)}
      />
      <span>Harness switch</span>
    </label>
    <p data-testid="ui-harness-switch-value">{switched ? "on" : "off"}</p>
  </div>;
}
