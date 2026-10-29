import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  OxpilotQueuedMessage,
  OxpilotSession,
  OxpilotToolCallCard,
} from "@oxidean/api-client";
import type { UIMessage } from "@tanstack/ai-client";
import { useEffect, useMemo, useRef, useState } from "octane";
import { ArrowLeft, Bot } from "@octanejs/lucide";
import { AppPage } from "@/components/app-page";
import { AuthErrorBanner } from "@/components/auth-shell";
import { OxpilotComposer } from "@/components/oxpilot/composer";
import { ContextDrawer } from "@/components/oxpilot/context-drawer";
import { OxpilotFlagOff } from "@/components/oxpilot/flag-off";
import { OxpilotTranscript, type TranscriptCard } from "@/components/oxpilot/transcript";
import { SessionsRail } from "@/components/oxpilot/sessions-rail";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { toUiMessage, useOxpilotChat } from "@/lib/oxpilot-chat.tsrx";
import {
  invalidateOxpilotQueries,
  oxpilotDiffListQuery,
  oxpilotGrantListQuery,
  oxpilotMessageQueueMutation,
  oxpilotQueueListQuery,
  oxpilotSessionGetQuery,
  oxpilotSessionListQueryOptions,
  oxpilotStatusQueryOptions,
  oxpilotTargetListQuery,
  oxpilotTurnCancelMutation,
} from "@/lib/oxpilot-queries";
import {
  createAppendDedupe,
  fetchOxpilotSnapshot,
  openOxpilotStream,
  type OxpilotActiveTurn,
  type OxpilotFrame,
} from "@/lib/oxpilot-stream";
import { authSessionQueryOptions } from "@/lib/session-queries";

/** `/oxpilot/{session}` — three-pane transcript workspace over the SSE stream. */
export function OxpilotSessionPage() @{
  <AppPage>
    <OxpilotSessionContent />
  </AppPage>;
}

/**
 * A `card` frame's ref — `card_kind` + `ref_id` verbatim (C2). The
 * transcript's `TranscriptCard` union accepts these alongside resolved
 * `tool_call` cards; per-kind resolvers turn refs into `diff`/`consent`.
 */
type CardRef = { card_kind: string; ref_id: string };

/**
 * Wire `tool_call` shapes (snapshot carrier, `messages[].tool_calls`,
 * frames) mark summary/index fields optional — the api-client card the
 * transcript renders requires them, so normalize once at the seam.
 */
function toCallCard(call: {
  call_id: string;
  call_index?: number;
  tool: string;
  status: string;
  args_summary?: string | null;
  output_summary?: string | null;
}): OxpilotToolCallCard {
  return {
    call_id: call.call_id,
    call_index: call.call_index ?? 0,
    tool: call.tool,
    status: call.status,
    args_summary: call.args_summary ?? null,
    output_summary: call.output_summary ?? null,
  };
}

/** `"${code}: ${message}"` — the query-layer/stream error envelope. */
function codeOf(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  return (msg.split(":")[0] ?? "").trim();
}

/**
 * `grantList{session_id}` refused ⇒ the flagged `session_id` amendment is
 * unlanded server-side — the code-shape degrade branch (BR7.4). Query-layer
 * errors carry `"${code}: ${message}"`, so the match is on the code segment
 * (prefixed or bare), never a probe.
 */
function isGrantAmendmentRefusal(e: unknown): boolean {
  const code = codeOf(e);
  return (
    code === "forbidden" ||
    code === "oxpilot.forbidden" ||
    code === "invalid_argument" ||
    code === "oxpilot.invalid_argument" ||
    code === "bad_input" ||
    code === "oxpilot.bad_input"
  );
}

