/**
 * "Re-run" dropdown for a workflow run — GitHub parity.
 *
 * Offers re-run all / re-run failed only / re-run a single job. The API maps
 * these to `ActionRunMutationRequest.job_id` / `.failed_only`.
 */
import type { ActionJobPublic } from "@oxidean/api-client";
import { ChevronDown, RotateCcw } from "@octanejs/lucide";
import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

export type RerunTarget = { job_id?: string; failed_only?: boolean };

const FAILED_STATUSES = new Set(["failure", "failed", "error", "cancelled"]);

export function ActionsRerunMenu(props: {
  jobs: ActionJobPublic[];
  /** True while a rerun request is in flight. */
  busy?: boolean;
  onRerun: (target: RerunTarget) => void;
}) @{
  const failedCount = props.jobs.filter((j) => FAILED_STATUSES.has(j.status)).length;

  <DropdownMenu>
    <DropdownMenuTrigger
      data-testid="actions-rerun-menu"
      aria-label="Re-run options"
      disabled={props.busy}
      className={cn(buttonVariants({ variant: "outline", size: "sm" }), "gap-1.5")}
    >
      <RotateCcw aria-hidden size={14} />
      Re-run jobs
      <ChevronDown aria-hidden size={14} className="text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" className="min-w-56">
      <DropdownMenuItem
        data-testid="actions-rerun-all"
        disabled={props.busy}
        onClick={() => props.onRerun({})}
      >
        Re-run all jobs
      </DropdownMenuItem>
      <DropdownMenuItem
        data-testid="actions-rerun-failed"
        disabled={props.busy || failedCount === 0}
        onClick={() => props.onRerun({ failed_only: true })}
      >
        {"Re-run failed jobs" + (failedCount > 0 ? ` (${failedCount})` : "")}
      </DropdownMenuItem>
      @if (props.jobs.length > 1) {
        <>
          <DropdownMenuSeparator />
          @for (const job of props.jobs; key job.id) {
            <DropdownMenuItem
              data-testid={`actions-rerun-job-${job.job_key || job.id}`}
              disabled={props.busy}
              onClick={() => props.onRerun({ job_id: job.id })}
            >
              {"Re-run " + '"' + job.name + '"'}
            </DropdownMenuItem>;
          }
        </>;
      }
    </DropdownMenuContent>
  </DropdownMenu>;
}
