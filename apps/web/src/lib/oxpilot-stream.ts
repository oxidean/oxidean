/**
 * `/api/oxpilot` SSE client — contract C2.
 *
 * `POST .../chat` is a streamed fetch (a request body rules out
 * `EventSource`); `GET .../chat/stream` is a plain EventSource. Both emit
 * the same frame vocabulary: `snapshot` → `append`* → exactly one
 * `terminal`, plus `ping` keep-alives and the `tool_call`/`card` frames
 * the card surfaces dispatch on. Exactly-once is by `(message_id, seq)`
 * dedupe on append frames — the snapshot+replay path already guarantees
 * identity, the live channel can re-deliver on reconnect.
 */
import { apiClient } from "@/lib/api-client";

/**
 * Durable tool-call card — merged onto `OxpilotMessage.tool_calls` in a
 * snapshot (sorted by `call_index`) and patched live by `tool_call`
 * frames, which carry `call_id`/`call_index` to disambiguate parallel
 * same-name calls (U3 additive fields).
 */
export type OxpilotToolCallCard = {
  call_id: string;
  call_index?: number;
  tool: string;
  status: string;
  args_summary?: string | null;
  output_summary?: string | null;
};

/**
 * `card` frames carry refs, never payloads — `ref_id` resolves through
 * the `card_kind`-keyed proc (`consent`→`grant_get`, `diff`→`diff_get`,
 * `queued`→`queue_list`).
 */
type OxpilotCardRef = {
  card_kind: string;
  ref_id: string;
};

type OxpilotMessage = {
  id: string;
  seq: number;
  role: string;
  content: string;
  status: string;
  client_msg_id?: string | null;
  tool_calls?: OxpilotToolCallCard[];
};

export type OxpilotActiveTurn = {
  turn_id: string;
  message_id: string;
  started_seq: number;
  last_delta_seq?: number;
};

export type OxpilotFrame =
  | {
      type: "snapshot";
      session_id?: string;
      messages: OxpilotMessage[];
      active_turn: OxpilotActiveTurn | null;
      pending_queue: unknown[];
      /**
       * Additive seed carriers — all optional/back-compat so the parsed
       * frame can feed the mount `Seed` shape (`streamingIds` ←
       * `streaming_ids`, `pendingQueue` ← `pending_queue`, `activeTurn`
       * ← `active_turn`, `toolCalls`, `cards`, `session`) when the wire
       * supplies them. Absent fields degrade to the derived values
       * (`streamingIds` from `status === "streaming"`, `toolCalls`
       * flattened from `messages[].tool_calls`).
       */
      streaming_ids?: string[];
      tool_calls?: OxpilotToolCallCard[];
      cards?: OxpilotCardRef[];
      session?: Record<string, unknown> | null;
    }
  | {
      type: "append";
      message_id: string;
      seq: number;
      role: string;
      delta: string;
    }
  | {
      type: "tool_call";
      call_id: string;
      call_index?: number;
      tool: string;
      status: string;
      message_id?: string;
      args_summary?: string | null;
      output_summary?: string | null;
    }
  | ({ type: "card" } & OxpilotCardRef)
  | { type: "terminal"; message_id: string; result: string; code?: string }
  | { type: "ping"; ts?: string }
  | { type: string; [k: string]: unknown };

/** Parse complete `data: {...}\n\n` blocks; returns frames + the trailing partial. */
export function parseSseBuffer(buffer: string): {
  frames: OxpilotFrame[];
  rest: string;
} {
  const frames: OxpilotFrame[] = [];
  let rest = buffer;
  for (;;) {
    const idx = rest.indexOf("\n\n");
    if (idx === -1) break;
    const block = rest.slice(0, idx);
    rest = rest.slice(idx + 2);
    for (const line of block.split("\n")) {
      if (!line.startsWith("data:")) continue;
      const payload = line.slice(5).trim();
      if (!payload) continue;
      try {
        frames.push(JSON.parse(payload) as OxpilotFrame);
      } catch {
        // Malformed frame — skip rather than kill the stream.
      }
    }
  }
  return { frames, rest };
}

/**
 * Dedupe state for append frames — keys on `message_id:seq` per C2 so a
 * mid-turn attach (snapshot replay) and the live fanout never double a
 * delta.
 */
