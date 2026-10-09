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
import { Label } from "@/components/ui/label";
import { useState } from "octane";

type IssueDeleteDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  issueNumber: number;
  pending: boolean;
  error: string;
  onConfirm: (confirmNumber: number) => void;
};

const DELETE_BODY =
  "This permanently deletes the issue and its comments. Type the issue number to confirm.";

/** Admin hard-delete with typed confirmNumber (D-ISS-02). */
export function IssueDeleteDialog(props: IssueDeleteDialogProps) @{
  const [confirmValue, setConfirmValue] = useState("");
  const expected = String(props.issueNumber);
  const matches = confirmValue.trim() === expected;

  <AlertDialog
    open={props.open}
    onOpenChange={(open) => {
      if (props.pending) return;
      props.onOpenChange(open);
      if (!open) setConfirmValue("");
    }}
  >
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>Delete issue?</AlertDialogTitle>
        <AlertDialogDescription>{DELETE_BODY as string}</AlertDialogDescription>
      </AlertDialogHeader>
      <div className="flex flex-col gap-2">
        <Label htmlFor="issue-delete-confirm">Issue number</Label>
        <Input
          id="issue-delete-confirm"
          value={confirmValue}
          onInput={(e) => setConfirmValue((e.currentTarget as HTMLInputElement).value)}
          autoComplete="off"
          disabled={props.pending}
          placeholder={expected}
        />
        @if (props.error) {
          <p role="alert" className="text-[13px] text-destructive">
            {props.error as string}
          </p>;
        }
      </div>
      <AlertDialogFooter>
        <AlertDialogCancel disabled={props.pending}>Cancel</AlertDialogCancel>
        <Button
          type="button"
          variant="destructive"
          disabled={props.pending || !matches}
          onClick={() => {
            if (!matches) return;
            props.onConfirm(props.issueNumber);
          }}
        >
          Delete issue
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
