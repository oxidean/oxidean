/**
 * Git enumeration for change detection (BR3.1). Thin synchronous wrappers
 * over `git` — injectable via the `Git` interface so tests can run against a
 * fixture repo or a canned transcript.
 */
import { execFileSync } from "node:child_process";

export interface GitCommit {
  sha: string;
  subject: string;
  body: string;
}

export interface Git {
  /** HEAD sha. */
  headSha(): string;
  /** Commits in `<lower>..HEAD`, or all of HEAD when lower is null. Newest first. */
  rangeCommits(lower: string | null): GitCommit[];
  /** Changed paths of one commit (root commits included). */
  commitFiles(sha: string): string[];
  /** Shas in `<tag>..HEAD` — the per-component bound set (FR3). */
  commitsSince(tag: string): Set<string>;
  /** Existing tags matching `<prefix>*`, resolved to {tag, version, commit}. */
  prefixedTags(prefix: string): { tag: string; version: string; commit: string }[];
}

const RS = "\x1e"; // record separator
const FS = "\x1f"; // field separator

function run(root: string, args: string[]): string {
  return execFileSync("git", args, {
    cwd: root,
    encoding: "utf-8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  });
}

export function liveGit(root: string): Git {
  return {
    headSha() {
      return run(root, ["rev-parse", "HEAD"]).trim();
    },
    rangeCommits(lower) {
      const spec = lower === null ? ["HEAD"] : [`${lower}..HEAD`];
      const out = run(root, [
        "log",
        `--format=%H${FS}%s${FS}%b${RS}`,
        ...spec,
      ]);
      const commits: GitCommit[] = [];
      for (const rec of out.split(RS)) {
        const t = rec.trim();
        if (t === "") continue;
        const nl = t.indexOf(FS);
        const nl2 = t.indexOf(FS, nl + 1);
        if (nl === -1) continue;
        commits.push({
          sha: t.slice(0, nl),
          subject: nl2 === -1 ? t.slice(nl + 1) : t.slice(nl + 1, nl2),
          body: nl2 === -1 ? "" : t.slice(nl2 + 1).replace(/\n+$/, ""),
        });
      }
      return commits;
    },
    commitFiles(sha) {
      const out = run(root, [
        "diff-tree",
        "--no-commit-id",
        "--name-only",
        "--root",
        "-r",
        sha,
      ]);
      return out.split("\n").filter((l) => l !== "");
    },
    commitsSince(tag) {
      const out = run(root, ["rev-list", `${tag}..HEAD`]);
      return new Set(out.split("\n").filter((l) => l !== ""));
    },
    prefixedTags(prefix) {
      const out = run(root, ["tag", "-l", `${prefix}*`, "--merged", "HEAD"]);
      const tags: { tag: string; version: string; commit: string }[] = [];
      for (const tag of out.split("\n")) {
        const t = tag.trim();
        if (t === "") continue;
        const commit = run(root, ["rev-list", "-n", "1", t]).trim();
        tags.push({ tag: t, version: t.slice(prefix.length), commit });
      }
      return tags;
    },
  };
}
