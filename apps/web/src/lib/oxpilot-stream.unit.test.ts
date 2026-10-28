import { describe, expect, it, vi, afterEach } from "vitest";
import {
  createAppendDedupe,
  parseSseBuffer,
  postOxpilotChat,
  type OxpilotFrame,
} from "@/lib/oxpilot-stream";

function sse(...frames: object[]): string {
  return frames.map((f) => `data: ${JSON.stringify(f)}\n\n`).join("");
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("parseSseBuffer", () => {
  it("parses complete data blocks and keeps the partial tail", () => {
    const input =
      sse({ type: "append", message_id: "m1", seq: 1, role: "assistant", delta: "a" }) +
      'data: {"type":"app';
    const { frames, rest } = parseSseBuffer(input);
    expect(frames).toHaveLength(1);
    expect(frames[0].type).toBe("append");
    expect(rest).toBe('data: {"type":"app');
  });

  it("skips malformed frames without throwing", () => {
    const { frames } = parseSseBuffer("data: not-json\n\n" + sse({ type: "ping" }));
    expect(frames.map((f) => f.type)).toEqual(["ping"]);
  });
});

describe("createAppendDedupe", () => {
  it("drops repeated (message_id, seq) appends, passes others", () => {
    const dedupe = createAppendDedupe();
    const a1: OxpilotFrame = {
      type: "append",
      message_id: "m",
      seq: 1,
      role: "assistant",
      delta: "x",
    };
    const a1dup: OxpilotFrame = { ...a1 };
    const a2: OxpilotFrame = {
      type: "append",
      message_id: "m",
      seq: 2,
      role: "assistant",
      delta: "y",
    };
    expect(dedupe(a1)).toBe(true);
    expect(dedupe(a1dup)).toBe(false);
    expect(dedupe(a2)).toBe(true);
    expect(dedupe({ type: "terminal", message_id: "m", result: "done" })).toBe(true);
  });
});

describe("postOxpilotChat", () => {
  it("reads frames to the terminal and resolves", async () => {
    const stream = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(
          new TextEncoder().encode(
            sse(
              { type: "append", message_id: "m", seq: 1, role: "assistant", delta: "Hi" },
              { type: "terminal", message_id: "m", result: "done" },
            ),
          ),
        );
        c.close();
      },
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(stream, { status: 200 })));
    const got: OxpilotFrame[] = [];
    await postOxpilotChat("s1", { content: "hi" }, (f) => got.push(f));
    expect(got.map((f) => f.type)).toEqual(["append", "terminal"]);
  });

  it("rejects with the error envelope on denial", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          new Response(
            JSON.stringify({ error: { code: "turn_active", message: "a turn is live" } }),
            { status: 409 },
          ),
        ),
    );
    await expect(postOxpilotChat("s1", { content: "x" }, () => {})).rejects.toThrow(/turn_active/);
  });
});
