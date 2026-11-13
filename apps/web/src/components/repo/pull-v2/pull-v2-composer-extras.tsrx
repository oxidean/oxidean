import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { SavedReplyPublic } from "@oxidean/api-client";
import { Bookmark, Loader, Paperclip, Plus, Trash2 } from "@octanejs/lucide";
import { useRef, useState } from "octane";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;

type ComposerExtrasProps = {
  value: string;
  onValueChange: (next: string) => void;
  disabled?: boolean;
  testidPrefix?: string;
  /** Notifies the parent while an attachment upload is in flight so submit
   * controls can stay disabled (the draft stays editable). */
  onUploadingChange?: (uploading: boolean) => void;
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
  const [createOpen, setCreateOpen] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftBody, setDraftBody] = useState("");
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
  const listError = repliesQ.isError
    ? String((repliesQ.error as Error | undefined)?.message ?? "Could not load saved replies.")
    : "";
  // Latest draft value — uploads append to whatever the composer holds when
  // they finish, not the snapshot from pick time (keystrokes during a slow
  // upload must not be lost).
  const latest = useRef(props.value);
  latest.current = props.value;

  function insert(markdown: string) {
    props.onValueChange(appendMarkdown(props.value, markdown));
  }

  async function invalidateReplies() {
    await queryClient.invalidateQueries({ queryKey: ["savedReplies", "list"] });
  }

  function openCreateDialog() {
    setDraftName("");
    setDraftBody("");
    setError("");
    setCreateOpen(true);
  }

  async function createReply() {
    const name = draftName.trim();
    const body = draftBody.trim();
    if (!name || !body) return;
    setBusy(true);
    setError("");
    try {
      const res = await apiClient.savedReplies.create({ name, body });
      if (!res.ok) {
        setError(res.error.message || "Could not save the reply.");
        return;
      }
      setCreateOpen(false);
      await invalidateReplies();
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setBusy(false);
    }
  }

  async function deleteReply(id: string) {
    setBusy(true);
    setError("");
    try {
      const res = await apiClient.savedReplies.delete({ id });
      if (!res.ok) {
        setError(res.error.message || "Could not delete the reply.");
        return;
      }
      await invalidateReplies();
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setBusy(false);
    }
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
    props.onUploadingChange?.(true);
    const parts: string[] = [];
    const failures: string[] = [];
    try {
      for (const file of ok) {
        try {
          parts.push(await uploadCommentAttachment(file));
        } catch (err) {
          failures.push(`${file.name}: ${err instanceof Error ? err.message : "upload failed"}`);
        }
      }
    } finally {
      setUploading(false);
      props.onUploadingChange?.(false);
    }
    if (failures.length > 0) {
      setError(failures.join(" "));
    }
    if (parts.length > 0) {
      let next = latest.current;
      for (const part of parts) next = appendMarkdown(next, part);
      if (next !== latest.current) props.onValueChange(next);
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
            @if (repliesQ.isError) {
              <DropdownMenuItem disabled>{listError as string}</DropdownMenuItem>;
            } @else {
              @if (replies.length === 0) {
                <DropdownMenuItem disabled>No saved replies yet</DropdownMenuItem>;
              } @else {
                @for (const r of replies; key r.id) {
                  <>
                    <DropdownMenuItem
                      disabled={props.disabled || busy}
                      data-testid={`${prefix}-reply-${r.id}` as string}
                      onClick={() => {
                        insert(r.body);
                        setMenuOpen(false);
                      }}
                    >
                      <span className="flex min-w-0 flex-1 flex-col">
                        <span className="truncate font-medium">{r.name as string}</span>
                        <span className="truncate text-[11px] text-muted-foreground">
                          {r.body as string}
                        </span>
                      </span>
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={busy}
                      data-testid={`${prefix}-reply-delete-${r.id}` as string}
                      aria-label={`Delete saved reply ${r.name}`}
                      onClick={() => void deleteReply(r.id)}
                    >
                      <Trash2 aria-hidden size={12} />
                      <span className="text-muted-foreground">Delete</span>
                    </DropdownMenuItem>
                  </>;
                }
              }
            }
          }
        }
        <DropdownMenuSeparator />
        <DropdownMenuItem
          data-testid={`${prefix}-reply-new` as string}
          disabled={props.disabled || busy || authFailed}
          onClick={openCreateDialog}
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
    <Dialog
      open={createOpen}
      onOpenChange={(open) => {
        if (busy) return;
        setCreateOpen(open);
        if (!open) setError("");
      }}
    >
      <DialogContent
        className="sm:max-w-lg"
        data-testid={`${prefix}-reply-create-dialog` as string}
      >
        <DialogHeader>
          <DialogTitle>New saved reply</DialogTitle>
          <DialogDescription>
            Saved replies insert markdown into any comment composer.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${prefix}-reply-name` as string}>Name</Label>
            <Input
              id={`${prefix}-reply-name` as string}
              value={draftName}
              onInput={(e) => setDraftName((e.currentTarget as HTMLInputElement).value)}
              placeholder="e.g. Repro steps"
              data-testid={`${prefix}-reply-name-input` as string}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${prefix}-reply-body` as string}>Body (markdown)</Label>
            <Textarea
              id={`${prefix}-reply-body` as string}
              value={draftBody}
              onInput={(e) => setDraftBody((e.currentTarget as HTMLTextAreaElement).value)}
              placeholder={"Line one\n\n- detail\n- detail" as string}
              className="min-h-28 font-mono text-[13px]"
              data-testid={`${prefix}-reply-body-input` as string}
            />
          </div>
          @if (error) {
            <p role="alert" className="text-[13px] text-destructive">
              {error as string}
            </p>;
          }
        </div>
        <DialogFooter>
          <button
            type="button"
            className="rounded-md px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-muted"
            disabled={busy}
            onClick={() => setCreateOpen(false)}
          >
            Cancel
          </button>
          <button
            type="button"
            className="rounded-md bg-primary px-3 py-1.5 text-[13px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
            disabled={busy || !draftName.trim() || !draftBody.trim()}
            data-testid={`${prefix}-reply-create-save` as string}
            onClick={() => void createReply()}
          >
            {busy ? "Saving…" : "Save reply"}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>;
}
