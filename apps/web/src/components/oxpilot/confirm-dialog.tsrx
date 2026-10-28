import { useState } from "octane";
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

/**
 * The M5 shared confirm shell — every destructive action in the unit
 * (session delete, target disconnect, target revoke, `[Create branch +
 * PR]` → `diff_apply`) rides this one `AlertDialog` (SEC-12, BR6.3):
 *
 * - the `title` names the object, so the dialog's accessible name does;
 * - the confirm button carries the destructive label verbatim;
 * - Cancel is first in DOM order so initial focus lands on it;
 * - single explicit confirm — no type-to-confirm anywhere in the unit.
 *
 * `onConfirm` may return a promise (the parent's proc); the dialog stays
 * open on a rejected call so the caller's typed error surface is the only
 * record — closing is the parent's `onClose` decision on success.
 */
export function OxpilotConfirmDialog(props: {
  open: boolean;
  /** Names the object — e.g. "Delete this session?", `Revoke ${label}?`. */
  title: string;
  /** Optional consequence text under the title. */
  body?: string;
  /** The destructive label verbatim — e.g. "Delete session", "Revoke". */
  confirmLabel: string;
  /** Danger styling on the confirm (destructive actions). */
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
  onClose: () => void;
}) @{
  const [pending, setPending] = useState(false);

  async function confirm() {
    if (pending) return;
    setPending(true);
    try {
      await props.onConfirm();
      props.onClose();
    } finally {
      setPending(false);
    }
  }

  <AlertDialog
    open={props.open}
    onOpenChange={(next) => {
      if (pending) return;
      if (!next) props.onClose();
    }}
  >
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{props.title as string}</AlertDialogTitle>
        @if (props.body) {
          <AlertDialogDescription className="break-words whitespace-normal">
            {props.body as string}
          </AlertDialogDescription>;
        }
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel disabled={pending}>Cancel</AlertDialogCancel>
        <Button
          type="button"
          variant={props.danger === true ? "destructive" : "outline"}
          disabled={pending}
          onClick={() => void confirm()}
        >
          {(pending ? "Working…" : props.confirmLabel) as string}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
