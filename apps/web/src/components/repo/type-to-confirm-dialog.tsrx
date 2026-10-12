import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

type TypeToConfirmDialogProps = {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  confirmValue: string;
  expectedName: string;
  onConfirmValueChange: (value: string) => void;
  pending?: boolean;
  error?: string;
  confirmButtonLabel: string;
  pendingButtonLabel: string;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  inputId: string;
  testId?: string;
};

/**
 * Type-to-confirm modal — in-tree fixed overlay (no AlertDialog portal).
 * Portals remounting against Octane DOM updates caused insertBefore crashes
 * when transfer/delete confirm submitted and error/pending UI updated.
 */
export function TypeToConfirmDialog(props: TypeToConfirmDialogProps) @{
  const pending = props.pending === true;
  const error = props.error ?? "";
  const canConfirm = !pending && props.confirmValue.trim() === props.expectedName;

  function requestClose() {
    if (pending) return;
    props.onOpenChange(false);
  }

  @if (props.open) {
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      role="presentation"
      data-testid={props.testId}
    >
      <button
        type="button"
        aria-label="Dismiss"
        className="absolute inset-0 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]"
        disabled={pending}
        onClick={() => requestClose()}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${props.inputId}-title`}
        className="relative z-10 grid w-full max-w-[calc(100%-2rem)] gap-4 rounded-md border border-border bg-popover p-4 text-[14px] text-popover-foreground shadow-lg outline-none sm:max-w-md"
      >
        <div className="flex flex-col gap-2">
          <h2
            id={`${props.inputId}-title`}
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground"
          >
            {props.title as string}
          </h2>
          <p className="text-[14px] text-muted-foreground">{props.description as string}</p>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor={props.inputId}>{props.confirmLabel as string}</Label>
          <Input
            id={props.inputId}
            value={props.confirmValue}
            onInput={(e) => props.onConfirmValueChange((e.currentTarget as HTMLInputElement).value)}
            autoComplete="off"
            disabled={pending}
            placeholder={props.expectedName as string}
          />
        </div>
        <p
          role="alert"
          className={
            error ? "text-[14px] text-destructive" : "invisible h-0 overflow-hidden text-[14px]"
          }
        >
          {(error || "\u00a0") as string}
        </p>
        <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button
            type="button"
            variant="secondary"
            disabled={pending}
            onClick={() => requestClose()}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={!canConfirm}
            onClick={() => props.onConfirm()}
          >
            {(pending ? props.pendingButtonLabel : props.confirmButtonLabel) as string}
          </Button>
        </div>
      </div>
    </div>;
  }
}
