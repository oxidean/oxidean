import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";

const sessionGetMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    oxpilot: {
      sessionGet: (...args: unknown[]) => sessionGetMock(...args),
    },
  },
}));

import {
  createAppendDedupe,
  fetchOxpilotSnapshot,
  openOxpilotStream,
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

beforeEach(() => {
  sessionGetMock.mockReset();
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

describe("parseSseBuffer — U8 frame-union pins", () => {
  it("parses type:tool_call frames into the tool_call variant verbatim", () => {
    const { frames } = parseSseBuffer(
      sse({
        type: "tool_call",
        call_id: "call-7",
        call_index: 2,
        tool: "exec_shell",
        status: "running",
        message_id: "m-9",
        args_summary: "ls -la /srv",
        output_summary: "total 0",
      }),
    );
    expect(frames).toHaveLength(1);
    const frame = frames[0];
    expect(frame.type).toBe("tool_call");
    // Every wire field survives the parse — the transcript merges on
    // call_id/call_index verbatim (REL-13 discriminant pin).
    expect(frame).toMatchObject({
      call_id: "call-7",
      call_index: 2,
      tool: "exec_shell",
      status: "running",
      message_id: "m-9",
      args_summary: "ls -la /srv",
      output_summary: "total 0",
    });
  });

  it('keeps a type:"tool" frame on the catch-all arm — never a tool_call row', () => {
    const { frames } = parseSseBuffer(
      sse({
        type: "tool",
        call_id: "ghost-1",
        tool: "exec_shell",
        status: "running",
      }),
    );
    expect(frames).toHaveLength(1);
    // `"tool"` is NOT the `"tool_call"` discriminant — the consumer's
    // `frame.type === "tool_call"` arm must never see this frame.
    expect(frames[0].type).toBe("tool");
    expect(frames.filter((f) => f.type === "tool_call")).toHaveLength(0);
  });

  it("parses type:card frames carrying card_kind + ref_id verbatim", () => {
    const { frames } = parseSseBuffer(
      sse(
        { type: "card", card_kind: "consent", ref_id: "g-1" },
        { type: "card", card_kind: "diff", ref_id: "p-9" },
      ),
    );
    expect(frames).toHaveLength(2);
    const [consent, diff] = frames as Extract<OxpilotFrame, { type: "card" }>[];
    expect(consent.card_kind).toBe("consent");
    expect(consent.ref_id).toBe("g-1");
    expect(diff.card_kind).toBe("diff");
    expect(diff.ref_id).toBe("p-9");
  });

  it("keeps an unknown card_kind a frame — no throw, no special arm", () => {
    const { frames } = parseSseBuffer(sse({ type: "card", card_kind: "wibble", ref_id: "w-1" }));
    expect(frames).toHaveLength(1);
    const frame = frames[0] as Extract<OxpilotFrame, { type: "card" }>;
    expect(frame.type).toBe("card");
    expect(frame.card_kind).toBe("wibble");
    expect(frame.ref_id).toBe("w-1");
  });

  it("round-trips a snapshot message's tool_calls merge shape", () => {
    const { frames } = parseSseBuffer(
      sse({
        type: "snapshot",
        session_id: "s-1",
        messages: [
          {
            id: "m-1",
            seq: 3,
            role: "assistant",
            content: "done",
            status: "complete",
            tool_calls: [
              {
                call_id: "c-b",
                call_index: 1,
                tool: "fs_read",
                status: "done",
                args_summary: "read /tmp/x",
                output_summary: null,
              },
              {
                call_id: "c-a",
                call_index: 0,
                tool: "exec_shell",
                status: "done",
              },
            ],
          },
        ],
        active_turn: null,
        pending_queue: [],
      }),
    );
    expect(frames).toHaveLength(1);
    const snap = frames[0] as Extract<OxpilotFrame, { type: "snapshot" }>;
    const calls = snap.messages[0]?.tool_calls ?? [];
    expect(calls).toHaveLength(2);
    // The workspace sorts by call_index — the field must survive so
    // c-a (index 0) lands before c-b (index 1).
    const sorted = [...calls].sort((a, b) => (a.call_index ?? 0) - (b.call_index ?? 0));
    expect(sorted.map((c) => c.call_id)).toEqual(["c-a", "c-b"]);
    expect(sorted[1].args_summary).toBe("read /tmp/x");
    expect(sorted[1].output_summary).toBeNull();
  });
});

describe("createAppendDedupe — non-append pass-through", () => {
  it("passes tool_call / card / terminal frames unconditionally", () => {
    const dedupe = createAppendDedupe();
    const toolCall: OxpilotFrame = {
      type: "tool_call",
      call_id: "c-1",
      tool: "exec_shell",
      status: "running",
    };
    const card: OxpilotFrame = { type: "card", card_kind: "consent", ref_id: "g-1" };
    const terminal: OxpilotFrame = { type: "terminal", message_id: "m", result: "done" };
    // Non-append frames are never keyed — repeats still pass.
    expect(dedupe(toolCall)).toBe(true);
    expect(dedupe(toolCall)).toBe(true);
    expect(dedupe(card)).toBe(true);
    expect(dedupe(card)).toBe(true);
    expect(dedupe(terminal)).toBe(true);
    // …while append keeps its (message_id, seq) keying (existing pin).
    const append: OxpilotFrame = {
      type: "append",
      message_id: "m",
      seq: 1,
      role: "assistant",
      delta: "x",
    };
    expect(dedupe(append)).toBe(true);
    expect(dedupe({ ...append })).toBe(false);
  });
});

describe("fetchOxpilotSnapshot", () => {
  function sseStream(frames: object[]): Response {
    const text = frames.map((f) => `data: ${JSON.stringify(f)}\n\n`).join("");
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(text));
        controller.close();
      },
    });
    return new Response(stream, { status: 200 });
  }

  it("returns the first snapshot frame and ignores later blocks", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((url: string) => {
        expect(url).toBe("/api/oxpilot/sessions/s-9/chat/stream");
        return Promise.resolve(
          sseStream([
            {
              type: "snapshot",
              session_id: "s-9",
              messages: [],
              active_turn: null,
              pending_queue: [],
            },
            { type: "ping", ts: "2026-01-01T00:00:00Z" },
          ]),
        );
      }),
    );
    const snap = await fetchOxpilotSnapshot("s-9");
    expect(snap.type).toBe("snapshot");
    expect(snap.session_id).toBe("s-9");
  });

  it("rejects with the error envelope on a non-2xx denial", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: { code: "not_found", message: "gone" } }), {
          status: 404,
        }),
      ),
    );
    await expect(fetchOxpilotSnapshot("s-9")).rejects.toThrow(/not_found/);
  });
});

