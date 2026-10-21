/**
 * Job-log parsing for the step-aware Actions log viewer.
 *
 * The bundled runner (`crates/oxidean-runner`) emits marker lines around each
 * workflow step:
 *
 *   ##[step]<title>      — step starts; following lines belong to it
 *   ##[step-done]<title> — step finished successfully
 *   ##[step-plan]<title> — declared step announced pre-gate; steps planned
 *                          but never opened render as skipped in the rail
 *   ::error::<msg>       — failure annotation (runner emits before aborting)
 *   ::warning::/::notice:: — annotation severities
 *   ##[group]<title>     — group header line (runner rewrites `::group::`)
 *   ##[endgroup]         — group close (hidden)
 *   ::debug::<msg> / ##[debug]<msg> — step-debug lines (dimmed)
 *
 * Everything else is raw streamed output (ANSI SGR escapes included). A step
 * still open at end-of-log is running/failed/cancelled depending on the job
 * status — the runner protocol does not persist per-step status rows.
 */

export type StepStatus = "running" | "success" | "failure" | "cancelled" | "skipped";

export type AnsiSegment = {
  text: string;
  /** CSS color, when an SGR foreground applies. */
  fg?: string;
  bold?: boolean;
  dim?: boolean;
};

type LogLineKind = "out" | "error" | "warning" | "notice" | "debug" | "group";

export type LogLine = {
  /** 1-based line number in the raw log. */
  n: number;
  kind: LogLineKind;
  segments: AnsiSegment[];
  /** Concatenated segment text (search target). */
  plain: string;
};

type LogStep = {
  name: string;
  status: StepStatus;
  startLine: number;
  lines: LogLine[];
};

export type ParsedJobLog = {
  /** Lines before the first `##[step]` marker ("setup" output). */
  preamble: LogLine[];
  steps: LogStep[];
  /** Lines after the last closed step (rare runner epilogue). */
  epilogue: LogLine[];
};

