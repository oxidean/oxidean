import { describe, expect, it } from "vitest";
import {
  commentBodySegments,
  suggestionAppliable,
  suggestionBlockCount,
} from "./pull-v2-suggestion";

const base = {
  id: "c1",
  pull_id: "p1",
  author_id: "u1",
  author_username: "ada",
  body: "",
  path: "src/main.rs",
  side: "RIGHT",
  line: 2,
  pending: false,
  resolved: false,
  outdated: false,
  created_at: "2026-03-04T10:00:00Z",
  updated_at: "2026-03-04T10:00:00Z",
};

describe("commentBodySegments", () => {
  it("splits markdown around a single suggestion fence", () => {
    const segs = commentBodySegments("please fix\n```suggestion\nlet x = 1;\n```\nthanks");
    expect(segs).toEqual([
      { kind: "md", text: "please fix" },
      { kind: "suggestion", text: "let x = 1;", index: 0 },
      { kind: "md", text: "thanks" },
    ]);
  });

  it("indexes multiple fences in order and accepts tildes", () => {
    const body =
      "a\n```suggestion\none\n```\nb\n~~~suggestion\ntwo\n~~~\n```rust\nnot a suggestion\n```\n";
    const segs = commentBodySegments(body);
    const sugs = segs.filter((s) => s.kind === "suggestion");
    expect(sugs).toEqual([
      { kind: "suggestion", text: "one", index: 0 },
      { kind: "suggestion", text: "two", index: 1 },
    ]);
    // The plain rust fence stays inside the trailing markdown segment.
    const md = segs.filter((s) => s.kind === "md").map((s) => (s as { text: string }).text);
    expect(md.join("\n")).toContain("```rust\nnot a suggestion\n```");
  });

  it("ignores a suggestion opener nested inside another fence", () => {
    const segs = commentBodySegments("```markdown\n```suggestion\nfake\n```\n```\n");
    expect(segs.every((s) => s.kind === "md")).toBe(true);
    expect(suggestionBlockCount("```markdown\n```suggestion\nfake\n```\n```\n")).toBe(0);
  });

  it("strips the opener's indentation from content lines", () => {
    const segs = commentBodySegments("   ```suggestion\n   foo\n    bar\n   ```");
    expect(segs).toEqual([{ kind: "suggestion", text: "foo\n bar", index: 0 }]);
  });

  it("treats EOF as the closer and keeps an empty body as a delete", () => {
    expect(commentBodySegments("```suggestion\nx\ny")).toEqual([
      { kind: "suggestion", text: "x\ny", index: 0 },
    ]);
    expect(commentBodySegments("note\n```suggestion\n```")).toEqual([
      { kind: "md", text: "note" },
      { kind: "suggestion", text: "", index: 0 },
    ]);
  });
});

describe("suggestionAppliable", () => {
  const withBody = (over: Record<string, unknown>) => ({
    ...base,
    body: "```suggestion\nnew()\n```",
    ...over,
  });

  it("accepts a submitted RIGHT-side line comment with a fence", () => {
    expect(suggestionAppliable(withBody({}))).toBe(true);
    // Range anchors apply too.
    expect(suggestionAppliable(withBody({ start_line: 1 }))).toBe(true);
  });

  it("rejects pending, outdated, unanchored, and LEFT-side comments", () => {
    expect(suggestionAppliable(withBody({ pending: true }))).toBe(false);
    expect(suggestionAppliable(withBody({ outdated: true }))).toBe(false);
    expect(suggestionAppliable(withBody({ line: null }))).toBe(false);
    expect(suggestionAppliable(withBody({ path: null }))).toBe(false);
    expect(suggestionAppliable(withBody({ side: "LEFT" }))).toBe(false);
    expect(suggestionAppliable({ ...base, body: "plain note" })).toBe(false);
  });
});
