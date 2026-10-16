import { useState } from "octane";
import { Button } from "@/components/ui/button";

type RecoveryCodesViewProps = {
  /** Plaintext codes — shown exactly once (only hashes persist server-side). */
  codes: string[];
  onDone?: () => void;
  doneLabel?: string;
};

/**
 * One-time recovery-code reveal (#130): grid + clipboard copy. Rendered right
 * after enrollment or regeneration; there is no way to re-show them later.
 */
export function RecoveryCodesView({
  codes,
  onDone,
  doneLabel = "I saved my recovery codes",
}: RecoveryCodesViewProps) @{
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(codes.join("\n"));
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  <div className="flex flex-col gap-4">
    <p className="text-[14px] leading-[1.5] text-muted-foreground">
      Save these one-time recovery codes somewhere safe — a password manager or printed copy. Each
      code works exactly once, and they are never shown again.
    </p>
    <div
      role="status"
      aria-label="Recovery codes"
      className="grid grid-cols-1 gap-2 rounded-md border border-border bg-muted/40 p-4 font-mono text-[13px] text-foreground sm:grid-cols-2"
    >
      @for (const code of codes; key code) {
        <span>{code as string}</span>;
      }
    </div>
    <div className="flex flex-wrap items-center gap-3">
      <Button type="button" variant="outline" onClick={() => void copy()}>
        {(copied ? "Copied" : "Copy codes") as string}
      </Button>
      @if (onDone) {
        <Button type="button" onClick={() => onDone()}>
          {doneLabel as string}
        </Button>;
      }
    </div>
  </div>;
}
