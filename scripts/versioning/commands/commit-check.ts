/**
 * `commit-check` — the grammar enforcement surface (C2 cli contract; U5 wires
 * it into the commit-msg hook and the CI job). Every mode funnels through the
 * shared `parseCommit` (BR2.5). Exit: 0 valid-or-warning, 1 grammar-invalid,
 * 2 usage error (BR2.6/BR2.7).
 */
import { readFileSync } from "node:fs";
import { loadRegistry, type RegistryConfig } from "../lib/registry.ts";
import { parseCommit, type CommitMessage } from "../lib/commit.ts";
import { scanAttribution } from "../lib/attribution.ts";

export const usage =
  "commit-check (--message|--file|--range|--pr-title|--scan-file) <v>\n" +
  "                                lint commit message(s) against the grammar;\n" +
  "                                --scan-file lints attribution markers only";

export interface CommitCheckDeps {
  /** Injectable range enumeration for tests — defaults to `git log`. */
  logRange?: (rootDir: string, range: string) => { sha: string; subject: string; body: string }[];
}

function gitLogRange(rootDir: string, range: string) {
  const proc = Bun.spawnSync(
    ["git", "log", "--format=%H%x00%B%x00%x1e", range],
    { cwd: rootDir, stdout: "pipe", stderr: "pipe" },
  );
  if (proc.exitCode !== 0) {
    throw new Error(`git log ${range} failed: ${proc.stderr.toString().slice(0, 300)}`);
  }
  return proc.stdout
    .toString()
    .split("\x1e")
    .map((rec) => rec.trim())
    .filter(Boolean)
    .map((rec) => {
      const [sha, ...rest] = rec.split("\x00");
      const body = rest.join("\x00");
      const nl = body.indexOf("\n");
      return {
        sha: sha.trim(),
        subject: nl === -1 ? body : body.slice(0, nl),
        body: nl === -1 ? "" : body.slice(nl + 1),
      };
    });
}

export interface CommitCheckResult {
  exitCode: number;
  /** Per-commit report lines for stdout (`commit <sha>: ok | ok (exempt) | <error>`). */
  lines: string[];
  failures: { sha?: string; error: string }[];
  warnings: string[];
}

export function runCommitCheck(
  rootDir: string,
  messages: CommitMessage[],
): CommitCheckResult {
  const loaded = loadRegistry(rootDir);
  if (!loaded.ok) {
    return {
      exitCode: 2,
      lines: [],
      failures: loaded.violations.map((v) => ({ error: `${v.file}:${v.line}: ${v.message}` })),
      warnings: [],
    };
  }
  return checkMessages(loaded.config, messages);
}

export function checkMessages(
  registry: RegistryConfig,
  messages: CommitMessage[],
): CommitCheckResult {
  const failures: { sha?: string; error: string }[] = [];
  const warnings: string[] = [];
  const lines: string[] = [];
  for (const msg of messages) {
    const parsed = parseCommit(msg, registry);
    const label = msg.sha ? `commit ${msg.sha.slice(0, 10)}` : "commit";
    const problems: string[] = [];
    if (!parsed.valid) problems.push(parsed.error!);
    // Attribution scans the whole message (subject + body + footers),
    // not just the grammar surface — trailers and hidden markers live
    // below the subject line.
    const text =
      msg.body === undefined ? msg.subject : `${msg.subject}\n${msg.body}`;
    for (const v of scanAttribution(text)) problems.push(`attribution: ${v}`);
    for (const p of problems) failures.push({ sha: msg.sha, error: p });
    lines.push(
      `${label}: ${problems.length ? problems.join("; ") : `ok${parsed.exempt ? " (exempt)" : ""}`}`,
    );
    warnings.push(...parsed.warnings.map((w) => `${label}: ${w}`));
  }
  return { exitCode: failures.length > 0 ? 1 : 0, lines, failures, warnings };
}

const USAGE = `usage: versioning commit-check (--message <m> | --file <path> | --range <a..b> | --pr-title <t> | --scan-file <path>)
  --message <m>   lint a literal commit message (subject + body)
  --file <path>   lint the message in a commit-msg file (git hook)
  --range <a..b>  lint every commit in a git range (CI)
  --pr-title <t>  lint a PR title (same grammar)
  --scan-file <path>  attribution-marker scan only (PR body; no grammar)
`;

/** `git commit -v` scissors line: "# ------------------------ >8 ----..." */
const SCISSORS_RE = /^[ \t]*[#;][ \t]+-{20,}[ \t]>8[ \t]-{20,}[ \t]*$/;

/** CLI body — parse argv, run the check, print, return exit code. */
export function commitCheckCommand(
  rootDir: string,
  argv: string[],
  deps: CommitCheckDeps = {},
): number {
  const mode = argv[0];
  const value = argv[1];
  if (!mode || argv.length !== 2 || !["--message", "--file", "--range", "--pr-title", "--scan-file"].includes(mode)) {
    process.stderr.write(USAGE);
    return 2;
  }

  if (mode === "--scan-file") {
    try {
      const violations = scanAttribution(readFileSync(value, "utf-8"));
      for (const v of violations) process.stderr.write(`commit-check: ${v}\n`);
      if (violations.length === 0) process.stdout.write("commit-check: ok\n");
      return violations.length === 0 ? 0 : 1;
    } catch (err) {
      process.stderr.write(`commit-check: ${err instanceof Error ? err.message : String(err)}\n`);
      return 2;
    }
  }

  let messages: CommitMessage[];
  try {
    switch (mode) {
      case "--message":
      case "--pr-title": {
        const nl = value.indexOf("\n");
        messages = [
          {
            subject: nl === -1 ? value : value.slice(0, nl),
            body: nl === -1 ? undefined : value.slice(nl + 1),
            source: mode === "--pr-title" ? "pr_title" : "message",
          },
        ];
        break;
      }
      case "--file": {
        const raw = readFileSync(value, "utf-8");
        // `git commit -v` appends the staged diff below the scissors
        // line — everything past it is not message text. Comment lines
        // before it stay in the scanned body: a marker hidden in one
        // must fail here exactly as it would in CI.
        const rawLines = raw.split("\n");
        const cut = rawLines.findIndex((l) => SCISSORS_RE.test(l));
        const text = cut === -1 ? raw : rawLines.slice(0, cut).join("\n");
        const nl = text.indexOf("\n");
        messages = [
          {
            subject: nl === -1 ? text : text.slice(0, nl),
            body: nl === -1 ? undefined : text.slice(nl + 1),
            source: "file",
          },
        ];
        break;
      }
      case "--range": {
        const enumerate = deps.logRange ?? gitLogRange;
        messages = enumerate(rootDir, value).map((c) => ({
          sha: c.sha,
          subject: c.subject,
          body: c.body,
          source: "range",
        }));
        break;
      }
    }
  } catch (err) {
    process.stderr.write(`commit-check: ${err instanceof Error ? err.message : String(err)}\n`);
    return 2;
  }

  const result = runCommitCheck(rootDir, messages);
  if (result.exitCode === 2) {
    for (const f of result.failures) process.stderr.write(`${f.error}\n`);
    return 2;
  }
  // Per-commit report lines on stdout (all problems joined per commit),
  // warnings on stderr (BR1.8-style split).
  for (const line of result.lines) process.stdout.write(`${line}\n`);
  for (const w of result.warnings) process.stderr.write(`${w}\n`);
  return result.exitCode;
}
