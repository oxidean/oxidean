import type { OxpilotGrantView } from "@oxidean/api-client";
import { useMutation, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { ShieldAlert } from "@octanejs/lucide";
import { Button } from "@/components/ui/button";
import { invalidateOxpilotQueries, oxpilotGrantRespondMutation } from "@/lib/oxpilot-queries";

/** Resolved grant `state` → outcome line (verbatim wire vocabulary). */
function outcomeLine(state: string, targetLabel: string): string {
  switch (state) {
    case "allowed":
      return `Allowed — ${targetLabel} may proceed.`;
    case "denied":
      return `Denied — ${targetLabel} was not allowed to proceed.`;
    case "expired":
      return "Request expired — no response was recorded in time.";
    case "cancelled":
      return "Request cancelled.";
    case "revoked":
      return "Revoked — target access ended.";
    default:
      return `Request ${state}.`;
  }
}

/**
 * The consent card — `role="group"`, `aria-label="Action request"`.
 * Buttons in pinned DOM order: `[Allow once]` → `grant_respond
 * {allow:true, scope:"once"}`, `[Allow for this session]` →
 * `{scope:"session"}`, `[Deny]` → `{allow:false}` (scopeless). There is
 * no "always" option — deliberately (SEC-11). The card freezes to a
 * resolved outcome line after a response or when `grant.state` leaves
 * `pending` (`allowed`/`denied`/`expired`/`cancelled`/`revoked`); Deny
 * stays reachable while any allow respond is in flight.
 */
export function OxpilotConsentCard(props: {
  grant: OxpilotGrantView;
  targetLabel?: string;
}) @{
  const qc = useQueryClient();
  const g = props.grant;
  const target = props.targetLabel?.trim() || "the target";
  /** Locally-recorded outcome — freezes the card on our own response. */
  const [outcome, setOutcome] = useState<string | null>(null);
  const [respondError, setRespondError] = useState<string | null>(null);

  const respondM = useMutation({
    ...oxpilotGrantRespondMutation(),
    onSuccess: (res) => {
      setOutcome(outcomeLine(res.state, target));
      invalidateOxpilotQueries(qc);
    },
    onError: (e) => {
      setRespondError(e instanceof Error ? e.message : "Could not record your response.");
    },
  });

  // Wire state wins once it resolves; our own response freezes first.
  const resolved = outcome ?? (g.state !== "pending" ? outcomeLine(g.state, target) : null);
  const pending = respondM.isPending;

  <div
    role="group"
    aria-label="Action request"
    className="rounded-md border border-border bg-card px-3 py-3"
    data-testid="oxpilot-consent"
  >
    <div className="flex items-start gap-2">
      <ShieldAlert size={16} aria-hidden className="mt-0.5 shrink-0 text-muted-foreground" />
      <div className="min-w-0">
        <p className="text-[14px] font-medium text-foreground">
          {`${target} is asking to ${g.capability || "act"}` as string}
        </p>
        @if (g.args_summary) {
          <code className="mt-1 block truncate font-mono text-[12px] text-muted-foreground">
            {g.args_summary as string}
          </code>;
        }
        @if (g.expires_at) {
          <p className="mt-1 text-[12px] text-muted-foreground">
            {`Expires ${g.expires_at}` as string}
          </p>;
        }
      </div>
    </div>
    @if (resolved) {
      <p
        role="status"
        className="mt-3 border-t border-border pt-2 text-[13px] text-foreground"
      >
        {resolved as string}
      </p>;
    } @else {
      <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={pending}
          onClick={() => {
            setRespondError(null);
            respondM.mutate({ grant_id: g.id, allow: true, scope: "once" });
          }}
        >
          Allow once
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={pending}
          onClick={() => {
            setRespondError(null);
            respondM.mutate({ grant_id: g.id, allow: true, scope: "session" });
          }}
        >
          Allow for this session
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
          onClick={() => {
            setRespondError(null);
            respondM.mutate({ grant_id: g.id, allow: false });
          }}
        >
          Deny
        </Button>
      </div>;
    }
    @if (respondError) {
      <p role="alert" className="mt-2 text-[13px] text-destructive">
        {respondError as string}
      </p>;
    }
  </div>;
}