export function createAppendDedupe() {
  const seen = new Set<string>();
  return (frame: OxpilotFrame): boolean => {
    if (frame.type !== "append") return true;
    const key = `${String(frame.message_id)}:${String(frame.seq)}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  };
}

export type StreamHandle = { close: () => void };

/**
 * POST a user message and stream the turn's frames. Resolves when the
 * `terminal` frame lands (or the body ends — defensive). Rejects with an
 * `Error` carrying the `{code, message}` envelope on non-2xx denial.
 */
export async function postOxpilotChat(
  sessionId: string,
  body: { content: string; client_msg_id?: string },
  onFrame: (frame: OxpilotFrame) => void,
): Promise<void> {
  const res = await fetch(`/api/oxpilot/sessions/${encodeURIComponent(sessionId)}/chat`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok || !res.body) {
    let detail = `${res.status}`;
    try {
      const err = (await res.json()) as {
        error?: { code?: string; message?: string };
      };
      if (err.error) detail = `${err.error.code ?? res.status}: ${err.error.message ?? ""}`;
    } catch {
      // Non-JSON denial — keep the status.
    }
    throw new Error(detail);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const { frames, rest } = parseSseBuffer(buffer);
    buffer = rest;
    for (const frame of frames) {
      onFrame(frame);
      if (frame.type === "terminal") {
        try {
          await reader.cancel();
        } catch {
          // Already closing.
        }
        return;
      }
    }
  }
}

/**
 * One-shot transcript read — opens `GET .../chat/stream`, reads the first
 * `snapshot` frame, and closes. Used to (re)seed the chat surface; the
 * long-lived attach below handles live frames.
 */
export async function fetchOxpilotSnapshot(
  sessionId: string,
): Promise<Extract<OxpilotFrame, { type: "snapshot" }>> {
  const res = await fetch(`/api/oxpilot/sessions/${encodeURIComponent(sessionId)}/chat/stream`, {
    credentials: "include",
  });
  if (!res.ok || !res.body) {
    let detail = `${res.status}`;
    try {
      const err = (await res.json()) as {
        error?: { code?: string; message?: string };
      };
      if (err.error) detail = `${err.error.code ?? res.status}: ${err.error.message ?? ""}`;
    } catch {
      // Non-JSON denial — keep the status.
    }
    throw new Error(detail);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const { frames, rest } = parseSseBuffer(buffer);
      buffer = rest;
      const snapshot = frames.find((f) => f.type === "snapshot");
      if (snapshot) return snapshot as Extract<OxpilotFrame, { type: "snapshot" }>;
    }
  } finally {
    try {
      await reader.cancel();
    } catch {
      // Already closed.
    }
  }
  throw new Error("stream closed before a snapshot frame");
}

/**
 * Attach to `GET .../chat/stream` — replayed snapshot then live fanout.
 * Returns a close handle; EventSource auto-reconnect stays enabled so a
 * transient drop re-attaches on a fresh snapshot.
 *
 * `es.onerror` splits on `es.readyState` (REL-3): `CONNECTING` is a
 * network drop with native retry already in flight — return, zero procs
 * spent. `CLOSED` is the HTTP-denial class — `es.close()` ends the dead
 * source and ONE cache-blind `sessionGet` names it for `opts.onFatal`:
 * `not_found`/`oxpilot.disabled` verbatim, any other code or a probe
 * throw ⇒ `"stream_error"`. Never a retry loop, never a dead spinner.
 */
export function openOxpilotStream(
  sessionId: string,
  onFrame: (frame: OxpilotFrame) => void,
  opts?: { onFatal?: (code: string) => void },
): StreamHandle {
  const es = new EventSource(`/api/oxpilot/sessions/${encodeURIComponent(sessionId)}/chat/stream`);
  let buffer = "";
  es.onmessage = (ev) => {
    buffer += `data:${String(ev.data)}\n\n`;
    const { frames, rest } = parseSseBuffer(buffer);
    buffer = rest;
    for (const frame of frames) onFrame(frame);
  };
  es.onerror = () => {
    if (es.readyState === EventSource.CONNECTING) return;
    if (es.readyState !== EventSource.CLOSED) return;
    es.close();
    void (async () => {
      let code = "stream_error";
      try {
        const res = await apiClient.oxpilot.sessionGet({
          session_id: sessionId,
        });
        if (!res.ok) {
          const c = res.error.code;
          code = c === "not_found" || c === "oxpilot.disabled" ? c : "stream_error";
        }
      } catch {
        // Probe unanswerable — degrade to the same generic stream-error.
      }
      opts?.onFatal?.(code);
    })();
  };
  return {
    close: () => es.close(),
  };
}
