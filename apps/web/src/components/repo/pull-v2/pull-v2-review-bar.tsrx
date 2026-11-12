import { useState } from "octane";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * Sticky "Finish your review" bar for the v2 changes tab (WI-0001 S6):
 * Comment / Approve / Request changes radio + optional summary body.
 * Approve/request-changes are disabled for the pull author (backend
 * enforces too). Kept in its own file so the RadioGroup close tag has no
 * sibling `@if`/`@else` arms — the DOM-race lint flags that pairing.
 */
export function PullV2ReviewBar(props: {
  pendingCount: number;
  isAuthor: boolean;
  busy: boolean;
  error: string | null;
  onSubmit: (state: "commented" | "approved" | "changes_requested", body: string) => void;
}) @{
  const [state, setState] = useState<"commented" | "approved" | "changes_requested">("commented");
  const [body, setBody] = useState("");

  const needsContent = state === "commented";
  const canSubmit =
    !props.busy && (props.pendingCount > 0 || body.trim().length > 0 || !needsContent);

  <div
    className="rounded-md border border-border bg-card p-3 shadow-sm"
    data-testid="pull-v2-review-bar"
  >
    <p className="text-[13px] font-medium text-foreground">
      @if (props.pendingCount > 0) {
        <span>
          {
            `${props.pendingCount} pending comment${props.pendingCount === 1 ? "" : "s"} — finish your review` as string
          }
        </span>;
      } @else {
        <span>Review this pull request</span>;
      }
    </p>
    <Textarea
      rows={2}
      className="mt-2 text-[13px]"
      placeholder="Leave a review summary (optional)"
      value={body}
      disabled={props.busy}
      data-testid="pull-v2-review-body"
      onInput={(e) => setBody((e.currentTarget as HTMLTextAreaElement).value)}
    />
    <RadioGroup
      className="mt-2 flex flex-row flex-wrap gap-x-4 gap-y-1"
      aria-label="Review decision"
      value={state}
      disabled={props.busy}
      onValueChange={(v) => {
        if (v === "commented" || v === "approved" || v === "changes_requested") setState(v);
      }}
    >
      <label className="flex items-center gap-1.5 text-[13px] text-foreground">
        <RadioGroupItem value="commented" className="size-4" data-testid="pull-v2-review-comment" />
        Comment
      </label>
      <label
        className={
          cn(
            "flex items-center gap-1.5 text-[13px]",
            props.isAuthor ? "text-muted-foreground" : "text-foreground",
          ) as string
        }
        title={props.isAuthor ? "You can't approve your own pull request" : undefined}
      >
        <RadioGroupItem
          value="approved"
          className="size-4"
          disabled={props.isAuthor}
          data-testid="pull-v2-review-approve"
        />
        Approve
      </label>
      <label
        className={
          cn(
            "flex items-center gap-1.5 text-[13px]",
            props.isAuthor ? "text-muted-foreground" : "text-foreground",
          ) as string
        }
        title={props.isAuthor ? "You can't request changes on your own pull request" : undefined}
      >
        <RadioGroupItem
          value="changes_requested"
          className="size-4"
          disabled={props.isAuthor}
          data-testid="pull-v2-review-request-changes"
        />
        Request changes
      </label>
    </RadioGroup>
    <p
      className={cn("mt-2 text-[13px] text-destructive", !props.error && "hidden") as string}
      role="alert"
      data-testid="pull-v2-review-error"
    >
      {(props.error ?? "") as string}
    </p>
    <div className="mt-3 flex justify-end">
      <Button
        type="button"
        size="sm"
        disabled={!canSubmit}
        data-testid="pull-v2-review-submit"
        onClick={() => props.onSubmit(state, body)}
      >
        Submit review
      </Button>
    </div>
  </div>;
}
