import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * Regression: under `set -o pipefail`, `git tag | grep | sort | tail` exits 1
 * when no tags match (grep's empty-match status). That killed staging-cli-feed
 * on every PR ox-cli completion and would kill production promote on untagged
 * SHAs before the --merged fallback. See images.yml staging-cli-feed.
 */

const SCRIPT = join(import.meta.dir, "../ox-dist-tag.sh");
const TIMEOUT_MS = 60_000;

function sh(cwd: string, cmd: string): { stdout: string; exit: number } {
  // Non-login shell: `bash -lc` can hang on interactive profile hooks in CI/dev
  // images and is not needed for these fixtures.
  const proc = Bun.spawnSync(["bash", "-c", cmd], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, GIT_OPTIONAL_LOCKS: "0" },
  });
  return {
    stdout: new TextDecoder().decode(proc.stdout).trim(),
    exit: proc.exitCode ?? 1,
  };
}

function withRepo(fn: (dir: string) => void): void {
  const dir = mkdtempSync(join(tmpdir(), "ox-dist-tag-"));
  try {
    sh(dir, "git init -q && git config user.email t@t && git config user.name t");
    sh(dir, "echo x > f && git add f && git commit -qm base");
    fn(dir);
  } finally {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // best-effort cleanup — never fail the test on rm
    }
  }
}

describe.serial("ox-dist-tag.sh", () => {
  test(
    "untagged commit resolves empty without pipefail failure",
    () => {
      withRepo((dir) => {
        const sha = sh(dir, "git rev-parse HEAD").stdout;
        const tag = sh(dir, `set -euo pipefail; bash "${SCRIPT}" tag "${sha}"`);
        expect(tag.exit).toBe(0);
        expect(tag.stdout).toBe("");
        const url = sh(dir, `set -euo pipefail; bash "${SCRIPT}" url "${sha}"`);
        expect(url.exit).toBe(0);
        expect(url.stdout).toBe("");
      });
    },
    TIMEOUT_MS,
  );

  test(
    "prefers stable over prerelease at the same SHA",
    () => {
      withRepo((dir) => {
        const sha = sh(dir, "git rev-parse HEAD").stdout;
        sh(dir, `git tag ox-v0.1.0-beta.1 ${sha} && git tag ox-v0.1.0 ${sha}`);
        const tag = sh(dir, `bash "${SCRIPT}" tag "${sha}"`);
        expect(tag.exit).toBe(0);
        expect(tag.stdout).toBe("ox-v0.1.0");
      });
    },
    TIMEOUT_MS,
  );

  test(
    "falls back to prerelease when no stable points at the SHA",
    () => {
      withRepo((dir) => {
        const sha = sh(dir, "git rev-parse HEAD").stdout;
        sh(dir, `git tag ox-v0.2.0-beta.3 ${sha}`);
        const tag = sh(dir, `bash "${SCRIPT}" tag "${sha}"`);
        expect(tag.exit).toBe(0);
        expect(tag.stdout).toBe("ox-v0.2.0-beta.3");
      });
    },
    TIMEOUT_MS,
  );

  test(
    "--merged-fallback finds an ancestor stable tag",
    () => {
      withRepo((dir) => {
        const base = sh(dir, "git rev-parse HEAD").stdout;
        sh(dir, `git tag ox-v0.3.0 ${base}`);
        sh(dir, "echo y >> f && git add f && git commit -qm bump");
        const tip = sh(dir, "git rev-parse HEAD").stdout;
        expect(tip).not.toBe(base);
        const without = sh(dir, `bash "${SCRIPT}" tag "${tip}"`);
        expect(without.stdout).toBe("");
        const withMerged = sh(
          dir,
          `bash "${SCRIPT}" tag "${tip}" --merged-fallback`,
        );
        expect(withMerged.exit).toBe(0);
        expect(withMerged.stdout).toBe("ox-v0.3.0");
        const url = sh(
          dir,
          `GITHUB_REPOSITORY=oxidean/oxidean bash "${SCRIPT}" url "${tip}" --merged-fallback`,
        );
        expect(url.stdout).toBe(
          "https://github.com/oxidean/oxidean/releases/download/ox-v0.3.0",
        );
      });
    },
    TIMEOUT_MS,
  );
});
