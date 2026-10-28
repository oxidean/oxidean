/**
 * `@tanstack/ai-octane` `useChat` bound to the Oxpilot SSE surface.
 *
 * The wire stays contract C2 (`POST /api/oxpilot/sessions/{id}/chat`,
 * `snapshot` → `append`* → `terminal` frames); the `fetcher` transport
 * adapts each frame into the AG-UI `StreamChunk` vocabulary the chat
 * client drives (`RUN_STARTED` → `TEXT_MESSAGE_*` → `RUN_FINISHED` /
 * `RUN_ERROR`). Server-side `client_msg_id` idempotency rides the
 * `sendMessage` `body` channel through to `input.data`.
 */
import { useMemo, useRef } from "octane";
import { useChat } from "@tanstack/ai-octane";
import type { ChatFetcher, ChatFetcherInput, UIMessage } from "@tanstack/ai-client";
import { parseSseBuffer } from "@/lib/oxpilot-stream";

function textOf(m: UIMessage): string {
  return m.parts
    .filter((p) => p.type === "text")
    .map((p) => p.content)
    .join("");
}

/** Persisted Oxpilot message → the `UIMessage` shape `useChat` seeds from. */
export function toUiMessage(m: { id: string; role: string; content: string }): UIMessage {
  return {
    id: m.id,
    role: m.role === "user" ? "user" : m.role === "system" ? "system" : "assistant",
    parts: [{ type: "text", content: m.content }],
  } as UIMessage;
}

/**
 * One turn: POST the newest user message, stream `append` deltas as
 * `TEXT_MESSAGE_CONTENT`, close with `RUN_FINISHED` or `RUN_ERROR` at the
 * `terminal` frame. Errors throw — `useChat` surfaces them on `.error`.
 */
async function* oxpilotFetcher(
  sessionId: string,
  input: ChatFetcherInput,
): AsyncGenerator<Record<string, unknown>> {
  const lastUser = [...input.messages].reverse().find((m) => m.role === "user");
  const content = lastUser ? textOf(lastUser).trim() : "";
  const clientMsgId =
    typeof input.data?.client_msg_id === "string" ? input.data.client_msg_id : input.runId;

  const res = await fetch(`/api/oxpilot/sessions/${encodeURIComponent(sessionId)}/chat`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ content, client_msg_id: clientMsgId }),
  });
  if (!res.ok || !res.body) {
    let detail = `${res.status}`;
    try {
      const err = (await res.json()) as { error?: { code?: string; message?: string } };
      if (err.error) detail = `${err.error.code ?? res.status}: ${err.error.message ?? ""}`;
    } catch {
      // Non-JSON denial — keep the status.
    }
    throw new Error(detail);
  }

  yield { type: "RUN_STARTED", threadId: input.threadId, runId: input.runId };

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let messageId: string | null = null;
  let opened = false;
  let sawTerminal = false;
  // The server subscribes before replaying its buffer (and replays that
  // buffer again on a broadcast Lagged), so an append can arrive twice —
  // dedupe by (message_id, seq) per the C2 exactly-once rule.
  const seen = new Set<string>();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const { frames, rest } = parseSseBuffer(buffer);
      buffer = rest;
      for (const frame of frames) {
        if (frame.type === "append") {
          messageId = String(frame.message_id);
          const key = `${messageId}:${String(frame.seq)}`;
          if (seen.has(key)) continue;
          seen.add(key);
          if (!opened) {
            opened = true;
            yield { type: "TEXT_MESSAGE_START", messageId, role: "assistant" };
          }
          yield { type: "TEXT_MESSAGE_CONTENT", messageId, delta: String(frame.delta) };
        } else if (frame.type === "terminal") {
          sawTerminal = true;
          messageId = String(frame.message_id);
          if (opened) yield { type: "TEXT_MESSAGE_END", messageId };
          if (frame.result === "done") {
            yield { type: "RUN_FINISHED", threadId: input.threadId, runId: input.runId };
          } else {
            const code = typeof frame.code === "string" ? frame.code : "error";
            yield { type: "RUN_ERROR", message: code, code };
          }
          return;
        }
        // snapshot/ping/tool/card frames: display-level, not run deltas.
      }
    }
  } finally {
    try {
      await reader.cancel();
    } catch {
      // Already closed.
    }
  }
  if (!sawTerminal) {
    // Resend/idempotent path or truncated stream — the server ended without
    // a terminal; close the run cleanly rather than hanging `isLoading`.
    if (opened && messageId) yield { type: "TEXT_MESSAGE_END", messageId };
    yield { type: "RUN_FINISHED", threadId: input.threadId, runId: input.runId };
  }
}

/**
 * `useChat` over the session — the mandated TanStack AI Octane hook owns
 * messages/loading/error; the fetcher keeps the C2 wire. `initialMessages`
 * is read at client construction, so callers seed it from the snapshot
 * before first mount (remount on reseed).
 *
 * `hooks.onFetchError` fires synchronously inside the run when the fetcher
 * throws (HTTP denial / stream failure) — the `chat.error` surface is a
 * render-time snapshot, so callers that need the failure inside the same
 * `sendMessage` tick (the composer's draft-preservation check) read this
 * instead of the stale snapshot value.
 */
export function useOxpilotChat(
  sessionId: string,
  initialMessages: UIMessage[],
  hooks?: { onFetchError?: (error: Error) => void },
) {
  const hooksRef = useRef(hooks);
  hooksRef.current = hooks;
  const fetcher = useMemo(
    () =>
      ((input: ChatFetcherInput) =>
        (async function* () {
          try {
            yield* oxpilotFetcher(sessionId, input);
          } catch (e) {
            hooksRef.current?.onFetchError?.(e instanceof Error ? e : new Error(String(e)));
            throw e;
          }
        })()) as unknown as ChatFetcher,
    [sessionId],
  );
  return useChat({ threadId: sessionId, fetcher, initialMessages });
}
