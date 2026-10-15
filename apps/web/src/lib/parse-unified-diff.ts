/** Line kinds in a unified diff patch (commit / compare file hunks). */
export type DiffLineKind = "meta" | "hunk" | "add" | "del" | "ctx" | "note";

export type DiffLine = {
  kind: DiffLineKind;
  text: string;
};

/**
 * Classify unified-diff lines for styled rendering.
 * Keeps raw text (including leading `+`/`-`/` `) so copy-paste stays faithful.
 */
export function parseUnifiedDiffLines(patch: string): DiffLine[] {
  if (!patch) return [];
  const raw = patch.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const parts = raw.split("\n");
  // Drop a single trailing empty segment from a final newline.
  if (parts.length > 0 && parts[parts.length - 1] === "") {
    parts.pop();
  }
  return parts.map((text) => ({ kind: classifyDiffLine(text), text }));
}

export function classifyDiffLine(line: string): DiffLineKind {
  if (line.startsWith("@@")) return "hunk";
  if (line === "\\ No newline at end of file") return "note";
  if (
    line.startsWith("diff --git ") ||
    line.startsWith("index ") ||
    line.startsWith("new file mode ") ||
    line.startsWith("deleted file mode ") ||
    line.startsWith("old mode ") ||
    line.startsWith("new mode ") ||
    line.startsWith("similarity index ") ||
    line.startsWith("rename from ") ||
    line.startsWith("rename to ") ||
    line.startsWith("copy from ") ||
    line.startsWith("copy to ") ||
    line.startsWith("Binary files ") ||
    line.startsWith("--- ") ||
    line.startsWith("+++ ")
  ) {
    return "meta";
  }
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  if (line.startsWith(" ") || line === "") return "ctx";
  return "meta";
}

/**
 * `+`/`-` line counts from unified-diff patch text. Hunk-header budgets
 * (`@@ -a,b +c,d @@`) decide the `---`/`+++` header-vs-content ambiguity:
 * a `--- ` line inside a hunk's remaining old-line budget is a real deletion
 * (e.g. markdown `---` hr), while a header after the budget is exhausted is
 * the next file's `--- a/path`.
 */
export function diffPatchStatCounts(patch: string): [number, number] {
  let adds = 0;
  let dels = 0;
  let oldLeft = 0;
  let newLeft = 0;
  for (const line of patch.split("\n")) {
    const hunk = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (hunk) {
      oldLeft = Number(hunk[2] ?? "1");
      newLeft = Number(hunk[4] ?? "1");
      continue;
    }
    if (line.startsWith("+") && newLeft > 0) {
      adds++;
      newLeft--;
    } else if (line.startsWith("-") && oldLeft > 0) {
      dels++;
      oldLeft--;
    } else if ((line.startsWith(" ") || line === "") && oldLeft > 0 && newLeft > 0) {
      oldLeft--;
      newLeft--;
    }
  }
  return [adds, dels];
}