/** The ONE not-found surface — a foreign session renders identically to an absent one. */
function SessionNotFound() @{
  <div className="mt-6">
    <EmptyState
      icon={<Bot size={22} />}
      title="Session not found"
      description="This session may have been deleted, or it belongs to another account."
      action={
        <AppLink
          href="/oxpilot"
          className="text-[13px] font-medium text-foreground underline underline-offset-2"
        >
          All sessions
        </AppLink>
      }
    />
  </div>;
}

/** One keyed-remount seed — the mount's pinned read set folded together. */
type Seed = {
  version: number;
  messages: UIMessage[];
  streamingIds: Set<string>;
  activeTurn: OxpilotActiveTurn | null;
  pendingQueue: OxpilotQueuedMessage[];
  toolCalls: Map<string, OxpilotToolCallCard>;
  cards: CardRef[];
  session: OxpilotSession | null;
};

function OxpilotSessionContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  const pathname = usePathname();
  const sessionId = String(matchPath("/oxpilot/$session", pathname)?.session ?? "").trim();
  usePageRedirect(
    meQ.data === null || meQ.isError ? `/login?returnTo=${encodeURIComponent(pathname)}` : null,
  );
  const qc = useQueryClient();
  const statusQ = useQuery(oxpilotStatusQueryOptions());
  const enabled = statusQ.data?.enabled === true;

  const [seed, setSeed] = useState<Seed | null>(null);
  const [seedError, setSeedError] = useState<string | null>(null);
  const [seedNotFound, setSeedNotFound] = useState(false);
  const [turnNotice, setTurnNotice] = useState<"error" | "aborted" | null>(null);

  /**
   * The pinned 4-read set — `fetchOxpilotSnapshot` + `sessionGet` +
   * `grantList{session_id}` + `diffList{session_id}` — folded into ONE
   * seed so mount/remount card recovery is wholesale, never merged.
   * A server that refuses `grantList{session_id}` (`forbidden` /
   * `invalid_argument` — the flagged amendment unlanded) degrades the
   * read to the 3-read set: pending consent then surfaces through the
   * Manage expander's `grantList{target_id}` path (BR7.4) instead of
   * reconstructed transcript cards — a code-shape branch, never a probe.
   */
  async function reseed() {
    if (!sessionId) return;
    try {
      const grantsOrNull = qc
        .fetchQuery(oxpilotGrantListQuery({ session_id: sessionId }, true))
        .catch((e: unknown) => {
          if (isGrantAmendmentRefusal(e)) return null;
          throw e;
        });
      const [snap, session, grants, diffs] = await Promise.all([
        fetchOxpilotSnapshot(sessionId),
        qc.fetchQuery(oxpilotSessionGetQuery(sessionId)),
        grantsOrNull,
        qc.fetchQuery(oxpilotDiffListQuery(sessionId, true)),
      ]);

      // Wire carriers first — `streaming_ids`/`tool_calls`/`cards` are
      // the snapshot's additive seed fields; each degrades to the
      // derived value when the wire omits it.
      const toolCalls = new Map<string, OxpilotToolCallCard>();
      if (snap.tool_calls) {
        for (const call of snap.tool_calls) {
          toolCalls.set(call.call_id, toCallCard(call));
        }
      } else {
        for (const m of snap.messages ?? []) {
          for (const call of m.tool_calls ?? []) {
            toolCalls.set(call.call_id, toCallCard(call));
          }
        }
      }
      const cards: CardRef[] = [];
      const pushCard = (card_kind: string, ref_id: string) => {
        if (!cards.some((c) => c.card_kind === card_kind && c.ref_id === ref_id)) {
          cards.push({ card_kind, ref_id });
        }
      };
      // Pending consent/diff cards are recovered from the authoritative
      // reads (they outlive their turn's frames); any replayed wire refs
      // merge dedup'd. `g.session_id` verifies the response's scope — an
      // unamended server that silently ignores `{session_id}` returns
      // other sessions' grants; those cards never render here.
      for (const g of grants ?? []) {
        // Exact-match only — an unamended server returns session-less
        // grants on this read; `== null` would admit every foreign
        // pending grant as a consent card here (R-10).
        if (g.session_id === sessionId) {
          pushCard("consent", g.id);
        }
      }
      for (const d of diffs ?? []) pushCard("diff", d.id);
      // `queued` refs render through the `queued` prop (`queueList`/
      // `pendingQueue`), never the card dispatch — same split as the
      // live `card` frame handler.
      for (const c of snap.cards ?? []) {
        if (c.card_kind !== "queued") pushCard(c.card_kind, c.ref_id);
      }

      setSeed((prev) => ({
        version: (prev?.version ?? 0) + 1,
        messages: (snap.messages ?? []).map(toUiMessage),
        streamingIds: new Set(
          snap.streaming_ids ??
            (snap.messages ?? []).filter((m) => m.status === "streaming").map((m) => m.id),
        ),
        activeTurn: snap.active_turn ?? null,
        pendingQueue: (snap.pending_queue ?? []) as OxpilotQueuedMessage[],
        toolCalls,
        cards,
        session,
      }));
      setSeedError(null);
      setSeedNotFound(false);
    } catch (e) {
      const code = codeOf(e);
      setSeedNotFound(code === "not_found" || code === "oxpilot.not_found");
      setSeedError(e instanceof Error ? e.message : "Could not load the session.");
    }
  }

  // First-attach reconcile guard — the child reports the divergence
  // signature it saw; an identical repeat means it survived a full
  // reseed, so remounting cannot converge (the fresh fetch is never
  // staler than the replay it read against) and the attach keeps the
  // seed state instead of looping. Other reseed arms clear it.
  const reconcileRef = useRef<string | null>(null);

  useEffect(() => {
    if (!enabled || !sessionId) return;
    reconcileRef.current = null;
    void reseed();
  }, [enabled, sessionId]);

  <div className="mx-auto w-full max-w-6xl px-4 py-8" data-testid="oxpilot-session-page">
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
        <OxpilotFlagOff reason={statusQ.data?.reason} />;
      } @else {
        <>
          @if (seedNotFound) {
            <SessionNotFound />;
          } @else {
            @if (seedError) {
              <AuthErrorBanner message={seedError} />;
            }
          }
          @if (turnNotice) {
            <div
              className="mt-4 flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3"
              role="alert"
            >
              <p className="text-[14px] text-foreground">
                {
                  (turnNotice === "aborted"
                    ? "Reply stopped."
                    : "The provider stopped mid-reply — your messages are saved.") as string
                }
              </p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  setTurnNotice(null);
                  void reseed();
                }}
              >
                Retry
              </Button>
            </div>;
          }
          @if (!seed) {
            <Skeleton className="mt-6 h-48 w-full" />;
          } @else {
            <OxpilotChatSurface
              key={`s-${seed.version}`}
              sessionId={sessionId}
              seed={seed}
              onReseed={(reconcileSig) => {
                if (reconcileSig !== undefined) {
                  if (reconcileRef.current === reconcileSig) return;
                  reconcileRef.current = reconcileSig;
                } else {
                  reconcileRef.current = null;
                }
                void reseed();
              }}
              onTurnNotice={(notice) => setTurnNotice(notice)}
              disabledReason={statusQ.data?.reason}
            />;
          }
        </>;
      }
    }
  </div>;
}

