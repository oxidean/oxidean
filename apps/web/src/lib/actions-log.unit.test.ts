import { describe, expect, it } from "vitest";
import { ansiToSegments, flattenedLines, parseJobLog, splitHits } from "./actions-log";

describe("ansiToSegments", () => {
  it("returns plain text untouched", () => {
    expect(ansiToSegments("hello")).toEqual([{ text: "hello" }]);
  });

  it("maps SGR foreground colors and reset", () => {
    const segs = ansiToSegments("\x1b[31mred\x1b[0m plain");
    expect(segs).toEqual([{ text: "red", fg: "#f85149" }, { text: " plain" }]);
  });

  it("handles bold + truecolor", () => {
    const segs = ansiToSegments("\x1b[1;38;2;10;20;30mx");
    expect(segs[0].bold).toBe(true);
    expect(segs[0].fg).toBe("rgb(10,20,30)");
  });

  it("keeps only text after the last carriage return", () => {
    expect(ansiToSegments("progress 10%\rprogress 99%")).toEqual([{ text: "progress 99%" }]);
  });

  it("strips non-SGR CSI sequences", () => {
    expect(ansiToSegments("a\x1b[2Kb")).toEqual([{ text: "ab" }]);
  });
});

describe("parseJobLog", () => {
  const log = [
    "Job test — ci / 2 step(s) [host]",
    "##[step]actions/checkout@v4",
    "Cloning ada/hello",
    "checkout ok",
    "##[step-done]actions/checkout@v4",
    "##[step]marker",
    "oxidean-stack-hello",
    "##[step-done]marker",
  ].join("\n");

  it("groups lines into named steps and hides marker lines", () => {
    const parsed = parseJobLog(log, "success");
    expect(parsed.preamble.map((l) => l.plain)).toEqual(["Job test — ci / 2 step(s) [host]"]);
    expect(parsed.steps).toHaveLength(2);
    expect(parsed.steps[0].name).toBe("actions/checkout@v4");
    expect(parsed.steps[0].status).toBe("success");
    expect(parsed.steps[0].lines.map((l) => l.plain)).toEqual(["Cloning ada/hello", "checkout ok"]);
    expect(parsed.steps[1].name).toBe("marker");
    // Marker lines are hidden but real line numbers are preserved.
    expect(parsed.steps[1].lines[0].n).toBe(7);
  });

  it("marks a still-open step failed when the job failed", () => {
    const failed = ["##[step]build", "compiling", "::error::step exited 2"].join("\n");
    const parsed = parseJobLog(failed, "failure");
    expect(parsed.steps[0].status).toBe("failure");
    expect(parsed.steps[0].lines.map((l) => l.kind)).toEqual(["out", "error"]);
  });

  it("marks a still-open step running while the job is in progress", () => {
    const parsed = parseJobLog("##[step]build\ncompiling\n", "in_progress");
    expect(parsed.steps[0].status).toBe("running");
    // Trailing empty line lands inside the step as an (empty) out line.
    expect(parsed.steps[0].lines).toHaveLength(2);
  });

  it("keeps epilogue lines after the last done step", () => {
    const parsed = parseJobLog("##[step]a\nx\n##[step-done]a\ndone-msg\n", "success");
    expect(parsed.epilogue.map((l) => l.plain)).toEqual(["done-msg", ""]);
    expect(flattenedLines(parsed).map((l) => l.plain)).toEqual(["x", "done-msg", ""]);
  });

  it("treats a run with no markers as one preamble", () => {
    const parsed = parseJobLog("plain\nlog\n", "success");
    expect(parsed.steps).toHaveLength(0);
    expect(flattenedLines(parsed).map((l) => l.plain)).toEqual(["plain", "log", ""]);
  });
});

describe("splitHits", () => {
  it("splits case-insensitive matches into hit spans", () => {
    expect(splitHits("Build OK build", "build")).toEqual([
      { t: "Build", hit: true },
      { t: " OK ", hit: false },
      { t: "build", hit: true },
    ]);
    expect(splitHits("abc", "")).toEqual([{ t: "abc", hit: false }]);
    expect(splitHits("abc", "zzz")).toEqual([{ t: "abc", hit: false }]);
  });
});
