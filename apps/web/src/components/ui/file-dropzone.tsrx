import { useDropzone, type Accept, type FileRejection } from "@octanejs/dropzone";
import { cn } from "@/lib/utils";

type FileDropzoneProps = {
  id?: string;
  accept?: Accept;
  maxFiles?: number;
  maxSize?: number;
  multiple?: boolean;
  disabled?: boolean;
  className?: string;
  /** Compact control (avatar “Change …” style) vs full drop target. */
  variant?: "dropzone" | "button";
  buttonLabel?: string;
  hint?: string;
  onFiles: (files: File[]) => void | Promise<void>;
  onReject?: (rejections: FileRejection[]) => void;
};

/**
 * Octane file picker built on `@octanejs/dropzone`.
 * Does not upload — callers own FormData / fetch / RPC.
 */
export function FileDropzone(props: FileDropzoneProps) @{
  const variant = props.variant ?? "dropzone";
  const maxFiles = props.maxFiles ?? (props.multiple ? undefined : 1);

  const dropzone = useDropzone({
    accept: props.accept,
    maxFiles,
    maxSize: props.maxSize,
    multiple: props.multiple ?? false,
    disabled: props.disabled,
    noClick: variant === "button",
    noKeyboard: variant === "button",
    onDropAccepted(files) {
      void props.onFiles(files);
    },
    onDropRejected(rejections) {
      props.onReject?.(rejections);
    },
  });

  const busy = dropzone.isDragActive || dropzone.isProcessing;
  const rootProps = dropzone.getRootProps({
    className: cn(
      variant === "dropzone" &&
        "flex min-h-28 cursor-pointer flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-card px-4 py-6 text-center transition-colors",
      variant === "dropzone" && busy && "border-primary bg-primary/5",
      variant === "dropzone" && props.disabled && "cursor-not-allowed opacity-60",
      variant === "button" && "inline-flex",
      props.className,
    ),
  });

  @if (variant === "button") {
    <div {...rootProps}>
      <input id={props.id} {...dropzone.getInputProps()} />
      <button
        type="button"
        className="inline-flex h-11 w-fit items-center justify-center rounded-md px-3 text-[14px] font-medium text-foreground underline-offset-4 hover:underline disabled:pointer-events-none disabled:opacity-50"
        disabled={props.disabled}
        onClick={(event) => {
          event.stopPropagation();
          dropzone.open();
        }}
      >
        {(props.buttonLabel ?? "Choose file") as string}
      </button>
      @if (props.hint) {
        <p className="mt-1 text-[14px] text-muted-foreground">{props.hint as string}</p>;
      }
    </div>;
  } @else {
    <div {...rootProps}>
      <input id={props.id} {...dropzone.getInputProps()} />
      <p className="text-[14px] font-medium text-foreground">
        {
          (dropzone.isDragActive
            ? "Drop files to upload"
            : dropzone.isProcessing
              ? "Checking files…"
              : "Drop files here, or click to browse") as string
        }
      </p>
      @if (props.hint) {
        <p className="text-[13px] text-muted-foreground">{props.hint as string}</p>;
      }
    </div>;
  }
}
