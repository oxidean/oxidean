import { useState } from "octane";
import { Check, ChevronDown } from "@octanejs/lucide";
import { CodeSnippet } from "@/components/code-snippet";
import { CopyCommand } from "@/components/settings/copy-command";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
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
import { mcpHarnessSnippets, type McpAuthVariant } from "@/lib/mcp-harness-snippets";

/**
 * Per-harness MCP config block — pick a client and an auth mode (static token
 * header or client-driven OAuth), get the highlighted file content to paste
 * (JSON, or TOML for Codex). The bearer stays a `<pat>` placeholder.
 */
export function McpHarnessBlocks(props: { endpoint: string }) @{
  const snippets = mcpHarnessSnippets(props.endpoint);
  const [id, setId] = useState(snippets[0]?.id ?? "");
  const [mode, setMode] = useState<McpAuthVariant>("token");
  const [copied, setCopied] = useState(false);
  const current = snippets.find((s) => s.id === id) ?? snippets[0];
  const block = mode === "oauth" ? current.oauth : current.token;

  async function copy() {
    try {
      await navigator.clipboard.writeText(block.code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via select-all */
    }
  }

  <div className="flex flex-col gap-3">
    <div className="flex flex-wrap items-end gap-4">
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

      <RadioGroup
        value={mode}
        onValueChange={(next) => {
          if (next === "token" || next === "oauth") {
            setMode(next);
            setCopied(false);
          }
        }}
        className="flex-row gap-4 pb-1"
        aria-label="Auth mode"
      >
        <div className="flex items-center gap-2">
          <RadioGroupItem value="token" id="mcp-auth-token" data-testid="mcp-auth-token" />
          <Label htmlFor="mcp-auth-token" className="text-[13px] font-medium">
            Token
          </Label>
        </div>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="oauth" id="mcp-auth-oauth" data-testid="mcp-auth-oauth" />
          <Label htmlFor="mcp-auth-oauth" className="text-[13px] font-medium">
            OAuth
          </Label>
        </div>
      </RadioGroup>
    </div>

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
      data-auth={mode}
    >
      <CodeSnippet code={block.code} path={current.file} />
    </div>
    @if (block.command) {
      <>
        <p
          className="text-[13px] text-muted-foreground"
          data-testid="mcp-snippet-command-label"
        >
          {block.command.label as string}
        </p>
        <CopyCommand command={block.command.cmd} ariaLabel={block.command.aria} />
      </>;
    }
    @if (block.note) {
      <p className="text-[13px] text-muted-foreground" data-testid="mcp-snippet-note">
        {block.note as string}
      </p>;
    }
  </div>;
}
