/**
 * Step-aware job log viewer for Actions runs.
 *
 * Renders the runner's `##[step]` / `##[step-done]` markers as a step rail +
 * collapsible sections, with ANSI SGR colors, line numbers, text search, a
 * raw-log view, and a raw download (GitHub Actions parity).
 *
 * Parity note: GitHub auto-collapses finished steps; we expand them by
 * default so streamed output stays visible to the stack-browser checks.
 */
import { useMemo, useState } from "octane";
import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Download,
  FileCode2,
  Loader2,
  MinusCircle,
  XCircle,
} from "@octanejs/lucide";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  flattenedLines,
  parseJobLog,
  splitHits,
  type LogLine,
  type StepStatus,
} from "@/lib/actions-log";
import { cn } from "@/lib/utils";

function stepIcon(status: StepStatus) {
  switch (status) {
    case "success":
      return <CheckCircle2 aria-hidden size={14} className="shrink-0 text-green-600" />;
    case "failure":
      return <XCircle aria-hidden size={14} className="shrink-0 text-destructive" />;
    case "running":
      return <Loader2 aria-hidden size={14} className="shrink-0 animate-spin text-amber-500" />;
    default:
      return <MinusCircle aria-hidden size={14} className="shrink-0 text-muted-foreground" />;
  }
}

function lineKindClass(kind: LogLine["kind"]): string {
  switch (kind) {
    case "error":
      return "bg-red-500/10 text-red-600 dark:text-red-400";
    case "warning":
      return "bg-amber-500/10 text-amber-700 dark:text-amber-400";
    case "notice":
      return "text-muted-foreground";
    default:
      return "";
  }
}

/** One numbered log line — ANSI-styled segments or <mark> search hits. */
function LogLineRow(props: { line: LogLine; q?: string }) @{
  const keyedSegs = props.line.segments.map((s, i) => ({ ...s, k: i }));
  const keyedParts = props.q
    ? splitHits(props.line.plain, props.q).map((p, i) => ({ ...p, k: i }))
    : null;

  <div
    className={cn("flex px-2 leading-[1.45]", lineKindClass(props.line.kind) as string)}
    data-line-n={props.line.n}
  >
    <span
      aria-hidden="true"
      className="w-12 shrink-0 select-none pr-3 text-right text-muted-foreground/50"
    >
      {props.line.n}
    </span>
    <span className="min-w-0 flex-1 whitespace-pre-wrap break-all">
      @if (keyedParts) {
        @for (const p of keyedParts; key p.k) {
          @if (p.hit) {
            <mark className="bg-amber-300/60 text-inherit">{p.t as string}</mark>;
          } @else {
            <>{p.t as string}</>;
          }
        }
      } @else {
        @for (const s of keyedSegs; key s.k) {
          <span
            style={{
              color: s.fg,
              fontWeight: s.bold ? 600 : undefined,
              opacity: s.dim ? 0.7 : undefined,
            }}
          >
            {s.text as string}
          </span>;
        }
      }
    </span>
  </div>;
}

