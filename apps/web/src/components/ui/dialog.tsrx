import { Dialog as DialogPrimitive } from "@octanejs/base-ui/dialog";
import { X } from "@octanejs/lucide";
import type { ComponentProps } from "octane";
import { Button } from "@/components/ui/button";
import { portalRoot } from "@/lib/portal-root";
import { cn } from "@/lib/utils";

function DialogOverlay({
  className,
  ...props
}: ComponentProps<typeof DialogPrimitive.Backdrop>) @{
  <DialogPrimitive.Backdrop
    data-slot="dialog-overlay"
    className={cn(
      "fixed inset-0 isolate z-50 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]",
      className,
    )}
    {...props}
  />;
}
function DialogPortal(props: ComponentProps<typeof DialogPrimitive.Portal>) @{
  // octane-dom-race-ok: definition site — keepMounted flows through props;
  // DialogContent sets it so every consumer stays mounted.
  <DialogPrimitive.Portal data-slot="dialog-portal" container={portalRoot()} {...props} />;
}
export function Dialog(props: ComponentProps<typeof DialogPrimitive.Root>) @{
  <DialogPrimitive.Root data-slot="dialog" {...props} />;
}

export function DialogClose(props: ComponentProps<typeof DialogPrimitive.Close>) @{
  <DialogPrimitive.Close data-slot="dialog-close" {...props} />;
}

export function DialogContent({
  className,
  children,
  showCloseButton = true,
  ...props
}: ComponentProps<typeof DialogPrimitive.Popup> & {
  showCloseButton?: boolean;
}) @{
  <DialogPortal keepMounted>
    <DialogOverlay />
    <DialogPrimitive.Viewport className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <DialogPrimitive.Popup
        data-slot="dialog-content"
        className={cn(
          "relative grid w-full max-w-[calc(100%-2rem)] gap-4 rounded-md border border-border bg-popover p-4 text-[14px] text-popover-foreground shadow-lg outline-none sm:max-w-md",
          className,
        )}
        {...props}
      >
        {children}
        @if (showCloseButton) {
          <DialogPrimitive.Close
            data-slot="dialog-close"
            render={
              <Button
                variant="ghost"
                size="icon"
                className="absolute top-2 right-2"
                aria-label="Close"
              />
            }
          >
            <X aria-hidden size={16} />
          </DialogPrimitive.Close>;
        }
      </DialogPrimitive.Popup>
    </DialogPrimitive.Viewport>
  </DialogPortal>;
}

export function DialogHeader({ className, ...props }: ComponentProps<"div">) @{
  <div data-slot="dialog-header" className={cn("flex flex-col gap-2", className)} {...props} />;
}

export function DialogFooter({ className, ...props }: ComponentProps<"div">) @{
  <div
    data-slot="dialog-footer"
    className={cn(
      "-mx-4 -mb-4 mt-2 flex flex-col-reverse gap-2 border-t border-border bg-muted/40 p-4 sm:flex-row sm:justify-end",
      className,
    )}
    {...props}
  />;
}

export function DialogTitle({
  className,
  ...props
}: ComponentProps<typeof DialogPrimitive.Title>) @{
  <DialogPrimitive.Title
    data-slot="dialog-title"
    className={cn(
      "font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground",
      className,
    )}
    {...props}
  />;
}

export function DialogDescription({
  className,
  ...props
}: ComponentProps<typeof DialogPrimitive.Description>) @{
  <DialogPrimitive.Description
    data-slot="dialog-description"
    className={cn("text-[14px] text-muted-foreground", className)}
    {...props}
  />;
}
