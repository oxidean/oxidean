import { describe, expect, it } from "vitest";
import { applyJobLogFrame, actionsJobLogStreamUrl } from "./actions-queries";

describe("applyJobLogFrame", () => {
  it("replaces the whole buffer on an offset:0 snapshot", () => {
    const out = applyJobLogFrame(
      { content: "stale", next_offset: 5, size: 5, done: false },
      { offset: 0, content: "fresh log", size: 9, done: false },
    );
    expect(out).toEqual({ content: "fresh log", next_offset: 9, size: 9, done: false });
  });

  it("appends a chunk whose offset matches the current size", () => {
    const snap = applyJobLogFrame(undefined, {
      offset: 0,
      content: "one\n",
      size: 4,
      done: false,
    });
    const out = applyJobLogFrame(snap, { offset: 4, content: "two\n", size: 8, done: true });
    expect(out).toEqual({ content: "one\ntwo\n", next_offset: 8, size: 8, done: true });
  });

  it("drops stale or gapped frames — the next snapshot self-corrects", () => {
    const cur = { content: "abc", next_offset: 3, size: 3, done: false };
    // Older offset (re-delivery) and future offset (a gap) both ignored.
    expect(applyJobLogFrame(cur, { offset: 1, content: "x", size: 2, done: false })).toBe(cur);
    expect(applyJobLogFrame(cur, { offset: 9, content: "x", size: 10, done: false })).toBe(cur);
  });

  it("starts from an empty buffer when prev is undefined", () => {
    const out = applyJobLogFrame(undefined, { offset: 0, content: "", size: 0, done: true });
    expect(out.done).toBe(true);
    expect(out.content).toBe("");
  });
});

describe("actionsJobLogStreamUrl", () => {
  it("builds the stream route with encoded segments", () => {
    expect(actionsJobLogStreamUrl("o wner", "re/po", "j/1")).toBe(
      "/api/repos/o%20wner/re%2Fpo/actions/jobs/j%2F1/logs/stream",
    );
  });
});
