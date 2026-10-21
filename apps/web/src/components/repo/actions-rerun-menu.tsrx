/**
 * "Re-run" dropdown for a workflow run — GitHub parity.
 *
 * Offers re-run all / re-run failed only / re-run a single job. The API maps
 * these to `ActionRunMutationRequest.job_id` / `.failed_only`; the "Enable
 * debug logging" checkbox maps to `.debug`.
 */
import type { ActionJobPublic } from "@oxidean/api-client";
import { ChevronDown, RotateCcw } from "@octanejs/lucide";
import { useState } from "octane";
import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

type RerunTarget = { job_id?: string; failed_only?: boolean; debug?: boolean };

const FAILED_STATUSES = new Set(["failure", "failed", "error", "cancelled"]);

export function ActionsRerunMenu(props: {
  jobs: ActionJobPublic[];
  /** True while a rerun request is in flight. */
  busy?: boolean;
  onRerun: (target: RerunTarget) => void;
}) @{
  const failedCount = props.jobs.filter((j) => FAILED_STATUSES.has(j.status)).length;
  const [debug, setDebug] = useState(false);

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
        onClick={() => props.onRerun({ debug })}
      >
        Re-run all jobs
      </DropdownMenuItem>
      <DropdownMenuItem
        data-testid="actions-rerun-failed"
        disabled={props.busy || failedCount === 0}
        onClick={() => props.onRerun({ failed_only: true, debug })}
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
              onClick={() => props.onRerun({ job_id: job.id, debug })}
            >
              {"Re-run " + '"' + job.name + '"'}
            </DropdownMenuItem>;
          }
        </>;
      }
      <DropdownMenuSeparator />
      <DropdownMenuCheckboxItem
        data-testid="actions-rerun-debug"
        checked={debug}
        onCheckedChange={(v) => setDebug(v)}
        disabled={props.busy}
      >
        Enable debug logging
      </DropdownMenuCheckboxItem>
    </DropdownMenuContent>
  </DropdownMenu>;
}
