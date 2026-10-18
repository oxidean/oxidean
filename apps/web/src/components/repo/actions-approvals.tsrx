import type { ActionJobPublic, ActionRunPublic } from "@oxidean/api-client";
import { useMutation, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Check, ShieldCheck, Timer, X } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Human label for a job/run `waiting_reason`. */
export function waitingReasonLabel(reason?: string): string {
  switch (reason) {
    case "fork_approval":
      return "Waiting for maintainer approval";
    case "environment_review":
      return "Waiting for deployment review";
    case "environment_wait_timer":
      return "Waiting for environment wait timer";
    case "needs":
      return "Waiting for dependent jobs";
    default:
      return "";
  }
}

/** Banner + Approve button for a run held on first-time-contributor
 * approval (`run.waiting_reason === "fork_approval"`). */
export function ActionsForkApprovalBanner(props: {
  owner: string;
  name: string;
  run: ActionRunPublic;
}) @{
  const qc = useQueryClient();
  const [err, setErr] = useState("");
  const approveM = useMutation({
    mutationFn: async () => {
      const res = await apiClient.repo.actions.approveRun({
        owner: props.owner,
        name: props.name,
        run_id: props.run.id,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    onSuccess: async () => {
      setErr("");
      await qc.invalidateQueries({ queryKey: ["repo", "actions"] });
    },
    onError: (e) => setErr(e instanceof Error ? e.message : NETWORK_ERROR),
  });

  @if (props.run.waiting_reason === "fork_approval") {
    <div
      className="mt-4 flex flex-wrap items-center gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2.5"
      role="status"
      data-testid="actions-fork-approval-banner"
    >
      <ShieldCheck aria-hidden size={16} className="shrink-0 text-amber-600" />
      <p className="min-w-0 flex-1 text-[13px] text-foreground">
        This pull request is from a first-time contributor and is waiting for maintainer approval
        before workflows run.
      </p>
      @if (props.run.can_approve) {
        <Button
          type="button"
          size="sm"
          disabled={approveM.isPending}
          data-testid="actions-approve-run"
          onClick={() => approveM.mutate()}
        >
          {(approveM.isPending ? "Approving…" : "Approve and run") as string}
        </Button>;
      }
      @if (err) {
        <p className="w-full text-[13px] text-destructive" role="alert">
          {err as string}
        </p>;
      }
    </div>;
  }
}

/** Per-job review controls for an environment-held deployment — approve /
 * reject with an optional comment, plus the recorded review history. Renders
 * nothing for jobs without an environment gate. */
export function ActionsJobReview(props: {
  owner: string;
  name: string;
  job: ActionJobPublic;
}) @{
  const qc = useQueryClient();
  const [comment, setComment] = useState("");
  const [err, setErr] = useState("");
  const reviewM = useMutation({
    mutationFn: async (state: "approved" | "rejected") => {
      const res = await apiClient.repo.actions.reviewDeployment({
        owner: props.owner,
        name: props.name,
        job_id: props.job.id,
        state,
        comment: comment.trim() || undefined,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    onSuccess: async () => {
      setErr("");
      setComment("");
      await qc.invalidateQueries({ queryKey: ["repo", "actions"] });
    },
    onError: (e) => setErr(e instanceof Error ? e.message : NETWORK_ERROR),
  });

  const held = props.job.waiting_reason === "environment_review";
  const reviews = props.job.reviews ?? [];

  @if (held || reviews.length > 0) {
    <div
      className="mt-3 flex flex-col gap-2 rounded-md border border-border bg-muted/30 px-3 py-2.5"
      data-testid="actions-job-review"
    >
      @if (held) {
        <div className="flex items-center gap-2 text-[13px] font-medium text-foreground">
          <ShieldCheck aria-hidden size={15} className="text-amber-600" />
          <span>
            {
              ("Deployment to " +
                (props.job.environment ?? "") +
                " is waiting for review") as string
            }
          </span>
        </div>;
      }
      @if (held && props.job.can_review) {
        <div className="flex flex-col gap-2">
          <Input
            value={comment}
            onInput={(e) => setComment((e.target as HTMLInputElement).value)}
            placeholder="Review comment (optional)"
            data-testid="actions-review-comment"
            disabled={reviewM.isPending}
          />
          <div className="flex gap-2">
            <Button
              type="button"
              size="sm"
              disabled={reviewM.isPending}
              data-testid="actions-review-approve"
              onClick={() => reviewM.mutate("approved")}
            >
              <Check aria-hidden size={14} />
              Approve and deploy
            </Button>
            <Button
              type="button"
              variant="destructive"
              size="sm"
              disabled={reviewM.isPending}
              data-testid="actions-review-reject"
              onClick={() => reviewM.mutate("rejected")}
            >
              <X aria-hidden size={14} />
              Reject
            </Button>
          </div>
        </div>;
      } @else {
        @if (held) {
          <p className="text-[12px] text-muted-foreground">
            You are not a required reviewer for this environment.
          </p>;
        }
      }
      @if (err) {
        <p className="text-[13px] text-destructive" role="alert">
          {err as string}
        </p>;
      }
      @for (const r of reviews; key r.reviewer + (r.created_at ?? "")) {
        <p className="text-[12px] text-muted-foreground">
          <Badge
            className="mr-1.5 align-middle"
            variant={r.state === "approved" ? "success" : "destructive"}
          >
            {r.state as string}
          </Badge>
          <span>
            {
              (r.reviewer +
                (r.created_at ? " · " + formatRelative(r.created_at) : "")) as string
            }
          </span>
          @if (r.comment) {
            <span className="italic">{" — " + r.comment}</span>;
          }
        </p>;
      }
    </div>;
  }
}

/** Small waiting indicator for a job row (wait timer / needs). */
export function ActionsJobWaitingChip(props: { job: ActionJobPublic }) @{
  const label = waitingReasonLabel(props.job.waiting_reason);
  @if (label) {
    <span
      className="inline-flex items-center gap-1 text-[11px] text-amber-600"
      data-testid={"actions-waiting-" + props.job.id}
    >
      <Timer aria-hidden size={11} />
      {label as string}
    </span>;
  }
}
