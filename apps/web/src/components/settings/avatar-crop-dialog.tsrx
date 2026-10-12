import { useEffect, useRef, useState } from "octane";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { clampOffset, cropImageToBlob, displayedSize, type CropFrame } from "@/lib/avatar-crop";
import { cn } from "@/lib/utils";

const VIEWPORT = 280;

type AvatarCropDialogProps = {
  open: boolean;
  imageUrl: string | null;
  pending?: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (file: File) => void | Promise<void>;
};

/**
 * Square crop modal — in-tree fixed overlay (no Dialog portal).
 * Portals remounting against Octane's direct DOM updates caused insertBefore
 * crashes when opening from the profile dropzone.
 */
export function AvatarCropDialog(props: AvatarCropDialogProps) @{
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [natural, setNatural] = useState({ w: 0, h: 0 });
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const dragOrigin = useRef<{ x: number; y: number; ox: number; oy: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!props.open) return;
    setZoom(1);
    setOffset({ x: 0, y: 0 });
    setError("");
    setBusy(false);
    setNatural({ w: 0, h: 0 });
  }, [props.open, props.imageUrl]);

  function frame(): CropFrame {
    return {
      imageWidth: natural.w,
      imageHeight: natural.h,
      viewport: VIEWPORT,
      zoom,
      offsetX: offset.x,
      offsetY: offset.y,
    };
  }

  function onImageLoad(el: HTMLImageElement) {
    const w = el.naturalWidth;
    const h = el.naturalHeight;
    setNatural({ w, h });
    const f0: CropFrame = {
      imageWidth: w,
      imageHeight: h,
      viewport: VIEWPORT,
      zoom: 1,
      offsetX: 0,
      offsetY: 0,
    };
    const { width, height } = displayedSize(f0);
    const centered = clampOffset({
      ...f0,
      offsetX: (VIEWPORT - width) / 2,
      offsetY: (VIEWPORT - height) / 2,
    });
    setOffset({ x: centered.offsetX, y: centered.offsetY });
  }

  function onPointerDown(e: PointerEvent) {
    if (props.pending || busy) return;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    setDragging(true);
    dragOrigin.current = {
      x: e.clientX,
      y: e.clientY,
      ox: offset.x,
      oy: offset.y,
    };
  }

  function onPointerMove(e: PointerEvent) {
    const origin = dragOrigin.current;
    if (!origin || !dragging) return;
    const next = clampOffset({
      ...frame(),
      offsetX: origin.ox + (e.clientX - origin.x),
      offsetY: origin.oy + (e.clientY - origin.y),
    });
    setOffset({ x: next.offsetX, y: next.offsetY });
  }

  function onPointerUp(e: PointerEvent) {
    const target = e.currentTarget as HTMLElement;
    if (target.hasPointerCapture(e.pointerId)) {
      target.releasePointerCapture(e.pointerId);
    }
    setDragging(false);
    dragOrigin.current = null;
  }

  function onZoomInput(value: number) {
    const nextZoom = Math.min(3, Math.max(1, value));
    const clamped = clampOffset({
      ...frame(),
      zoom: nextZoom,
      offsetX: offset.x,
      offsetY: offset.y,
    });
    setZoom(nextZoom);
    setOffset({ x: clamped.offsetX, y: clamped.offsetY });
  }

  async function confirmCrop() {
    const img = imgRef.current;
    if (!img || natural.w === 0) return;
    setBusy(true);
    setError("");
    try {
      const blob = await cropImageToBlob(img, frame(), 512);
      const type = blob.type || "image/webp";
      const ext = type.includes("jpeg") ? "jpg" : "webp";
      const file = new File([blob], `avatar.${ext}`, { type });
      await props.onConfirm(file);
    } catch {
      setError("Couldn't crop that image. Try another file.");
      setBusy(false);
    }
  }

  function requestClose() {
    if (props.pending || busy) return;
    props.onOpenChange(false);
  }

  const disp = natural.w > 0 ? displayedSize(frame()) : { width: VIEWPORT, height: VIEWPORT };
  const saving = props.pending === true || busy;
  const canSave = !saving && natural.w > 0;
  const show = props.open && !!props.imageUrl;

  @if (show) {
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
      <button
        type="button"
        aria-label="Dismiss crop"
        className="absolute inset-0 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]"
        disabled={saving}
        onClick={() => requestClose()}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="avatar-crop-title"
        className="relative z-10 grid w-full max-w-[calc(100%-2rem)] gap-4 rounded-md border border-border bg-popover p-4 text-[14px] text-popover-foreground shadow-lg outline-none sm:max-w-md"
      >
        <div className="flex flex-col gap-2">
          <h2
            id="avatar-crop-title"
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground"
          >
            Crop profile picture
          </h2>
          <p className="text-[14px] text-muted-foreground">
            Drag to reposition. Use zoom to fill the square, then save.
          </p>
        </div>

        <div
          className={cn(
            "relative mx-auto overflow-hidden rounded-md border border-border bg-muted",
            dragging ? "cursor-grabbing" : "cursor-grab",
          )}
          style={{ width: `${VIEWPORT}px`, height: `${VIEWPORT}px` }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          <img
            ref={imgRef}
            src={props.imageUrl as string}
            alt=""
            draggable={false}
            className="pointer-events-none absolute max-w-none select-none"
            style={{
              width: `${disp.width}px` as string,
              height: `${disp.height}px` as string,
              transform: `translate(${offset.x}px, ${offset.y}px)` as string,
            }}
            onLoad={(e) => onImageLoad(e.currentTarget as HTMLImageElement)}
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="avatar-crop-zoom">Zoom</Label>
          <input
            id="avatar-crop-zoom"
            type="range"
            min={1}
            max={3}
            step={0.01}
            value={zoom}
            disabled={!canSave}
            className="h-11 w-full accent-primary"
            onInput={(e) => onZoomInput(Number((e.currentTarget as HTMLInputElement).value))}
          />
        </div>

        @if (error) {
          <p role="alert" className="text-[14px] text-destructive">
            {error as string}
          </p>;
        }

        <div className="-mx-4 -mb-4 mt-2 flex flex-col-reverse gap-2 border-t border-border bg-muted/40 p-4 sm:flex-row sm:justify-end">
          <Button
            type="button"
            variant="secondary"
            disabled={saving}
            onClick={() => requestClose()}
          >
            Cancel
          </Button>
          <Button type="button" disabled={!canSave} onClick={() => void confirmCrop()}>
            {(saving ? "Saving…" : "Save picture") as string}
          </Button>
        </div>
      </div>
    </div>;
  }
}
