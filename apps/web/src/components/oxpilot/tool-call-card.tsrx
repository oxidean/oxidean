import type { OxpilotToolCallCard } from "@oxidean/api-client";
import { Wrench } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";

/**
 * One durable tool-call card (keyed by `call_id` in the transcript's
 * `@for`). `args_summary`/`output_summary` render in `<code>` text nodes
 * — never markup. The status word (`running`/`done`/`failed`) renders
 * verbatim as text next to the tool name, never icon-only (BR10.3).
 */
export function OxpilotToolCall(props: { call: OxpilotToolCallCard }) @{
  const call = props.call;

  <div className="rounded-md border border-border bg-card px-3 py-2">
    <div className="flex items-center gap-2">
      <Wrench size={14} aria-hidden className="shrink-0 text-muted-foreground" />
      <span className="font-mono text-[13px] font-medium text-foreground">
        {call.tool as string}
      </span>
      <Badge variant="outline" className="shrink-0 font-normal">
        {call.status as string}
      </Badge>
    </div>
    @if (call.args_summary) {
      <code className="mt-1.5 block truncate font-mono text-[12px] text-muted-foreground">
        {call.args_summary as string}
      </code>;
    }
    @if (call.output_summary) {
      <code className="mt-1 block truncate font-mono text-[12px] text-muted-foreground">
        {call.output_summary as string}
      </code>;
    }
  </div>;
}
