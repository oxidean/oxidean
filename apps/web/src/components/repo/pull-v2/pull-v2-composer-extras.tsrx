import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { SavedReplyPublic } from "@oxidean/api-client";
import { Bookmark, Loader, Paperclip, Plus, Trash2 } from "@octanejs/lucide";
import { useRef, useState } from "octane";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

type ComposerExtrasProps = {
  value: string;
  onValueChange: (next: string) => void;
  disabled?: boolean;
  testidPrefix?: string;
};

function appendMarkdown(draft: string, markdown: string): string {
  const trimmed = draft.trimEnd();
  return trimmed ? `${trimmed}\n\n${markdown}` : markdown;
}

type UploadResponse = {
  ok?: boolean;
  data?: { id: string; url: string; name: string; content_type: string; size: number };
  error?: { code?: string; message?: string };
};

async function uploadCommentAttachment(file: File): Promise<string> {
  const qs =
    `?name=${encodeURIComponent(file.name)}` +
    `&content_type=${encodeURIComponent(file.type || "application/octet-stream")}`;
  const res = await fetch(`/api/uploads/comment-attachments${qs}` as string, {
    method: "POST",
    body: file,
  });
  const json = (await res.json().catch(() => null)) as UploadResponse | null;
  if (!res.ok || !json?.ok || !json.data) {
    throw new Error(json?.error?.message || `Upload failed (${res.status})`);
  }
  const d = json.data;
  const alt = d.name.replace(/[[\]]/g, "");
  return d.content_type.startsWith("image/") ? `![${alt}](${d.url})` : `[${alt}](${d.url})`;
}

/**
 * Composer toolbar extras shared by the v2 comment/review surfaces (#263):
 * a saved-replies picker (session-scoped savedReplies.* RPCs, hidden on auth
 * failure) and a paperclip attachment button that uploads raw bytes to
 * /api/uploads/comment-attachments and inserts the returned markdown link.
 */
export function ComposerExtras(props: ComposerExtrasProps) @{
  const queryClient = useQueryClient();
  const prefix = props.testidPrefix ?? "pull-v2";
  const [menuOpen, setMenuOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);

  const repliesQ = useQuery({
    queryKey: ["savedReplies", "list"] as const,
    queryFn: async () => {
      const res = await apiClient.savedReplies.list();
      if (!res.ok) {
        throw Object.assign(new Error(res.error.message || NETWORK_ERROR), {
          code: res.error.code,
        });
      }
      return res.data.replies;
    },
    enabled: menuOpen,
    retry: false,
  });
  const replies: SavedReplyPublic[] = repliesQ.data ?? [];
  const authFailed =
    !!repliesQ.error && (repliesQ.error as { code?: string }).code === "auth.unauthenticated";

  function insert(markdown: string) {
    props.onValueChange(appendMarkdown(props.value, markdown));
  }

  async function invalidateReplies() {
    await queryClient.invalidateQueries({ queryKey: ["savedReplies", "list"] });
  }

  async function createReply() {
    const name = window.prompt("Saved reply name");
    if (!name?.trim()) return;
    const body = window.prompt("Saved reply body (markdown)");
    if (!body?.trim()) return;
    setBusy(true);
    setError("");
    const res = await apiClient.savedReplies.create({ name: name.trim(), body: body.trim() });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not save the reply.");
      return;
    }
    await invalidateReplies();
  }

  async function deleteReply(id: string) {
    setBusy(true);
    setError("");
    const res = await apiClient.savedReplies.delete({ id });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not delete the reply.");
      return;
    }
    await invalidateReplies();
  }

  async function onFilesPicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = "";
    if (files.length === 0) return;
    const oversize = files.filter((f) => f.size > MAX_ATTACHMENT_BYTES);
    const ok = files.filter((f) => f.size <= MAX_ATTACHMENT_BYTES);
    setError("");
    if (oversize.length > 0) {
      setError(
        oversize.map((f) => f.name).join(", ") +
          (oversize.length === 1 ? " exceeds" : " exceed") +
          " the 10 MiB limit.",
      );
    }
    if (ok.length === 0) return;
    setUploading(true);
    let next = props.value;
    const failures: string[] = [];
    for (const file of ok) {
      try {
        next = appendMarkdown(next, await uploadCommentAttachment(file));
      } catch (err) {
        failures.push(`${file.name}: ${err instanceof Error ? err.message : "upload failed"}`);
      }
    }
    setUploading(false);
    if (failures.length > 0) {
      setError(failures.join(" "));
    }
    if (next !== props.value) {
      props.onValueChange(next);
    }
  }

  const toolCls =
    "inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50";

  <div className="flex items-center gap-0.5" data-testid={`${prefix}-composer-extras` as string}>
    <DropdownMenu
      open={menuOpen}
      onOpenChange={(open) => {
        setMenuOpen(open);
        if (!open) setError("");
      }}
    >
      <DropdownMenuTrigger
        type="button"
        className={toolCls}
        aria-label="Insert saved reply"
        title="Saved replies"
        disabled={props.disabled || busy}
        data-testid={`${prefix}-saved-replies` as string}
      >
        <Bookmark aria-hidden size={14} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-56">
        @if (repliesQ.isPending) {
          <DropdownMenuItem disabled>Loading saved replies…</DropdownMenuItem>;
        } @else {
          @if (authFailed) {
            <DropdownMenuItem disabled>Sign in to use saved replies</DropdownMenuItem>;
          } @else {
            @if (replies.length === 0) {
              <DropdownMenuItem disabled>No saved replies yet</DropdownMenuItem>;
            } @else {
              @for (const r of replies; key r.id) {
                <div
                  className="flex items-center gap-1"
                  data-testid={`${prefix}-reply-${r.id}` as string}
                >
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] text-foreground hover:bg-accent"
                    disabled={props.disabled || busy}
                    onClick={() => {
                      insert(r.body);
                      setMenuOpen(false);
                    }}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{r.name as string}</span>
                      <span className="block truncate text-[11px] text-muted-foreground">
                        {r.body as string}
                      </span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-destructive"
                    aria-label={`Delete saved reply ${r.name}`}
                    data-testid={`${prefix}-reply-delete-${r.id}` as string}
                    disabled={busy}
                    onClick={() => void deleteReply(r.id)}
                  >
                    <Trash2 aria-hidden size={12} />
                  </button>
                </div>;
              }
            }
          }
        }
        <DropdownMenuSeparator />
        <DropdownMenuItem
          data-testid={`${prefix}-reply-new` as string}
          disabled={props.disabled || busy || authFailed}
          onClick={() => void createReply()}
        >
          <Plus aria-hidden size={13} />
          New saved reply
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    <button
      type="button"
      className={toolCls}
      aria-label="Attach files"
      title="Attach files"
      disabled={props.disabled || uploading}
      data-testid={`${prefix}-attach` as string}
      onClick={() => fileRef.current?.click()}
    >
      @if (uploading) {
        <Loader aria-hidden size={14} className="animate-spin" />;
      } @else {
        <Paperclip aria-hidden size={14} />;
      }
    </button>
    <input
      ref={fileRef}
      type="file"
      multiple
      className="hidden"
      data-testid={`${prefix}-attach-input` as string}
      onChange={(e) => void onFilesPicked(e)}
    />
    <span
      className={cn("text-[11px] text-destructive", !error && "hidden")}
      role="alert"
      data-testid={`${prefix}-composer-extras-error` as string}
    >
      {error as string}
    </span>
  </div>;
}