describe("openOxpilotStream — readyState arms (REL-3)", () => {
  class FakeES {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSED = 2;
    static instances: FakeES[] = [];
    onmessage: ((ev: { data: string }) => void) | null = null;
    onerror: (() => void) | null = null;
    readyState = FakeES.OPEN;
    closed = false;
    constructor(public url: string) {
      FakeES.instances.push(this);
    }
    close() {
      this.closed = true;
      this.readyState = FakeES.CLOSED;
    }
  }

  function stubES() {
    FakeES.instances = [];
    vi.stubGlobal("EventSource", FakeES as unknown as typeof EventSource);
  }

  it("CONNECTING onerror returns early — zero sessionGet, no fatal, still open", async () => {
    stubES();
    const onFatal = vi.fn();
    const handle = openOxpilotStream("s-1", () => {}, { onFatal });
    const es = FakeES.instances[0];
    expect(es).toBeTruthy();
    es.readyState = FakeES.CONNECTING;
    es.onerror?.();
    // Give the (not-started) probe microtask a chance to fire.
    await new Promise((r) => setTimeout(r, 0));
    expect(sessionGetMock).not.toHaveBeenCalled();
    expect(onFatal).not.toHaveBeenCalled();
    expect(es.closed).toBe(false);
    handle.close();
  });

  for (const [probe, expected] of [
    [{ ok: false, error: { code: "not_found", message: "gone" } }, "not_found"],
    [{ ok: false, error: { code: "oxpilot.disabled", message: "off" } }, "oxpilot.disabled"],
    [{ ok: false, error: { code: "forbidden", message: "nope" } }, "stream_error"],
    [{ ok: true, data: { id: "s-1" } }, "stream_error"],
  ] as const) {
    it(`CLOSED onerror probes once → onFatal(${expected})`, async () => {
      stubES();
      sessionGetMock.mockResolvedValue(probe);
      const onFatal = vi.fn();
      openOxpilotStream("s-7", () => {}, { onFatal });
      const es = FakeES.instances[0];
      es.readyState = FakeES.CLOSED;
      es.onerror?.();
      // The dead source closes before the single probe fires.
      expect(es.closed).toBe(true);
      await vi.waitFor(() => expect(onFatal).toHaveBeenCalledWith(expected));
      expect(sessionGetMock).toHaveBeenCalledTimes(1);
      expect(sessionGetMock).toHaveBeenCalledWith({ session_id: "s-7" });
    });
  }

  it("CLOSED onerror with a throwing probe degrades to onFatal(stream_error)", async () => {
    stubES();
    sessionGetMock.mockRejectedValue(new Error("network down"));
    const onFatal = vi.fn();
    openOxpilotStream("s-8", () => {}, { onFatal });
    const es = FakeES.instances[0];
    es.readyState = FakeES.CLOSED;
    es.onerror?.();
    expect(es.closed).toBe(true);
    await vi.waitFor(() => expect(onFatal).toHaveBeenCalledWith("stream_error"));
    expect(sessionGetMock).toHaveBeenCalledTimes(1);
  });

  it("routes parsed frames to onFrame via the EventSource onmessage seam", () => {
    stubES();
    const got: OxpilotFrame[] = [];
    openOxpilotStream("s-2", (f) => got.push(f));
    const es = FakeES.instances[0];
    es.onmessage?.({
      data: JSON.stringify({
        type: "append",
        message_id: "m",
        seq: 1,
        role: "assistant",
        delta: "hi",
      }),
    });
    es.onmessage?.({ data: JSON.stringify({ type: "ping" }) });
    expect(got.map((f) => f.type)).toEqual(["append", "ping"]);
  });
});
