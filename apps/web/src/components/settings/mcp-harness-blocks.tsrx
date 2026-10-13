import { useState } from "octane";
import { Toggle } from "@octanejs/base-ui/toggle";
import { ToggleGroup } from "@octanejs/base-ui/toggle-group";
import { Check, ChevronDown } from "@octanejs/lucide";
import { CodeSnippet } from "@/components/code-snippet";
import { CopyCommand } from "@/components/settings/copy-command";
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
import { mcpHarnessSnippets, type McpAuthVariant } from "@/lib/mcp-harness-snippets";

/**
 * Per-harness MCP config block — pick a client and an auth mode (static token
 * header or client-driven OAuth), get the highlighted file content to paste
 * (JSON, or TOML for Codex). The bearer stays a `<pat>` placeholder.
 * `initialHtml` is the SSR-highlighted markup for the default view (first
 * harness + token mode) so first paint ships styled, not plain; the markup
 * is dual-theme `light-dark()`, correct under either color scheme.
 */
export function McpHarnessBlocks(props: {
  endpoint: string;
  initialHtml?: string | null;
}) @{
  const snippets = mcpHarnessSnippets(props.endpoint);
  const [id, setId] = useState(snippets[0]?.id ?? "");
  const [mode, setMode] = useState<McpAuthVariant>("token");
  const [copied, setCopied] = useState(false);
  const current = snippets.find((s) => s.id === id) ?? snippets[0];
  const block = mode === "oauth" ? current.oauth : current.token;
  // The SSR markup matches exactly one block — the first harness in token
  // mode; everything else highlights client-side once the engine is warm.
  const isInitialBlock = current.id === snippets[0]?.id && mode === "token";

  async function copy() {
    try {
      await navigator.clipboard.writeText(block.code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via select-all */
    }
  }

  const segItem =
    "inline-flex h-9 items-center justify-center rounded-sm px-3 text-[13px] font-medium text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring data-[pressed]:bg-accent data-[pressed]:text-foreground data-[pressed]:shadow-sm";

  <div className="flex flex-col gap-3">
    <div className="flex flex-wrap items-center gap-3">
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

      <ToggleGroup
        value={[mode]}
        onValueChange={(values) => {
          // Single-select: Base UI can emit an empty array on deselect — keep
          // the current mode instead of clearing it.
          const next = values[values.length - 1];
          if (next === "token" || next === "oauth") {
            setMode(next);
            setCopied(false);
          }
        }}
        className="inline-flex h-11 items-center gap-1 rounded-md border border-input bg-card p-1"
        aria-label="Auth mode"
      >
        <Toggle
          value="token"
          data-testid="mcp-auth-token"
          className={segItem}
          aria-label="Token auth"
        >
          Token
        </Toggle>
        <Toggle
          value="oauth"
          data-testid="mcp-auth-oauth"
          className={segItem}
          aria-label="OAuth auth"
        >
          OAuth
        </Toggle>
      </ToggleGroup>
    </div>

    <div
      className="overflow-hidden rounded-md border border-border bg-card"
      data-testid="mcp-snippet-block"
      data-harness={current.id}
      data-auth={mode}
    >
      <div className="flex items-center justify-between gap-3 border-b border-border bg-muted/40 px-3 py-1.5">
        <p
          className="truncate font-mono text-[13px] text-muted-foreground"
          data-testid="mcp-snippet-file"
        >
          {current.file as string}
        </p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="-mr-2 shrink-0"
          aria-label={`Copy ${current.label} config`}
          onClick={() => void copy()}
        >
          {(copied ? "Copied" : "Copy") as string}
        </Button>
      </div>
      <div className="p-3">
        <CodeSnippet
          code={block.code}
          path={current.file}
          highlightedHtml={isInitialBlock ? (props.initialHtml ?? null) : null}
        />
      </div>
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