/**
 * The composition root — rail + transcript + composer + drawer around
 * the `useChat` send path and the live `EventSource` attach.
 */
function OxpilotChatSurface(props: {
  sessionId: string;
  seed: Seed;
  /**
   * Wholesale reseed request. The first-attach reconcile passes a
   * `reconcileSig` describing the divergence it saw so the parent can
   * bound repeats that a reseed cannot converge; every other caller
   * leaves it unset, which also clears the guard.
   */
  onReseed: (reconcileSig?: string) => void;
  /** A foreign turn's non-`done` terminal — surfaced above the remount key. */
  onTurnNotice: (notice: "error" | "aborted") => void;
  /** `status.reason` for the mid-session flag-off swap. */
  disabledReason: string | null | undefined;
}) @{
  const qc = useQueryClient();
  // The fetcher reports an HTTP/stream denial synchronously — `chat.error`
  // is a render-time snapshot that only lands on the NEXT render, so the
  // composer's draft-preservation check reads this ref instead.
  const sendFailRef = useRef<Error | null>(null);
  const chat = useOxpilotChat(props.sessionId, props.seed.messages, {
    onFetchError: (e) => {
      sendFailRef.current = e;
    },
  });
  const sessionsQ = useQuery(oxpilotSessionListQueryOptions(true));
  const targetsQ = useQuery(oxpilotTargetListQuery(true));
  const diffsQ = useQuery(oxpilotDiffListQuery(props.sessionId, true));

  const [liveTail, setLiveTail] = useState<Record<string, string>>({});
  const [toolCalls, setToolCalls] = useState<Map<string, OxpilotToolCallCard>>(
    () => props.seed.toolCalls,
  );
  const [cardRefs, setCardRefs] = useState<CardRef[]>(() => props.seed.cards);
  const [queuedRows, setQueuedRows] = useState<OxpilotQueuedMessage[]>(
    () => props.seed.pendingQueue,
  );
  // Locally-cancelled queued ids — a stale in-flight `queueList` resolve
  // must not resurrect a row the server already dropped (N-09). Only the
  // mount seed bypasses the filter (initial server truth).
  const deletedQueued = useRef<Set<string>>(new Set());
  const applyQueued = (rows: OxpilotQueuedMessage[]) =>
    setQueuedRows(rows.filter((q) => !deletedQueued.current.has(q.id)));
  const [fatal, setFatal] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [railCollapsed, setRailCollapsed] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const queueM = useMutation({ ...oxpilotMessageQueueMutation() });
  const turnCancelM = useMutation({ ...oxpilotTurnCancelMutation() });

  const busy = chat.isLoading;
  // `chat.isLoading` inside the attach effect would capture the mount-time
  // value — a ref keeps the own-turn check live for terminal frames.
  const busyRef = useRef(busy);
  useEffect(() => {
    busyRef.current = busy;
  }, [busy]);
  // Async writes outside the stream attach (the post-queue `queueList`
  // refresh) get the same no-writes-after-unmount guard.
  const mountedRef = useRef(true);
  useEffect(() => {
    return () => {
      mountedRef.current = false;
    };
  }, []);

  /**
   * Turn liveness by `message_id` — an `append` opens, its `terminal`
   * closes. Seeded from the snapshot's `active_turn`; a turn that starts
   * mid-attach (foreign) flips `turnLive` so the composer takes the
   * `messageQueue` branch instead of a 409'd idle POST.
   */
  const [liveTurns, setLiveTurns] = useState<Set<string>>(
    () => new Set(props.seed.activeTurn ? [props.seed.activeTurn.message_id] : []),
  );

  // Focus the composer once the workspace opens (a11y pin) — the
  // post-mount tick so keepMounted portals have settled.
  useEffect(() => {
    const t = window.setTimeout(() => {
      document
        .querySelector<HTMLElement>(
          '[data-testid="oxpilot-composer"] textarea, [data-testid="oxpilot-composer"]',
        )
        ?.focus();
    }, 0);
    return () => window.clearTimeout(t);
  }, []);

  // Live attach — the first post-attach `snapshot` replays the seed we
  // rendered, EXCEPT a foreign turn may have finished between the seed
  // fetch and the attach: persisted ids or the live turn disagreeing
  // with the seed means the replay carries newer durable state, so it
  // reseeds instead of being discarded. Every LATER snapshot is a
  // reconnect replay and reseeds wholesale through the keyed remount.
  // `tool_call` / `card` frames feed the keyed row maps; a foreign
  // turn's `terminal` reseeds (an own turn's terminal is useChat's to
  // close out).
  useEffect(() => {
    let attached = false;
    let dead = false;
    // Attach-lifetime dedupe — a remount or session switch gets a fresh set.
    const dedupe = createAppendDedupe();
    const handle = openOxpilotStream(
      props.sessionId,
      (frame: OxpilotFrame) => {
        if (dead) return;
        if (frame.type === "snapshot") {
          if (!attached) {
            attached = true;
            // First-attach reconcile — drift in persisted ids or the
            // live turn means the replay carries newer durable state
            // than the mount seed; reseed rather than drop it.
            const seedIds = props.seed.messages.map((m) => m.id).join("");
            const snapIds = (frame.messages ?? []).map((m) => String(m.id)).join("");
            const seedTurn = props.seed.activeTurn?.message_id ?? null;
            const snapTurn = frame.active_turn?.message_id ?? null;
            if (snapIds !== seedIds || snapTurn !== seedTurn) {
              setLiveTail({});
              props.onReseed(`${seedIds}|${snapIds}|${String(seedTurn)}|${String(snapTurn)}`);
            }
            return;
          }
          setLiveTail({});
          props.onReseed();
          return;
        }
        if (!dedupe(frame)) return;
        if (frame.type === "append") {
          setLiveTurns((prev) => new Set(prev).add(String(frame.message_id)));
          // Own turn: useChat already accumulates these deltas from the POST
          // stream — adding them to liveTail too would double-render. Keep
          // the tail only for foreign turns (busy false).
          if (!busyRef.current) {
            setLiveTail((prev) => ({
              ...prev,
              [String(frame.message_id)]:
                (prev[String(frame.message_id)] ?? "") + String(frame.delta),
            }));
          }
        } else if (frame.type === "tool_call") {
          const call = toCallCard({
            call_id: String(frame.call_id),
            call_index: typeof frame.call_index === "number" ? frame.call_index : undefined,
            tool: String(frame.tool ?? ""),
            status: String(frame.status ?? ""),
            args_summary: frame.args_summary as string | null | undefined,
            output_summary: frame.output_summary as string | null | undefined,
          });
          setToolCalls((prev) => new Map(prev).set(call.call_id, call));
        } else if (frame.type === "card") {
          const kind = String(frame.card_kind ?? "");
          const refId = String(frame.ref_id ?? "");
          if (kind === "queued") {
            // The queued row set is `queueList`-resolved — the card frame
            // is only the trigger to refresh it.
            void qc
              .fetchQuery(oxpilotQueueListQuery(props.sessionId, true))
              .then((queued) => {
                if (!dead) applyQueued(queued);
              })
              .catch(() => {
                // A failed queue read stays silent — the next card or the
                // reseed refreshes it; never a fabricated queue state.
              });
          } else {
            setCardRefs((prev) =>
              prev.some((c) => c.card_kind === kind && c.ref_id === refId)
                ? prev
                : [...prev, { card_kind: kind, ref_id: refId }],
            );
          }
        } else if (frame.type === "terminal") {
          setLiveTail({});
          setLiveTurns((prev) => {
            const next = new Set(prev);
            next.delete(String(frame.message_id));
            return next;
          });
          invalidateOxpilotQueries(qc);
          if (!busyRef.current) {
            const result = String(frame.result ?? "");
            if (result === "aborted") props.onTurnNotice("aborted");
            else if (result !== "done") props.onTurnNotice("error");
            props.onReseed();
          }
        }
      },
      {
        // `es.readyState === CLOSED` — the stream lib already closed the
        // EventSource and spent its one `sessionGet` probe naming the
        // denial; the code selects the typed surface (REL-3).
        onFatal: (code) => {
          if (!dead) setFatal(code);
        },
      },
    );
    return () => {
      dead = true;
      handle.close();
    };
  }, [props.sessionId]);

  /**
   * Idle send — POST `…/chat` through `useChat` (fetch-reader stream).
   * `useChat` surfaces send failures through `chat.error`, never a
   * rejection — the boolean tells the composer whether the draft clears.
   */
  async function sendIdle(content: string): Promise<boolean> {
    setSendError(null);
    setLiveTail({});
    sendFailRef.current = null;
    try {
      await chat.sendMessage(content, {
        body: { client_msg_id: crypto.randomUUID() },
      });
    } catch (e) {
      // Defensive — useChat surfaces failures through `chat.error`, but a
      // reject must not reach the composer's uncaught submit arm.
      setSendError(e instanceof Error ? e.message : "Could not send the message.");
      return false;
    }
    // The fetcher's synchronous report — `chat.error` is still the stale
    // pre-send snapshot here. `false` keeps the draft on a refused send.
    const failed = sendFailRef.current;
    if (failed) {
      setSendError(failed.message);
      return false;
    }
    return true;
  }

  /** Live-turn send — `oxpilot.message_queue` with a fresh idempotency key. */
  async function sendQueued(content: string): Promise<boolean> {
    setSendError(null);
    try {
      const receipt = await queueM.mutateAsync({
        session_id: props.sessionId,
        content,
        client_msg_id: crypto.randomUUID(),
      });
      if (receipt?.queued_id) {
        const queuedId = String(receipt.queued_id);
        setQueuedRows((prev) => [
          ...prev,
          {
            id: queuedId,
            queue_seq: prev.length + 1,
            content,
            client_msg_id: null,
            created_at: new Date().toISOString(),
          },
        ]);
        void qc
          .fetchQuery(oxpilotQueueListQuery(props.sessionId, true))
          .then((queued) => {
            if (mountedRef.current) applyQueued(queued);
          })
          .catch(() => {});
      }
      // `{queued_id:null, position:null}` — the transcript-merge resend
      // guard (U3-WF5): NO queued row renders.
      return true;
    } catch (e) {
      // `session_active_turn_absent`/`turn_active`/`budget_exceeded` —
      // a typed banner, never silent enqueue (C2's forbidden pair).
      setSendError(e instanceof Error ? e.message : "Could not queue the message.");
      return false;
    }
  }

  /**
   * Optional `[Stop]` affordance — `turnCancel` on a live own-turn.
   * `{aborted:false}`/`no_active_turn` are quiet no-ops: the turn ended
   * anyway (REL-15).
   */
  async function stopTurn() {
    try {
      await turnCancelM.mutateAsync({ session_id: props.sessionId });
    } catch {
      // Quiet — every outcome is server-authoritative.
    }
  }

  const tailIds = new Set(chat.messages.map((m) => m.id));
  const tails = Object.entries(liveTail).filter(([id]) => !tailIds.has(id));
  const surfaceCards: TranscriptCard[] = useMemo(
    () => [
      ...[...toolCalls.values()]
        .sort((a, b) => a.call_index - b.call_index)
        .map((call): TranscriptCard => ({ card_kind: "tool_call", call })),
      ...cardRefs,
    ],
    [toolCalls, cardRefs],
  );

  const chatError = chat.error
    ? chat.error instanceof Error
      ? chat.error.message
      : String(chat.error)
    : null;
  const sendBanner = sendError ?? chatError;
  const sendCode = sendBanner ? codeOf(sendBanner) : null;
  // WF4.3 — `oxpilot.disabled` on send/queue swaps to the flag-off
  // state, same surface as the mount/`fatal` path.
  const effectiveFatal = fatal ?? (sendCode === "oxpilot.disabled" ? sendCode : null);
  const turnLive = busy || liveTurns.size > 0;

  const targets = targetsQ.data ?? [];
  const boundTargetId =
    (props.seed.session as { computer_use_target_id?: string | null } | null)
      ?.computer_use_target_id ?? null;
  const boundTarget = boundTargetId ? (targets.find((t) => t.id === boundTargetId) ?? null) : null;
  // AC4.5.1 — `connected·busy` derives client-side from in-flight
  // exec/fs tool cards on the session's bound target (Q11-A); a
  // session binds at most one target, so any running exec/fs card
  // marks it busy.
  const busyTargets = useMemo(() => {
    const ids = new Set<string>();
    if (!boundTargetId) return ids;
    for (const call of toolCalls.values()) {
      if (
        call.status === "running" &&
        (call.tool === "exec_shell" || call.tool === "fs_read" || call.tool === "fs_write")
      ) {
        ids.add(boundTargetId);
        break;
      }
    }
    return ids;
  }, [toolCalls, boundTargetId]);
  const diffs = diffsQ.data ?? [];
  const latestDiff = diffs.length ? diffs[diffs.length - 1]! : null;

  <>
    @if (effectiveFatal === "oxpilot.disabled") {
      <OxpilotFlagOff reason={props.disabledReason} />;
    } @else {
      @if (effectiveFatal === "not_found") {
        <SessionNotFound />;
      } @else {
        <div className="mt-4 flex items-start gap-4">
          <SessionsRail
            sessionId={props.sessionId}
            sessions={sessionsQ.data ?? []}
            targets={targets}
            busyTargets={busyTargets}
            collapsed={railCollapsed}
            onToggle={() => setRailCollapsed((c) => !c)}
          />
          <div className="flex min-w-0 flex-1 flex-col">
            <PageHeader
              title={props.seed.session?.title || "Session"}
              description={
                props.seed.session
                  ? `${props.seed.session.repo_owner}/${props.seed.session.repo_name} · ${props.seed.session.branch}`
                  : props.sessionId
              }
              actions={
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  aria-expanded={drawerOpen}
                  onClick={() => setDrawerOpen((o) => !o)}
                  data-testid="oxpilot-context-toggle"
                >
                  Context
                </Button>
              }
            />
            @if (busy) {
              <div className="mt-3 flex justify-end">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void stopTurn()}
                >
                  Stop
                </Button>
              </div>;
            }
            @if (fatal) {
              <div
                className="mt-3 flex items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3"
                role="alert"
              >
                <p className="text-[14px] text-destructive">
                  {`The live stream ended (${fatal}).` as string}
                </p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => props.onReseed()}
                >
                  Retry
                </Button>
              </div>;
            }
            <OxpilotTranscript
              sessionId={props.sessionId}
              messages={chat.messages}
              streamingIds={props.seed.streamingIds}
              liveTail={liveTail}
              tails={tails}
              busy={busy}
              cards={surfaceCards}
              queued={queuedRows}
              turnFailed={sendBanner != null}
              onQueuedDelete={(queuedId) => {
                deletedQueued.current.add(queuedId);
                setQueuedRows((prev) => prev.filter((q) => q.id !== queuedId));
              }}
            />
            <OxpilotComposer
              busy={busy}
              turnLive={turnLive}
              onSend={sendIdle}
              onQueue={sendQueued}
            >
              @if (sendCode === "budget_exceeded") {
                <AuthErrorBanner
                  message={
                    (`Daily budget reached — send paused. Ask an admin to raise ` +
                      `OXIDEAN_OXPILOT_USER_BUDGET or OXIDEAN_OXPILOT_INSTANCE_BUDGET ` +
                      `(see docs/CONFIGURATION.md). ${sendBanner}`) as string
                  }
                />;
              } @else {
                @if (sendBanner && sendCode !== "oxpilot.disabled") {
                  <AuthErrorBanner message={sendBanner} />;
                }
              }
            </OxpilotComposer>
          </div>
          <ContextDrawer
            open={drawerOpen}
            onClose={() => setDrawerOpen(false)}
            session={props.seed.session}
            diff={latestDiff}
            target={boundTarget}
            targetBusy={boundTargetId !== null && busyTargets.has(boundTargetId)}
          />
        </div>;
      }
    }
  </>;
}