export function ActionsLogViewer(props: {
  log: string;
  /** Job status — decides how a still-open trailing step is labelled. */
  jobStatus: string;
  /** Filename base for the raw download. */
  downloadName?: string;
}) @{
  const [query, setQuery] = useState("");
  const [rawMode, setRawMode] = useState(false);
  const [closed, setClosed] = useState<Record<number, boolean>>({});

  const parsed = useMemo(
    () => parseJobLog(props.log, props.jobStatus),
    [props.log, props.jobStatus],
  );
  const q = query.trim();
  const searching = q.length > 0;
  const matched = searching
    ? flattenedLines(parsed).filter((l) => l.plain.toLowerCase().includes(q.toLowerCase()))
    : [];

  function toggleStep(startLine: number) {
    setClosed((prev) => ({ ...prev, [startLine]: !prev[startLine] }));
  }

  function jumpToStep(startLine: number) {
    if (closed[startLine]) toggleStep(startLine);
    document.getElementById(`actions-log-step-${startLine}`)?.scrollIntoView({ block: "start" });
  }

  function downloadLog() {
    const url = URL.createObjectURL(new Blob([props.log], { type: "text/plain;charset=utf-8" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = `${props.downloadName || "job"}-log.txt`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }

  <div data-testid="actions-log-viewer" className="min-w-0">
    <div className="mb-2 flex flex-wrap items-center gap-2">
      <div className="min-w-[12rem] flex-1 sm:max-w-xs">
        <Input
          value={query}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          placeholder="Search logs"
          aria-label="Search logs"
          data-testid="actions-log-search"
          className="h-8 text-[13px]"
        />
      </div>
      @if (searching) {
        <span className="text-[12px] text-muted-foreground">
          {`${matched.length} match${matched.length === 1 ? "" : "es"}` as string}
        </span>;
      }
      <div className="ml-auto flex items-center gap-1">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          data-testid="actions-log-raw"
          aria-pressed={rawMode}
          className={cn("h-8 gap-1 text-[12px]", rawMode ? "bg-muted" : "")}
          onClick={() => setRawMode(!rawMode)}
        >
          <FileCode2 aria-hidden size={14} />
          Raw
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          data-testid="actions-log-download"
          aria-label="Download raw log"
          className="h-8 gap-1 text-[12px]"
          onClick={downloadLog}
        >
          <Download aria-hidden size={14} />
        </Button>
      </div>
    </div>
    <div className="grid gap-0 md:grid-cols-[13rem_1fr]">
      @if (parsed.steps.length > 0 && !rawMode) {
        <nav
          aria-label="Steps"
          data-testid="actions-log-steps"
          className="max-h-[32rem] overflow-auto border-y border-l border-border bg-muted/20 py-1"
        >
          @for (const step of parsed.steps; key step.startLine) {
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] outline-none hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
              onClick={() => jumpToStep(step.startLine)}
            >
              {stepIcon(step.status)}
              <span className="min-w-0 truncate">{step.name as string}</span>
            </button>;
          }
        </nav>;
      }
      <div
        data-testid="repo-actions-job-log"
        className="max-h-[32rem] overflow-auto border border-border bg-muted/30 py-2 font-mono text-[12px] text-foreground"
      >
        @if (rawMode) {
          <pre className="whitespace-pre-wrap break-all px-2">{props.log as string}</pre>;
        } @else {
          @if (searching) {
            @for (const line of matched; key line.n) {
              <LogLineRow line={line} q={q} />;
            } @empty {
              <p className="px-3 py-2 font-sans text-[13px] text-muted-foreground">
                {"No lines match " + '"' + q + '".'}
              </p>;
            }
          } @else {
            <>
              @for (const line of parsed.preamble; key line.n) {
                <LogLineRow line={line} />;
              }
              @for (const step of parsed.steps; key step.startLine) {
                <div id={`actions-log-step-${step.startLine}` as string}>
                  <button
                    type="button"
                    data-testid={`actions-log-step-${step.startLine}`}
                    aria-expanded={!closed[step.startLine]}
                    className="flex w-full items-center gap-2 px-2 py-1 text-left font-semibold outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                    onClick={() => toggleStep(step.startLine)}
                  >
                    @if (closed[step.startLine]) {
                      <ChevronRight aria-hidden size={14} className="shrink-0" />;
                    } @else {
                      <ChevronDown aria-hidden size={14} className="shrink-0" />;
                    }
                    {stepIcon(step.status)}
                    <span className="min-w-0 truncate">{step.name as string}</span>
                  </button>
                  @if (!closed[step.startLine]) {
                    <div>
                      @for (const line of step.lines; key line.n) {
                        <LogLineRow line={line} />;
                      } @empty {
                        <p className="px-3 py-1 font-sans text-[12px] text-muted-foreground">
                          (no output)
                        </p>;
                      }
                    </div>;
                  }
                </div>;
              }
              @for (const line of parsed.epilogue; key line.n) {
                <LogLineRow line={line} />;
              }
            </>;
          }
        }
      </div>
    </div>
  </div>;
}
