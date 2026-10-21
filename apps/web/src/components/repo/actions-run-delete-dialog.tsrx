import { useMutation } from "@octanejs/tanstack-query";
import { actionsDeleteRunMutationOptions } from "@oxidean/api-client";
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
import { apiClient } from "@/lib/api-client";

const DELETE_ERROR = "Couldn't delete the workflow run. Check your connection and try again.";

type ActionsRunDeleteDialogProps = {
  owner: string;
  name: string;
  runId: string;
  runTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDeleted: () => void;
};

/** GitHub "Delete workflow run" — removes the run, its jobs, logs, artifacts. */
export function ActionsRunDeleteDialog({
  owner,
  name,
  runId,
  runTitle,
  open,
  onOpenChange,
  onDeleted,
}: ActionsRunDeleteDialogProps) @{
  const deleteM = useMutation(actionsDeleteRunMutationOptions(apiClient));

  function confirm() {
    if (deleteM.isPending) return;
    deleteM.mutate({ owner, name, run_id: runId }, { onSuccess: () => onDeleted() });
  }

  <AlertDialog
    open={open}
    onOpenChange={(next) => {
      if (deleteM.isPending) return;
      if (!next) deleteM.reset();
      onOpenChange(next);
    }}
  >
    <AlertDialogContent className="max-h-[min(90vh,32rem)] overflow-y-auto">
      <AlertDialogHeader>
        <AlertDialogTitle>Delete workflow run?</AlertDialogTitle>
        <AlertDialogDescription className="break-words whitespace-normal">
          {
            `Permanently deletes “${runTitle}” — its jobs, logs, and artifacts. This can’t be undone.` as string
          }
        </AlertDialogDescription>
      </AlertDialogHeader>
      @if (deleteM.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          {DELETE_ERROR as string}
        </p>;
      }
      <AlertDialogFooter>
        <AlertDialogCancel disabled={deleteM.isPending}>Keep run</AlertDialogCancel>
        <Button type="button" variant="destructive" disabled={deleteM.isPending} onClick={confirm}>
          {(deleteM.isPending ? "Deleting…" : "Delete workflow run") as string}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>;
}
