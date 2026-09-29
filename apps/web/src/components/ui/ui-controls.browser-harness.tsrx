import { useState } from "octane";
import { Checkbox } from "@/components/ui/checkbox";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";

/**
 * Minimal Chromium harness for Checkbox / Radio Indicator keepMounted races.
 * Not a product surface — excluded from browser-coverage discovery.
 */
export function UiControlsBrowserHarness() @{
  const [checked, setChecked] = useState(false);
  const [mode, setMode] = useState<"a" | "b">("a");

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
  </div>;
}
