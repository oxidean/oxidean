import type {
  OxpilotGrantView,
  OxpilotProposedDiff,
  OxpilotQueuedMessage,
  OxpilotToolCallCard,
} from "@oxidean/api-client";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { UIMessage } from "@tanstack/ai-client";
import { useEffect, useRef, useState } from "octane";
import { ArrowDown } from "@octanejs/lucide";
import { OxpilotConsentCard } from "@/components/oxpilot/consent-card";
import { OxpilotConfirmDialog } from "@/components/oxpilot/confirm-dialog";
import { OxpilotDiffCard } from "@/components/oxpilot/diff-card";
import { OxpilotQueuedRow } from "@/components/oxpilot/queued-row";
import { OxpilotToolCall } from "@/components/oxpilot/tool-call-card";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  invalidateOxpilotQueries,
  oxpilotDiffApplyMutation,
  oxpilotDiffGetQuery,
  oxpilotGrantGetQuery,
  oxpilotQueueDeleteMutation,
} from "@/lib/oxpilot-queries";

/** A `card` frame ref — `{card_kind, ref_id}` (C2), resolved per kind. */
type CardRef = { card_kind: string; ref_id: string };

/**
 * One transcript card row — a resolved tool call or a ref the per-kind
 * resolver turns into `diff`/`consent` cards. Unknown kinds degrade to a
 * generic notice (WF12).
 */
export type TranscriptCard = { card_kind: "tool_call"; call: OxpilotToolCallCard } | CardRef;

/** Stable `@for` key — `tool_call` rows key by `call_id`, refs by kind+id. */
function cardKey(card: TranscriptCard): string {
  return "call" in card ? `tool:${card.call.call_id}` : `${card.card_kind}:${card.ref_id}`;
}

/** Text parts of a UIMessage joined — streaming text stays plaintext. */
function textOf(m: UIMessage): string {
  return m.parts
    .filter((p) => p.type === "text")
    .map((p) => (p as { content?: string }).content ?? "")
    .join("");
}

/** `card{diff}` ref → `diff_get` → `OxpilotDiffCard`. */
function DiffCardRef(props: {
  refId: string;
  onApply: (diff: OxpilotProposedDiff) => void;
}) @{
  const diffQ = useQuery(oxpilotDiffGetQuery(props.refId, true));
  @if (diffQ.isPending) {
    <Skeleton className="h-20 w-full" />;
  } @else {
    @if (diffQ.isError || !diffQ.data) {
      <p role="status" className="text-[13px] text-muted-foreground">
        Could not load a proposed diff.
      </p>;
    } @else {
      <OxpilotDiffCard
        diff={diffQ.data as OxpilotProposedDiff}
        onApply={() => props.onApply(diffQ.data as OxpilotProposedDiff)}
      />;
    }
  }
}

/** `card{consent}` ref → `grant_get` → `OxpilotConsentCard`. */
function ConsentCardRef(props: { refId: string }) @{
  const grantQ = useQuery(oxpilotGrantGetQuery(props.refId, true));
  @if (grantQ.isPending) {
    <Skeleton className="h-20 w-full" />;
  } @else {
    @if (grantQ.isError || !grantQ.data) {
      <p role="status" className="text-[13px] text-muted-foreground">
        Could not load a consent request.
      </p>;
    } @else {
      <OxpilotConsentCard grant={grantQ.data as OxpilotGrantView} />;
    }
  }
}

/**
 * WF3/WF10 transcript — the workspace `main` landmark.
 *
 * - `aria-live="polite" aria-atomic="false"` on a visually-hidden region
 *   that carries ONLY message start/end boundary announcements — never a
 *   stream delta (BR10.5).
 * - Streaming text is plaintext forever: `liveTail`/`tails` render as
 *   text nodes with a `▌`/`typing…` indicator; only a COMPLETED
 *   assistant message runs `renderGfm` once → `MarkdownHtml` (SEC-2
 *   sink #1, inside `MarkdownBody`).
 * - Autoscroll follows only while pinned at the bottom; scrolling up
 *   preserves position and reveals the "Jump to latest" pill.
 * - `cards` dispatch to tool-call / diff / consent children; unknown
 *   kinds get a generic textual notice; `queued` rows render
 *   `OxpilotQueuedRow` (`◐` + text).
 * - `[Create branch + PR]` on a pending diff rides the shared
 *   `OxpilotConfirmDialog` before `diff_apply` (SEC-12).
 */
