import { useState } from "octane";
import { SendHorizontal } from "@octanejs/lucide";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

/**
 * WF4 composer — the `form` landmark under the transcript.
 *
 * Pinned behavior:
 * - visible `Label` "Message OxPilot" + `Ctrl/⌘+Enter` hint linked via
 *   `aria-describedby`;
 * - Enter inserts a newline; only Ctrl/⌘+Enter submits (keydown);
 * - Send is a real `type="submit"` button;
 * - NEVER disabled during a live turn — a `turnLive` submit routes to
 *   `onQueue` (`message_queue`), idle submits to `onSend`; the caller
 *   mints `client_msg_id` (`crypto.randomUUID()`);
 * - the input keeps its text on a failed submit so nothing is lost;
 * - `children` renders above the row — the route drops its typed
 *   send/queue error banner there.
 */
export function OxpilotComposer(props: {
  busy: boolean;
  turnLive: boolean;
  onSend: (content: string) => void | Promise<void>;
  onQueue: (content: string) => void | Promise<void>;
  children?: unknown;
}) @{
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);

  async function submit(ev?: { preventDefault: () => void }) {
    ev?.preventDefault();
    const content = text.trim();
    if (!content || sending) return;
    setSending(true);
    try {
      if (props.turnLive) {
        await props.onQueue(content);
      } else {
        await props.onSend(content);
      }
      setText("");
    } finally {
      setSending(false);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void submit();
    }
    // Plain Enter falls through — the textarea inserts its newline.
  }

  <form
    className="mt-4 flex flex-col gap-2"
    aria-label="Message composer"
    data-testid="oxpilot-composer"
    method="post"
    action="#"
    onSubmit={(e) => void submit(e)}
  >
    {props.children}
    <div className="flex flex-col gap-1.5">
      <Label htmlFor="oxpilot-composer-input">Message OxPilot</Label>
      <Textarea
        id="oxpilot-composer-input"
        rows={3}
        placeholder={props.turnLive ? "Turn in progress — your message queues." : "Ask OxPilot…"}
        value={text}
        aria-describedby="oxpilot-composer-hint"
        onInput={(e) => setText((e.target as HTMLTextAreaElement).value)}
        onKeydown={onKeydown}
      />
      <p id="oxpilot-composer-hint" className="text-[12px] text-muted-foreground">
        Enter adds a newline — Ctrl+Enter (⌘+Enter) sends.
      </p>
    </div>
    <div className="flex items-center justify-end gap-2">
      @if (props.busy) {
        <span role="status" className="mr-auto text-[12px] text-muted-foreground">
          OxPilot is replying…
        </span>;
      }
      <Button type="submit" disabled={!text.trim() || sending}>
        <SendHorizontal size={14} aria-hidden />
        {(props.turnLive ? "Queue" : "Send") as string}
      </Button>
    </div>
  </form>;
}
