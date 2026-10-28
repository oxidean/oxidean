import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { UIMessage } from "@tanstack/ai-client";
import { useEffect, useState } from "octane";
import { ArrowLeft, Bot, SendHorizonal, User } from "@octanejs/lucide";
import { AppPage } from "@/components/app-page";
import { AuthErrorBanner } from "@/components/auth-shell";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { toUiMessage, useOxpilotChat } from "@/lib/oxpilot-chat.tsrx";
import { invalidateOxpilotQueries, oxpilotStatusQueryOptions } from "@/lib/oxpilot-queries";
import {
  createAppendDedupe,
  fetchOxpilotSnapshot,
  openOxpilotStream,
  type OxpilotFrame,
} from "@/lib/oxpilot-stream";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { cn } from "@/lib/utils";

/** `/oxpilot/{session}` — transcript workspace + composer over the SSE stream. */
export function OxpilotSessionPage() @{
  <AppPage>
    <OxpilotSessionContent />
  </AppPage>;
}

type Seed = { version: number; messages: UIMessage[]; streamingIds: Set<string> };

function OxpilotSessionContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/oxpilot" : null);
  const pathname = usePathname();
  const sessionId = String(matchPath("/oxpilot/$session", pathname)?.session ?? "").trim();
  const statusQ = useQuery(oxpilotStatusQueryOptions());
  const enabled = statusQ.data?.enabled === true;

  const [seed, setSeed] = useState<Seed | null>(null);
  const [seedError, setSeedError] = useState<string | null>(null);

  async function reseed() {
    try {
      const snap = await fetchOxpilotSnapshot(sessionId);
      const rows = snap.messages ?? [];
      setSeed((prev) => ({
        version: (prev?.version ?? 0) + 1,
        messages: rows.map(toUiMessage),
        streamingIds: new Set(rows.filter((m) => m.status === "streaming").map((m) => m.id)),
      }));
      setSeedError(null);
    } catch (e) {
      setSeedError(e instanceof Error ? e.message : "Could not load the session.");
    }
  }

  useEffect(() => {
    if (!enabled || !sessionId) return;
    void reseed();
  }, [enabled, sessionId]);

  <main
    className="mx-auto flex w-full max-w-4xl flex-col px-4 py-8"
    data-testid="oxpilot-session-page"
  >
    <AppLink
      href="/oxpilot"
      className="inline-flex w-fit items-center gap-1 text-[13px] text-muted-foreground hover:text-foreground"
    >
      <ArrowLeft size={14} aria-hidden />
      All sessions
    </AppLink>
    @if (statusQ.isPending || meQ.isPending) {
      <Skeleton className="mt-6 h-48 w-full" />;
    } @else {
      @if (!enabled) {
        <p className="mt-6 rounded-lg border border-dashed border-border bg-card/40 px-6 py-8 text-center text-[14px] text-muted-foreground">
          Oxpilot is not enabled on this instance ({`OXIDEAN_OXPILOT_ENABLED` as string}). See{" "}
          <code className="font-mono">docs/CONFIGURATION.md</code>.
        </p>;
      } @else {
        <>
          @if (seedError) {
            <AuthErrorBanner message={seedError} />;
          }
          @if (!seed) {
            <Skeleton className="mt-6 h-48 w-full" />;
          } @else {
            <OxpilotChatSurface
              key={`s-${seed.version}`}
              sessionId={sessionId}
              initialMessages={seed.messages}
              streamingIds={seed.streamingIds}
              onReseed={() => void reseed()}
            />;
          }
        </>;
      }
    }
  </main>;
}

function messageText(m: UIMessage): string {
  return m.parts
    .filter((p) => p.type === "text")
    .map((p) => p.content)
    .join("");
}