export function OxpilotTranscript(props: {
  sessionId: string;
  messages: UIMessage[];
  streamingIds: Set<string>;
  liveTail: Record<string, string>;
  tails: [string, string][];
  busy: boolean;
  cards: TranscriptCard[];
  queued: OxpilotQueuedMessage[];
  /** Post-terminal-error flag — drives `stillQueued` copy (M8). */
  turnFailed?: boolean;
  /** Row-drop callback for a deleted queued message (N-01). */
  onQueuedDelete?: (queuedId: string) => void;
}) @{
  const qc = useQueryClient();
  const listRef = useRef<HTMLDivElement | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [announce, setAnnounce] = useState("");
  const [confirmDiff, setConfirmDiff] = useState<OxpilotProposedDiff | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);

  const applyM = useMutation({
    ...oxpilotDiffApplyMutation(),
    onSuccess: () => {
      setApplyError(null);
      invalidateOxpilotQueries(qc);
    },
    onError: (e) => {
      setApplyError(e instanceof Error ? e.message : "Could not apply the diff.");
    },
  });

  const queueDelM = useMutation({
    ...oxpilotQueueDeleteMutation(),
    onSuccess: (_data, vars) => {
      // The server emits no `card{queued}` on delete — drop the row
      // locally so the cancel is visibly honored (N-01).
      props.onQueuedDelete?.(vars.queued_id);
      invalidateOxpilotQueries(qc);
    },
    onError: (e) => {
      setApplyError(e instanceof Error ? e.message : "Could not remove the queued message.");
    },
  });

  // Boundary announcements — the ONLY things the live region ever says.
  const anyStreaming =
    props.busy || props.streamingIds.size > 0 || Object.keys(props.liveTail).length > 0;
  const wasStreaming = useRef(false);
  useEffect(() => {
    if (anyStreaming && !wasStreaming.current) setAnnounce("Reply started");
    else if (!anyStreaming && wasStreaming.current) setAnnounce("Reply complete");
    wasStreaming.current = anyStreaming;
  }, [anyStreaming]);

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLDivElement;
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
  }

  function jumpToLatest() {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    setAtBottom(true);
  }

  // Follow the tail only while the user is already at the bottom.
  useEffect(() => {
    if (atBottom) jumpToLatest();
  }, [props.messages, props.liveTail, props.tails, props.queued, props.cards, atBottom]);

  const empty =
    props.messages.length === 0 &&
    props.tails.length === 0 &&
    props.cards.length === 0 &&
    props.queued.length === 0;

  <section aria-label="Transcript" className="relative mt-4 min-w-0 flex-1">
    <div aria-live="polite" aria-atomic="false" className="sr-only">
      {announce as string}
    </div>
    <div
      ref={listRef}
      onScroll={onScroll}
      className="flex max-h-[62vh] min-h-64 flex-col gap-3 overflow-y-auto rounded-lg border border-border bg-card/40 p-4"
    >
      @if (empty && !props.busy) {
        <p className="my-auto text-center text-[13px] text-muted-foreground">
          No messages yet — send one below to start the session.
        </p>;
      }
      @for (const m of props.messages; key m.id) {
        @if (m.role === "user") {
          <div className="flex justify-end">
            <div className="max-w-[85%] rounded-lg bg-primary/10 px-3 py-2">
              <p className="whitespace-pre-wrap text-[14px] text-foreground">
                {textOf(m) as string}
              </p>
            </div>
          </div>;
        } @else {
          <div className="flex justify-start">
            <div className="max-w-[92%]">
              @if (
                props.streamingIds.has(m.id) ||
                props.liveTail[m.id] !== undefined
              ) {
                <>
                  <p className="whitespace-pre-wrap text-[14px] text-foreground">
                    {(textOf(m) + (props.liveTail[m.id] ?? "")) as string}
                    <span aria-hidden className="text-primary">
                      ▌
                    </span>
                  </p>
                  <p className="mt-1 text-[12px] text-muted-foreground">typing…</p>
                </>;
              } @else {
                <MarkdownBody markdown={textOf(m)} />;
              }
            </div>
          </div>;
        }
      }
      @for (const tail of props.tails; key tail[0]) {
        <div className="flex justify-start">
          <div className="max-w-[92%]">
            <p className="whitespace-pre-wrap text-[14px] text-foreground">
              {tail[1] as string}
              <span aria-hidden className="text-primary">
                ▌
              </span>
            </p>
            <p className="mt-1 text-[12px] text-muted-foreground">typing…</p>
          </div>
        </div>;
      }
      @if (props.busy) {
        <p className="text-[12px] text-muted-foreground">
          <span aria-hidden>▌ </span>OxPilot is typing…
        </p>;
      }
      @if (props.cards.length > 0) {
        <div className="flex flex-col gap-2">
          @for (const card of props.cards; key cardKey(card)) {
            @if (card.card_kind === "tool_call" && "call" in card) {
              <OxpilotToolCall call={card.call} />;
            } @else {
              @if (card.card_kind === "diff") {
                <DiffCardRef refId={card.ref_id} onApply={(d) => setConfirmDiff(d)} />;
              } @else {
                @if (card.card_kind === "consent") {
                  <ConsentCardRef refId={card.ref_id} />;
                } @else {
                  <p role="status" className="text-[13px] text-muted-foreground">
                    {
                      `Unsupported card kind “${card.card_kind}” — update your client.` as string
                    }
                  </p>;
                }
              }
            }
          }
        </div>;
      }
      @if (applyError) {
        <p role="alert" className="text-[13px] text-destructive">
          {applyError as string}
        </p>;
      }
      @if (props.queued.length > 0) {
        <div className="flex flex-col gap-2">
          @for (const q of props.queued; key q.id) {
            <OxpilotQueuedRow
              queued={q}
              stillQueued={props.turnFailed === true}
              onCancel={() =>
                queueDelM.mutate({ session_id: props.sessionId, queued_id: q.id })
              }
            />;
          }
        </div>;
      }
    </div>
    @if (!atBottom) {
      <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
        <Button
          type="button"
          size="sm"
          className="pointer-events-auto rounded-full shadow-md"
          onClick={() => jumpToLatest()}
        >
          <ArrowDown size={14} aria-hidden />
          Jump to latest
        </Button>
      </div>;
    }
    <OxpilotConfirmDialog
      open={confirmDiff !== null}
      title="Create branch + PR?"
      body={`This applies ${confirmDiff?.title ?? "the proposed diff"} on a new branch and opens a pull request.`}
      confirmLabel="Create branch + PR"
      onConfirm={async () => {
        if (!confirmDiff) return;
        await applyM.mutateAsync({ proposal_id: confirmDiff.id }).catch(() => {});
      }}
      onClose={() => setConfirmDiff(null)}
    />
  </section>;
}
