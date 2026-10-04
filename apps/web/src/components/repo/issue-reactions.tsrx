import type { ReactionContent, ReactionGroupPublic, ReactionTarget } from "@oxidean/api-client";
import { useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { apiClient } from "@/lib/api-client";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** The eight reaction contents (D-ISS-11). */
export const REACTION_CONTENTS: {
  content: ReactionContent;
  emoji: string;
  label: string;
}[] = [
  { content: "+1", emoji: "👍", label: "+1" },
  { content: "-1", emoji: "👎", label: "-1" },
  { content: "laugh", emoji: "😄", label: "laugh" },
  { content: "confused", emoji: "😕", label: "confused" },
  { content: "heart", emoji: "❤️", label: "heart" },
  { content: "hooray", emoji: "🎉", label: "hooray" },
  { content: "rocket", emoji: "🚀", label: "rocket" },
  { content: "eyes", emoji: "👀", label: "eyes" },
];

export type IssueReactionsBarProps = {
  owner: string;
  repoName: string;
  number: number;
  target: ReactionTarget;
  commentId?: string;
  reactions: ReactionGroupPublic[];
  canInteract: boolean;
  /** Extra work after a successful toggle (optional). */
  onToggled?: () => Promise<void>;
};

function groupFor(
  reactions: ReactionGroupPublic[],
  content: ReactionContent,
): ReactionGroupPublic | undefined {
  return reactions.find((r) => r.content === content);
}

function ReactionChip(props: {
  content: ReactionContent;
  emoji: string;
  label: string;
  count: number;
  mine: boolean;
  canInteract: boolean;
  disabled: boolean;
  onToggle: (content: ReactionContent) => void;
}) @{
  const title = props.count > 0 ? `${props.label} · ${props.count}` : props.label;

  @if (props.canInteract) {
    <button
      type="button"
      className={
        props.mine
          ? "inline-flex items-center gap-1 rounded-md border border-primary/40 bg-primary/10 px-1.5 py-0.5 text-[13px] text-foreground"
          : "inline-flex items-center gap-1 rounded-md border border-transparent px-1.5 py-0.5 text-[13px] text-muted-foreground hover:border-border hover:text-foreground"
      }
      aria-label={`React ${props.label}`}
      aria-pressed={props.mine}
      title={title}
      disabled={props.disabled}
      onClick={() => props.onToggle(props.content)}
    >
      <span aria-hidden="true">{props.emoji as string}</span>
      @if (props.count > 0) {
        <span className="tabular-nums">{String(props.count)}</span>;
      }
    </button>;
  } @else {
    <span
      className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[13px] text-muted-foreground"
      title={title}
      aria-label={props.label}
    >
      <span aria-hidden="true">{props.emoji as string}</span>
      @if (props.count > 0) {
        <span className="tabular-nums">{String(props.count)}</span>;
      }
    </span>;
  }
}

/** Emoji reaction bar for an issue or comment (D-ISS-11 / D-ISS-20). */
export function IssueReactionsBar(props: IssueReactionsBarProps) @{
  const queryClient = useQueryClient();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [local, setLocal] = useState<ReactionGroupPublic[]>(() => props.reactions ?? []);

  useEffect(() => {
    setLocal(props.reactions ?? []);
  }, [props.reactions]);

  async function toggle(content: ReactionContent) {
    if (!props.canInteract || pending) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.issue.reactions.toggle({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        target: props.target,
        commentId: props.commentId,
        content,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not update reaction.");
        setPending(false);
        return;
      }
      setLocal(res.data.reactions);
      await queryClient.invalidateQueries({
        queryKey: ["issue", "get", props.owner, props.repoName, props.number],
      });
      if (props.target === "comment") {
        await queryClient.invalidateQueries({
          queryKey: ["issue", "comments", "list", props.owner, props.repoName, props.number],
        });
      }
      if (props.onToggled) await props.onToggled();
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <div className="flex flex-col gap-1">
    <div
      role="toolbar"
      aria-label="Reactions"
      data-testid="issue-reactions"
      className="flex flex-wrap items-center gap-1"
    >
      @for (const item of REACTION_CONTENTS; key item.content) {
        <ReactionChip
          content={item.content}
          emoji={item.emoji}
          label={item.label}
          count={groupFor(local, item.content)?.count ?? 0}
          mine={!!groupFor(local, item.content)?.viewerHasReacted}
          canInteract={props.canInteract}
          disabled={pending}
          onToggle={(c) => void toggle(c)}
        />;
      }
    </div>
    @if (error) {
      <p role="alert" className="text-[12px] text-destructive">
        {error as string}
      </p>;
    }
  </div>;
}
