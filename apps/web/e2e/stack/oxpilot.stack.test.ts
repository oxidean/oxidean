import { beforeAll, describe, expect, it } from "vitest";
import { rpc, withAdminSession } from "./client";
import { apiOrigin, requireStack } from "./env";

interface SseFrame {
  type?: string;
  seq?: number;
  messages?: unknown[];
  [k: string]: unknown;
}

async function readSse(res: Response, want: (f: SseFrame) => boolean): Promise<SseFrame[]> {
  if (!res.body) throw new Error(`SSE response has no body (status=${res.status})`);
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const frames: SseFrame[] = [];
  let buf = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let idx = buf.indexOf("\n\n");
      while (idx !== -1) {
        const raw = buf.slice(0, idx);
        buf = buf.slice(idx + 2);
        for (const line of raw.split("\n")) {
          const data = line.startsWith("data: ")
            ? line.slice(6)
            : line.startsWith("data:")
              ? line.slice(5)
              : null;
          if (data !== null && data.trim().length > 0) {
            const frame = JSON.parse(data) as SseFrame;
            frames.push(frame);
            if (want(frame)) return frames;
          }
        }
        idx = buf.indexOf("\n\n");
      }
    }
  } finally {
    reader.cancel().catch(() => {});
  }
  return frames;
}

describe("stack e2e: /oxpilot skeleton slice", () => {
  beforeAll(() => {
    requireStack();
  });

  it("session lifecycle + POST chat streams append frames then terminal", async () => {
    await withAdminSession(async (cookie) => {
      const rpcStart = performance.now();
      const status = await rpc("oxpilot.status", {}, cookie);
      const rpcMs = performance.now() - rpcStart;
      expect(status.ok).toBe(true);
      expect((status.data as { enabled?: boolean }).enabled).toBe(true);
      expect(rpcMs).toBeLessThan(150);

      const me = await rpc("auth.me", {}, cookie);
      expect(me.ok).toBe(true);
      const owner = (me.data as { username?: string }).username as string;
      expect(owner).toBeTruthy();

      const repoName = `oxpilot-stack-${Date.now()}`;
      const repo = await rpc("repo.create", { name: repoName, visibility: "private" }, cookie);
      expect(repo.ok).toBe(true);

      const created = await rpc(
        "oxpilot.session_create",
        { repo_owner: owner, repo_name: repoName, goal: "stack smoke" },
        cookie,
      );
      expect(created.ok).toBe(true);
      const sessionId = (created.data as { id?: string }).id as string;

      const post = await fetch(`${apiOrigin()}/api/oxpilot/sessions/${sessionId}/chat`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          cookie,
          accept: "text/event-stream",
        },
        body: JSON.stringify({ content: "hello ox", client_msg_id: crypto.randomUUID() }),
      });
      expect(post.status).toBe(200);
      const firstFrameStart = performance.now();
      let firstFrameMs = -1;
      const frames = await readSse(post, (f) => {
        if (firstFrameMs < 0) firstFrameMs = performance.now() - firstFrameStart;
        return f.type === "terminal";
      });
      expect(firstFrameMs).toBeGreaterThanOrEqual(0);
      expect(firstFrameMs).toBeLessThan(250);
      const appends = frames.filter((f) => f.type === "append");
      const terminal = frames.at(-1);
      // C2 is `append*` — a fast stub turn completes inside the
      // claim→attach window and delivers the reply through a re-snapshot
      // instead of append frames. Both are contract-legal; assert the
      // reply text arrived whichever carried it.
      const appendedText = appends
        .map((f) => (typeof f.delta === "string" ? f.delta : ""))
        .join("");
      const snapshotText = frames
        .filter((f) => f.type === "snapshot")
        .flatMap((f) => (f.messages as { role?: string; content?: string }[] | undefined) ?? [])
        .filter((m) => m.role === "assistant")
        .map((m) => m.content ?? "")
        .join("");
      expect(appendedText + snapshotText).toContain("Oxpilot (stub): received");
      if (appends.length > 0) {
        expect(appends.every((f) => typeof f.seq === "number" && f.role === "assistant")).toBe(
          true,
        );
        const seqs = appends.map((f) => Number(f.seq));
        expect([...seqs].sort((a, b) => a - b)).toEqual(seqs);
      }
      expect(terminal?.type).toBe("terminal");
      expect(terminal?.result).toBe("done");

      const snapshotStart = performance.now();
      const get = await fetch(`${apiOrigin()}/api/oxpilot/sessions/${sessionId}/chat/stream`, {
        headers: { cookie, accept: "text/event-stream" },
      });
      expect(get.status).toBe(200);
      const first = await readSse(get, () => true);
      expect(performance.now() - snapshotStart).toBeLessThan(250);
      expect(first[0]?.type).toBe("snapshot");
      const messages = (first[0] as { messages?: Array<{ role?: string }> }).messages ?? [];
      expect(messages.some((m) => m.role === "user")).toBe(true);
      expect(messages.some((m) => m.role === "assistant")).toBe(true);

      const list = await rpc("oxpilot.session_list", {}, cookie);
      expect(list.ok).toBe(true);
      const sessions = (list.data as { sessions?: Array<{ id?: string }> }).sessions ?? [];
      expect(sessions.some((s) => s.id === sessionId)).toBe(true);
    });
  }, 60_000);
});
