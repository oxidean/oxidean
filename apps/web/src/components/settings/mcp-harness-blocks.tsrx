import { useState } from "octane";
import { Check, ChevronDown } from "@octanejs/lucide";
import { CodeSnippet } from "@/components/code-snippet";
import { Button } from "@/components/ui/button";
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
import { mcpHarnessSnippets } from "@/lib/mcp-harness-snippets";

/**
 * Per-harness MCP config block — pick a client, get the highlighted file
 * content to paste (JSON, or TOML for Codex). The bearer stays a `<pat>`
 * placeholder.
 */
export function McpHarnessBlocks(props: { endpoint: string }) @{
  const snippets = mcpHarnessSnippets(props.endpoint);
  const [id, setId] = useState(snippets[0]?.id ?? "");
  const [copied, setCopied] = useState(false);
  const current = snippets.find((s) => s.id === id) ?? snippets[0];

  async function copy() {
    try {
      await navigator.clipboard.writeText(current.code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via select-all */
    }
  }

  <div className="flex flex-col gap-3">
    <SelectRoot
      value={current.id}
      onValueChange={(next) => {
        if (typeof next === "string") {
          setId(next);
          setCopied(false);
        }
      }}
    >
      <SelectTrigger
        className="w-full max-w-xs"
        aria-label="MCP client"
        data-testid="mcp-harness-select"
      >
        <SelectValue>{current.label as string}</SelectValue>
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
              @for (const s of snippets; key s.id) {
                <SelectItem value={s.id} data-testid={`mcp-harness-option-${s.id}`}>
                  <SelectItemIndicator>
                    <Check aria-hidden size={16} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>{s.label as string}</SelectItemText>
                </SelectItem>;
              }
            </SelectList>
          </SelectPopup>
        </SelectPositioner>
      </SelectPortal>
    </SelectRoot>

    <div className="flex items-center justify-between gap-3">
      <p className="font-mono text-[13px] text-muted-foreground" data-testid="mcp-snippet-file">
        {current.file as string}
      </p>
      <Button
        type="button"
        variant="outline"
        className="shrink-0"
        aria-label={`Copy ${current.label} config`}
        onClick={() => void copy()}
      >
        {(copied ? "Copied" : "Copy") as string}
      </Button>
    </div>
    <div
      className="rounded-md border border-border bg-card p-3"
      data-testid="mcp-snippet-block"
      data-harness={current.id}
    >
      <CodeSnippet code={current.code} path={current.file} />
    </div>
  </div>;
}