const STEP_OPEN = /^##\[step\](.*)$/;
const STEP_DONE = /^##\[step-done\](.*)$/;
const STEP_PLAN = /^##\[step-plan\](.*)$/;
// Post-action phases (`runs.post`) — GitHub lists them in the rail as
// "Post <action>" steps.
const STEP_POST = /^##\[post\](.*)$/;
const GROUP_OPEN = /^##\[group\](.*)$/;
// `::error::` is the raw workflow-command form; `##[error]` is the runner's
// translated form (and what GitHub's own downloaded logs carry).
const ANNOTATION = /^(?:::(error|warning|notice)::|##\[(error|warning|notice)\])(.*)$/;
// `::debug::` is the bundled runner's passthrough form; `##[debug]` is what
// a stock act_runner emits — both render as dimmed debug lines.
const DEBUG_LINE = /^(?:::debug::|##\[debug\])(.*)$/;
// Raw `::cmd::` forms that still reach the log (pre-interpreter attempts,
// act_runner passthrough) stay hidden.
const HIDDEN_COMMAND = /^::(group|endgroup|add-mask|set-output|debug|stop-commands|save-state)::/;
const OTHER_MARKER = /^##\[/;

/** 16-color ANSI palette tuned for both themes (GitHub-ish hues). */
const ANSI_COLORS: Record<number, string> = {
  30: "#6e7681",
  31: "#f85149",
  32: "#3fb950",
  33: "#d29922",
  34: "#58a6ff",
  35: "#bc8cff",
  36: "#39c5cf",
  37: "#b1bac4",
  90: "#8b949e",
  91: "#ff7b72",
  92: "#56d364",
  93: "#e3b341",
  94: "#79c0ff",
  95: "#d2a8ff",
  96: "#56d4dd",
  97: "#f0f6fc",
};

/** 256-color cube approximation for `38;5;n`. */
function ansi256(n: number): string | undefined {
  if (n < 0 || n > 255) return undefined;
  if (n < 8) return ANSI_COLORS[30 + n];
  if (n < 16) return ANSI_COLORS[90 + (n - 8)];
  if (n >= 232) {
    const g = 8 + (n - 232) * 10;
    return `rgb(${g},${g},${g})`;
  }
  const idx = n - 16;
  const r = Math.floor(idx / 36);
  const g = Math.floor((idx % 36) / 6);
  const b = idx % 6;
  const ch = (v: number) => (v === 0 ? 0 : 55 + v * 40);
  return `rgb(${ch(r)},${ch(g)},${ch(b)})`;
}

type Style = { fg?: string; bold: boolean; dim: boolean };

/**
 * Split a raw log line into styled segments. Handles SGR color/bold/dim,
 * `38;5;n`/`38;2;r;g;b` extended colors, carriage-return overwrite (last
 * segment wins), and strips non-SGR CSI/OSC sequences.
 */
export function ansiToSegments(raw: string): AnsiSegment[] {
  // Carriage return: terminal semantics overwrite the line — keep the tail.
  const line = raw.includes("\r") ? raw.slice(raw.lastIndexOf("\r") + 1) : raw;
  const out: AnsiSegment[] = [];
  const style: Style = { bold: false, dim: false };
  let i = 0;
  let buf = "";
  const flush = () => {
    if (!buf) return;
    const seg: AnsiSegment = { text: buf };
    if (style.fg) seg.fg = style.fg;
    if (style.bold) seg.bold = true;
    if (style.dim) seg.dim = true;
    out.push(seg);
    buf = "";
  };
  while (i < line.length) {
    const ch = line[i];
    if (ch === "\x1b" && line[i + 1] === "[") {
      const csiEnd = line.slice(i + 2).search(/[a-zA-Z]/);
      if (csiEnd < 0) break; // dangling escape — drop the rest
      const finalByte = line[i + 2 + csiEnd];
      const paramsRaw = line.slice(i + 2, i + 2 + csiEnd);
      i = i + 2 + csiEnd + 1;
      if (finalByte === "m") {
        // Emit buffered text under the *previous* style before mutating it.
        flush();
        const params =
          paramsRaw === "" ? [0] : paramsRaw.split(";").map((p) => parseInt(p, 10) || 0);
        for (let p = 0; p < params.length; p++) {
          const code = params[p];
          if (code === 0) {
            style.fg = undefined;
            style.bold = false;
            style.dim = false;
          } else if (code === 1) {
            style.bold = true;
          } else if (code === 2) {
            style.dim = true;
          } else if (code === 22) {
            style.bold = false;
            style.dim = false;
          } else if (code === 39) {
            style.fg = undefined;
          } else if (ANSI_COLORS[code]) {
            style.fg = ANSI_COLORS[code];
          } else if (code === 38 && params[p + 1] === 5 && params[p + 2] !== undefined) {
            style.fg = ansi256(params[p + 2]);
            p += 2;
          } else if (
            code === 38 &&
            params[p + 1] === 2 &&
            params[p + 2] !== undefined &&
            params[p + 3] !== undefined &&
            params[p + 4] !== undefined
          ) {
            style.fg = `rgb(${params[p + 2]},${params[p + 3]},${params[p + 4]})`;
            p += 4;
          }
        }
      }
      // Non-SGR CSI (erase/cursor/…) — drop.
      continue;
    }
    if (ch === "\x1b" && line[i + 1] === "]") {
      // OSC — terminated by BEL or ST.
      const bel = line.indexOf("\x07", i);
      const st = line.indexOf("\x1b\\", i);
      const end = bel >= 0 && (st < 0 || bel < st) ? bel + 1 : st >= 0 ? st + 2 : line.length;
      i = end;
      continue;
    }
    if (ch === "\x07" || ch === "\x08") {
      i += 1;
      continue;
    }
    buf += ch;
    i += 1;
  }
  flush();
  return out;
}

const ACTIVE_JOB_STATUSES = new Set(["queued", "in_progress", "pending", "running"]);

function makeLine(n: number, kind: LogLineKind, text: string): LogLine {
  const segments = ansiToSegments(text);
  return { n, kind, segments, plain: segments.map((s) => s.text).join("") };
}

/**
 * Parse a job log blob into the preamble + step sections. `jobStatus` decides
 * how a still-open step at EOF is labelled (`failure` runs fail the open step;
 * active runs leave it `running`).
 */
export function parseJobLog(log: string, jobStatus: string): ParsedJobLog {
  const preamble: LogLine[] = [];
  const steps: LogStep[] = [];
  const epilogue: LogLine[] = [];
  // `##[step-plan]` markers — the runner announces every iterated step before
  // its `if`/cancel gates, so plans without a matching `##[step]` open are
  // steps GitHub renders as skipped (dimmed) in the rail.
  const planned: { name: string; n: number; step?: LogStep }[] = [];
  let current: LogStep | null = null;
  let sawDone = false;

  const lines = log.split("\n");
  for (let idx = 0; idx < lines.length; idx++) {
    const raw = lines[idx];
    const n = idx + 1;
    const open = STEP_OPEN.exec(raw);
    if (open) {
      if (current) {
        // A new step while one is open — the previous one ended (crash or
        // missing done marker); close it against the job status.
        current.status = jobStatus === "success" ? "success" : "failure";
      }
      current = {
        name: open[1].trim() || `Step ${steps.length + 1}`,
        status: "running",
        startLine: n,
        lines: [],
      };
      steps.push(current);
      // Pair with the LAST unpaired plan — a skipped step's plan marker
      // interleaves between the previous open and this one.
      const last = planned.findLastIndex((p) => !p.step);
      if (last >= 0) planned[last].step = current;
      sawDone = false;
      continue;
    }
    const post = STEP_POST.exec(raw);
    if (post) {
      if (current) {
        current.status = jobStatus === "success" ? "success" : "failure";
      }
      current = {
        name: post[1].trim() || `Post step ${steps.length + 1}`,
        status: "running",
        startLine: n,
        lines: [],
      };
      steps.push(current);
      sawDone = false;
      continue;
    }
    const done = STEP_DONE.exec(raw);
    if (done) {
      if (current) {
        current.status = "success";
        current = null;
        sawDone = true;
      }
      continue;
    }
    if (HIDDEN_COMMAND.test(raw) || OTHER_MARKER.test(raw)) {
      const plan = STEP_PLAN.exec(raw);
      if (plan) {
        planned.push({ name: plan[1].trim(), n });
        continue;
      }
      // `##[group]`/`::debug::`/`##[error]` are visible line kinds — handle
      // them first; `##[ann]` and any other `##[x]` stay hidden.
      const group = GROUP_OPEN.exec(raw);
      const debug = DEBUG_LINE.exec(raw);
      const ann = ANNOTATION.exec(raw);
      if (group || debug || ann) {
        const line = group
          ? makeLine(n, "group", group[1].trim() || "group")
          : debug
            ? makeLine(n, "debug", debug[1])
            : makeLine(n, (ann![1] ?? ann![2]) as LogLineKind, ann![3]);
        if (current) {
          current.lines.push(line);
        } else if (sawDone) {
          epilogue.push(line);
        } else {
          preamble.push(line);
        }
      }
      continue;
    }
    const ann = ANNOTATION.exec(raw);
    const line = ann
      ? makeLine(n, (ann[1] ?? ann[2]) as LogLineKind, ann[3])
      : makeLine(n, "out", raw);
    if (current) {
      current.lines.push(line);
    } else if (sawDone) {
      epilogue.push(line);
    } else {
      preamble.push(line);
    }
  }

  if (current) {
    current.status =
      jobStatus === "success"
        ? "success"
        : jobStatus === "cancelled"
          ? "cancelled"
          : ACTIVE_JOB_STATUSES.has(jobStatus)
            ? "running"
            : "failure";
  }
  if (planned.length === 0) {
    return { preamble, steps, epilogue };
  }
  // Planned-but-unopened steps — `if`-gated off or never reached after an
  // earlier failure — still list in GitHub's rail (dimmed). Merge in plan
  // order; opened steps pair positionally, leftovers become placeholders.
  const merged: LogStep[] = [];
  const seen = new Set<LogStep>();
  for (const p of planned) {
    if (p.step) {
      merged.push(p.step);
      seen.add(p.step);
    } else {
      merged.push({
        name: p.name || `Step ${merged.length + 1}`,
        status: "skipped",
        startLine: p.n,
        lines: [],
      });
    }
  }
  // Steps opened without a plan marker (older-runner logs) keep their order.
  for (const s of steps) {
    if (!seen.has(s)) merged.push(s);
  }
  return { preamble, steps: merged, epilogue };
}

/** All visible lines in order (preamble → steps → epilogue) — raw text basis
 * for "copy" and full-search fallbacks. */
export function flattenedLines(parsed: ParsedJobLog): LogLine[] {
  return [...parsed.preamble, ...parsed.steps.flatMap((s) => s.lines), ...parsed.epilogue];
}

/**
 * Split text into case-insensitive match spans for search highlighting.
 * `q` empty → a single unhighlighted span.
 */
export function splitHits(text: string, q: string): { t: string; hit: boolean }[] {
  if (!q) return [{ t: text, hit: false }];
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const out: { t: string; hit: boolean }[] = [];
  let i = 0;
  for (;;) {
    const j = lower.indexOf(needle, i);
    if (j < 0) {
      if (i < text.length) out.push({ t: text.slice(i), hit: false });
      break;
    }
    if (j > i) out.push({ t: text.slice(i, j), hit: false });
    out.push({ t: text.slice(j, j + needle.length), hit: true });
    i = j + needle.length;
  }
  return out;
}
