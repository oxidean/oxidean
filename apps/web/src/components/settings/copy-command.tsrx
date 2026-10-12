import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Read-only command field + copy-to-clipboard button. */
export function CopyCommand(props: { command: string; ariaLabel: string }) @{
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(props.command);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via focus-select */
    }
  }

  <div className="flex flex-col gap-3 sm:flex-row">
    <Input
      readOnly
      value={props.command as string}
      aria-label={props.ariaLabel as string}
      className="min-w-0 flex-1 font-mono text-[13px]"
      onFocus={(e) => {
        (e.currentTarget as HTMLInputElement).select();
      }}
    />
    <Button
      type="button"
      variant="outline"
      className="shrink-0"
      aria-label={props.ariaLabel as string}
      onClick={() => void copy()}
    >
      {(copied ? "Copied" : "Copy") as string}
    </Button>
  </div>;
}
