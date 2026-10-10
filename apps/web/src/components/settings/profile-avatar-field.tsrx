import { useDropzone } from "@octanejs/dropzone";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AvatarPreview } from "@/components/avatar-preview";
import { AvatarCropDialog } from "@/components/settings/avatar-crop-dialog";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

const ACCEPT = {
  "image/jpeg": [".jpg", ".jpeg"],
  "image/png": [".png"],
  "image/webp": [".webp"],
} as const;

const MAX_SIZE = 2 * 1024 * 1024;
const AVATAR_PX = 96;

type ProfileAvatarFieldProps = {
  user: Pick<UserPublic, "display_name" | "username" | "avatar_url">;
  src?: string | null;
  disabled?: boolean;
  uploadPending?: boolean;
  removePending?: boolean;
  onCroppedFile: (file: File) => void | Promise<void>;
  onRemove: () => void | Promise<void>;
  onReject: () => void;
};

/**
 * Profile picture control — squircle drop target opens a crop dialog; remove clears avatar.
 */
export function ProfileAvatarField(props: ProfileAvatarFieldProps) @{
  const [cropUrl, setCropUrl] = useState<string | null>(null);
  const [cropOpen, setCropOpen] = useState(false);

  useEffect(() => {
    return () => {
      if (cropUrl) URL.revokeObjectURL(cropUrl);
    };
  }, [cropUrl]);

  const dropzone = useDropzone({
    accept: ACCEPT,
    maxFiles: 1,
    maxSize: MAX_SIZE,
    multiple: false,
    disabled: props.disabled || props.uploadPending || props.removePending,
    onDropAccepted(files) {
      const file = files[0];
      if (!file) return;
      if (cropUrl) URL.revokeObjectURL(cropUrl);
      const url = URL.createObjectURL(file);
      setCropUrl(url);
      // Let dropzone finish its own DOM updates before mounting the overlay.
      requestAnimationFrame(() => setCropOpen(true));
    },
    onDropRejected() {
      props.onReject();
    },
  });

  const busy = dropzone.isDragActive || dropzone.isProcessing;
  const hasPicture = !!(props.src ?? props.user.avatar_url);
  const overlayLabel = dropzone.isDragActive ? "Drop" : dropzone.isProcessing ? "…" : "Change";
  const rootProps = dropzone.getRootProps({
    className: cn(
      "group relative inline-flex size-24 shrink-0 cursor-pointer outline-none",
      "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
      (props.disabled || props.uploadPending || props.removePending) &&
        "cursor-not-allowed opacity-60",
    ),
  });
  // Octane prefers native onInput; react-dropzone-style getInputProps wires onChange + ref.
  // Spread alone can drop `ref` under Rivet — bind ref/onInput explicitly so open() + picks work.
  const inputProps = dropzone.getInputProps({
    id: "profile-avatar",
    "aria-label": "Upload profile picture",
  });
  const { ref: _dropzoneInputRef, onChange: onFileChange, ...inputRest } = inputProps;

  function closeCrop() {
    setCropOpen(false);
    if (cropUrl) {
      URL.revokeObjectURL(cropUrl);
      setCropUrl(null);
    }
  }

  <div>
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:gap-5">
      <div {...rootProps}>
        <input
          {...inputRest}
          ref={(node: HTMLInputElement | null) => {
            // Object refs from useDropzone must be assigned explicitly (Rivet spread can drop `ref`).
            const r = _dropzoneInputRef as { current: HTMLInputElement | null } | null | undefined;
            if (r && typeof r === "object") r.current = node;
          }}
          onChange={(e) => {
            if (typeof onFileChange === "function") onFileChange(e as never);
          }}
          onInput={(e) => {
            if (typeof onFileChange === "function") onFileChange(e as never);
          }}
        />
        <AvatarPreview
          user={props.user}
          src={props.src}
          size={AVATAR_PX}
          className={cn("size-24 transition-[filter,opacity]", busy && "opacity-70")}
        />
        <span
          aria-hidden
          className={cn(
            "oxidean-squircle pointer-events-none absolute inset-0 flex items-center justify-center",
            "bg-foreground/70 text-[13px] font-medium text-background opacity-0 transition-opacity",
            "group-hover:opacity-100 group-focus-visible:opacity-100",
            busy && "opacity-100",
          )}
        >
          {overlayLabel as string}
        </span>
      </div>

      <div className="min-w-0 flex-1">
        <Label>Profile picture</Label>
        <p className="mt-1 text-[14px] leading-[1.4] text-muted-foreground">
          Click or drop an image to crop. JPEG, PNG, or WebP · max 2 MB.
        </p>
        <div className="mt-3 flex min-h-11 flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            disabled={props.disabled || props.uploadPending || props.removePending}
            onClick={() => dropzone.open()}
          >
            Upload new picture
          </Button>
          <Button
            type="button"
            variant="ghost"
            className={cn(
              "text-destructive hover:bg-destructive/10 hover:text-destructive",
              !hasPicture && "invisible",
            )}
            aria-hidden={!hasPicture}
            tabIndex={hasPicture ? 0 : -1}
            disabled={!hasPicture || props.disabled || props.uploadPending || props.removePending}
            onClick={() => void props.onRemove()}
          >
            {(props.removePending ? "Removing…" : "Remove picture") as string}
          </Button>
        </div>
      </div>
    </div>

    <AvatarCropDialog
      open={cropOpen}
      imageUrl={cropUrl}
      pending={props.uploadPending}
      onOpenChange={(open) => {
        if (!open) closeCrop();
        else setCropOpen(true);
      }}
      onConfirm={async (file) => {
        await props.onCroppedFile(file);
        closeCrop();
      }}
    />
  </div>;
}