function OxpilotChatSurface(props: {
  sessionId: string;
  initialMessages: UIMessage[];
  streamingIds: Set<string>;
  onReseed: () => void;
}) @{
  const qc = useQueryClient();
  const chat = useOxpilotChat(props.sessionId, props.initialMessages);
  const [draft, setDraft] = useState("");
  const [liveTail, setLiveTail] = useState<Record<string, string>>({});
  const [dedupe] = useState(() => createAppendDedupe());

  // Live attach — foreign/background turns surface as a transient tail
  // bubble; a terminal on a turn this mount does not own reseeds the
  // canonical transcript through the parent. The attach-time snapshot is
  // the seed we already rendered, so it is skipped; a snapshot arriving
  // later means a reconnect replay and is applied.
  useEffect(() => {
    let attached = false;
    const handle = openOxpilotStream(props.sessionId, (frame: OxpilotFrame) => {
      if (frame.type === "snapshot") {
        if (!attached) {
          attached = true;
          return;
        }
        setLiveTail({});
        props.onReseed();
        return;
      }
      if (!dedupe(frame)) return;
      if (frame.type === "append") {
        setLiveTail((prev) => ({
          ...prev,
          [String(frame.message_id)]: (prev[String(frame.message_id)] ?? "") + String(frame.delta),
        }));
      } else if (frame.type === "terminal") {
        setLiveTail({});
        invalidateOxpilotQueries(qc);
        if (!chat.isLoading) props.onReseed();
      }
    });
    return () => handle.close();
  }, [props.sessionId]);

  async function send(ev: SubmitEvent) {
    ev.preventDefault();
    const content = draft.trim();
    if (!content || chat.isLoading) return;
    setLiveTail({});
    await chat.sendMessage(content, {
      body: { client_msg_id: crypto.randomUUID() },
    });
    setDraft("");
  }

  const busy = chat.isLoading;
  const tailIds = new Set(chat.messages.map((m) => m.id));
  const tails = Object.entries(liveTail).filter(([id]) => !tailIds.has(id));

  <>
    <PageHeader title="Session" description={props.sessionId} />
    <section
      className="mt-4 flex min-h-[16rem] flex-col gap-3 rounded-lg border border-border bg-card p-4"
      data-testid="oxpilot-transcript"
      aria-live="polite"
    >
      @if (chat.messages.length === 0 && tails.length === 0) {
        <p className="text-[14px] text-muted-foreground">
          No messages yet — send the first one below.
        </p>;
      }
      @for (const m of chat.messages; key m.id) {
        <div
          className={cn(
            "flex gap-3 rounded-md px-3 py-2",
            m.role === "user" ? "bg-muted/60" : "bg-card",
          )}
          data-testid={`oxpilot-msg-${m.role}`}
        >
          <span className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden>
            @if (m.role === "user") {
              <User size={16} />;
            } @else {
              <Bot size={16} />;
            }
          </span>
          <div className="min-w-0 flex-1">
            <p className="whitespace-pre-wrap break-words text-[14px] text-foreground">
              {
                // A seeded `streaming` row is a foreign turn in flight —
                // its live tail overlays the persisted partial text.
                (messageText(m) +
                  (props.streamingIds.has(m.id) ? (liveTail[m.id] ?? "") : "")) as string
              }
            </p>
            @if (props.streamingIds.has(m.id)) {
              <p className="mt-1 text-[12px] text-muted-foreground">
                Oxpilot is typing…
              </p>;
            }
          </div>
        </div>;
      }
      @for (const [id, text] of tails; key id) {
        <div
          className="flex gap-3 rounded-md bg-card px-3 py-2"
          data-testid="oxpilot-msg-assistant"
        >
          <span className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden>
            <Bot size={16} />
          </span>
          <div className="min-w-0 flex-1">
            <p className="whitespace-pre-wrap break-words text-[14px] text-foreground">
              {text as string}
            </p>
            <p className="mt-1 text-[12px] text-muted-foreground">Oxpilot is typing…</p>
          </div>
        </div>;
      }
      @if (busy) {
        <p className="text-[12px] text-muted-foreground">Oxpilot is replying…</p>;
      }
    </section>
    <form
      method="post"
      action="#"
      className="mt-4 flex flex-col gap-2"
      onSubmit={send}
      data-testid="oxpilot-composer"
    >
      @if (chat.error) {
        <AuthErrorBanner message={chat.error.message} />;
      }
      <Textarea
        aria-label="Message Oxpilot"
        rows={3}
        placeholder="Message Oxpilot…"
        value={draft}
        onInput={(e) => setDraft((e.target as HTMLTextAreaElement).value)}
      />
      <div className="flex justify-end">
        <Button type="submit" disabled={busy || draft.trim() === ""}>
          <SendHorizonal size={14} aria-hidden className="mr-1" />
          {busy ? "Oxpilot is replying…" : "Send"}
        </Button>
      </div>
    </form>
  </>;
}
