import { useState } from "octane";
import { useQueryClient } from "@octanejs/tanstack-query";
import { KeyRound, Pencil, Trash2 } from "@octanejs/lucide";
import type { PasskeyPublic } from "@oxidean/api-client";
import { AuthErrorBanner } from "@/components/auth-shell";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { toastError, toastSuccess } from "@/lib/toast";
import {
  browserSupportsWebAuthn,
  passkeyErrorMessage,
  runPasskeyRegistration,
} from "@/lib/two-factor";

const LIST_KEY = ["twoFactor", "status"] as const;

type PasskeySectionProps = {
  passkeys: PasskeyPublic[];
  /** Enroll-kind challenge id — registration completes the gated login. */
  challenge?: string | null;
  /** Signed-in path: refetch factor status after a change. */
  onChanged?: () => void;
  /** Challenge path: the API minted the session — caller navigates. */
  onEnrolled?: () => void;
};

/**
 * Passkey management (#130): platform-authenticator credentials (Face ID,
 * Touch ID, Windows Hello, security keys) with nickname, rename, and removal.
 */
export function PasskeySection(props: PasskeySectionProps) @{
  const queryClient = useQueryClient();
  const supported = typeof window !== "undefined" && browserSupportsWebAuthn();

  const [registerOpen, setRegisterOpen] = useState(false);
  const [nickname, setNickname] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [renamePending, setRenamePending] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<PasskeyPublic | null>(null);
  const [removePending, setRemovePending] = useState(false);

  async function afterChange() {
    if (props.onChanged) {
      props.onChanged();
    } else {
      await queryClient.invalidateQueries({ queryKey: [...LIST_KEY] });
    }
  }

  async function register() {
    setPending(true);
    setError("");
    try {
      const begin = await apiClient.twoFactor.passkeyRegisterBegin({
        challenge: props.challenge ?? null,
      });
      if (!begin.ok) {
        setError(rpcErrorMessage(begin.error));
        setPending(false);
        return;
      }
      const attestation = await runPasskeyRegistration(begin.data.publicKey);
      const done = await apiClient.twoFactor.passkeyRegisterComplete({
        nickname: nickname.trim() || "Passkey",
        attestation,
        challenge: props.challenge ?? null,
      });
      if (!done.ok) {
        setError(rpcErrorMessage(done.error));
        setPending(false);
        return;
      }
      setPending(false);
      setRegisterOpen(false);
      setNickname("");
      if (props.challenge && props.onEnrolled) {
        // Enroll challenge — the API already minted the session.
        props.onEnrolled();
        return;
      }
      toastSuccess("Passkey added.");
      await afterChange();
    } catch (e) {
      setError(passkeyErrorMessage(e));
      setPending(false);
    }
  }

  async function saveRename(id: string) {
    const next = renameValue.trim();
    if (!next) return;
    setRenamePending(true);
    try {
      const res = await apiClient.twoFactor.passkeyUpdate({ id, nickname: next });
      if (!res.ok) {
        toastError(rpcErrorMessage(res.error));
        setRenamePending(false);
        return;
      }
      setRenamingId(null);
      setRenamePending(false);
      toastSuccess("Passkey renamed.");
      await afterChange();
    } catch {
      toastError(networkErrorMessage());
      setRenamePending(false);
    }
  }

  async function removePasskey(id: string) {
    setRemovePending(true);
    try {
      const res = await apiClient.twoFactor.passkeyDelete({ id });
      if (!res.ok) {
        toastError(rpcErrorMessage(res.error));
        setRemovePending(false);
        return;
      }
      setRemoveTarget(null);
      setRemovePending(false);
      toastSuccess("Passkey removed.");
      await afterChange();
    } catch {
      toastError(networkErrorMessage());
      setRemovePending(false);
    }
  }

  <div className="flex flex-col gap-3">
    @if (props.passkeys.length === 0) {
      <p className="text-[14px] text-muted-foreground">
        No passkeys yet. A passkey lets you sign in with your device — Face ID, Touch ID,
        Windows Hello, or a security key.
      </p>;
    } @else {
      <List aria-label="Registered passkeys">
        @for (const pk of props.passkeys; key pk.id) {
          <ListRow>
            <KeyRound
              aria-hidden
              size={16}
              className="shrink-0 text-muted-foreground"
            />
            @if (renamingId === pk.id) {
              <form
                className="flex min-w-0 flex-1 items-center gap-2"
                method="post"
                action="#"
                onSubmit={(e) => {
                  e.preventDefault();
                  void saveRename(pk.id);
                }}
              >
                <Input
                  aria-label="Passkey nickname"
                  value={renameValue}
                  onInput={(ev) =>
                    setRenameValue((ev.currentTarget as HTMLInputElement).value)
                  }
                  className="h-8 max-w-56"
                />
                <Button type="submit" size="sm" disabled={renamePending}>
                  {(renamePending ? "Saving…" : "Save") as string}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={renamePending}
                  onClick={() => setRenamingId(null)}
                >
                  Cancel
                </Button>
              </form>;
            } @else {
              <>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[14px] font-medium text-foreground">
                    {pk.nickname as string}
                  </p>
                  <p className="text-[13px] text-muted-foreground">
                    {
                      `Added ${formatRelativeUpdated(pk.created_at).replace(/^Updated\s+/i, "")}${
                        pk.last_used_at
                          ? ` · Last used ${formatRelativeUpdated(pk.last_used_at).replace(/^Updated\s+/i, "")}`
                          : ""
                      }` as string
                    }
                  </p>
                </div>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Rename ${pk.nickname}`}
                  onClick={() => {
                    setRenamingId(pk.id);
                    setRenameValue(pk.nickname);
                  }}
                >
                  <Pencil aria-hidden size={15} />
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Remove ${pk.nickname}`}
                  className="text-destructive hover:text-destructive"
                  onClick={() => setRemoveTarget(pk)}
                >
                  <Trash2 aria-hidden size={15} />
                </Button>
              </>;
            }
          </ListRow>;
        }
      </List>;
    }

    @if (error) {
      <AuthErrorBanner message={error} />;
    }

    @if (registerOpen) {
      <form
        className="flex flex-wrap items-center gap-2"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void register();
        }}
      >
        <Input
          aria-label="Passkey nickname"
          placeholder="Nickname (e.g. MacBook Touch ID)"
          value={nickname}
          onInput={(ev) => setNickname((ev.currentTarget as HTMLInputElement).value)}
          className="h-9 max-w-64"
          disabled={pending}
        />
        <Button type="submit" disabled={pending}>
          {(pending ? "Waiting for passkey…" : "Add passkey") as string}
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={pending}
          onClick={() => {
            setRegisterOpen(false);
            setNickname("");
            setError("");
          }}
        >
          Cancel
        </Button>
      </form>;
    } @else {
      <div>
        <Button
          type="button"
          variant="outline"
          disabled={!supported}
          onClick={() => {
            setRegisterOpen(true);
            setError("");
          }}
        >
          Add a passkey
        </Button>
        @if (!supported) {
          <p className="mt-2 text-[13px] text-muted-foreground">
            This browser does not support passkeys.
          </p>;
        }
      </div>;
    }

    <AlertDialog
      open={removeTarget !== null}
      onOpenChange={(open) => {
        if (!open) setRemoveTarget(null);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove passkey?</AlertDialogTitle>
          <AlertDialogDescription className="break-words whitespace-normal">
            {
              (removeTarget
                ? `Remove “${removeTarget.nickname}”? You will no longer be able to use it to sign in.`
                : "") as string
            }
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={removePending}>Keep passkey</AlertDialogCancel>
          <Button
            type="button"
            variant="destructive"
            disabled={removePending || !removeTarget}
            onClick={() => {
              if (removeTarget) void removePasskey(removeTarget.id);
            }}
          >
            {(removePending ? "Removing…" : "Remove passkey") as string}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
