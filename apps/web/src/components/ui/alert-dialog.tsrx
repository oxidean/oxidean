import { AlertDialog as AlertDialogPrimitive } from "@octanejs/base-ui/alert-dialog";
import type { ComponentProps } from "octane";
import { Button, buttonVariants } from "@/components/ui/button";
import { portalRoot } from "@/lib/portal-root";
import { cn } from "@/lib/utils";

export function AlertDialog(props: ComponentProps<typeof AlertDialogPrimitive.Root>) @{
  <AlertDialogPrimitive.Root data-slot="alert-dialog" {...props} />;
}

export function AlertDialogTrigger(
  props: ComponentProps<typeof AlertDialogPrimitive.Trigger>,
) @{
  <AlertDialogPrimitive.Trigger data-slot="alert-dialog-trigger" {...props} />;
}

export function AlertDialogPortal(
  props: ComponentProps<typeof AlertDialogPrimitive.Portal>,
) @{
  // octane-dom-race-ok: definition site — keepMounted flows through props;
  // AlertDialogContent sets it so every consumer stays mounted.
  <AlertDialogPrimitive.Portal
    data-slot="alert-dialog-portal"
    container={portalRoot()}
    {...props}
  />;
}

export function AlertDialogOverlay({
  className,
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Backdrop>) @{
  <AlertDialogPrimitive.Backdrop
    data-slot="alert-dialog-overlay"
    className={cn(
      "fixed inset-0 isolate z-50 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]",
      className,
    )}
    {...props}
  />;
}

export function AlertDialogContent({
  className,
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Popup>) @{
  <AlertDialogPortal keepMounted>
    <AlertDialogOverlay />
    <AlertDialogPrimitive.Viewport className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <AlertDialogPrimitive.Popup
        data-slot="alert-dialog-content"
        className={cn(
          "relative grid w-full max-w-[calc(100%-2rem)] gap-4 rounded-md border border-border bg-popover p-4 text-[14px] text-popover-foreground shadow-lg outline-none sm:max-w-md",
          className,
        )}
        {...props}
      />
    </AlertDialogPrimitive.Viewport>
  </AlertDialogPortal>;
}

export function AlertDialogHeader({ className, ...props }: ComponentProps<"div">) @{
  <div
    data-slot="alert-dialog-header"
    className={cn("flex flex-col gap-2 text-left", className)}
    {...props}
  />;
}

export function AlertDialogFooter({ className, ...props }: ComponentProps<"div">) @{
  <div
    data-slot="alert-dialog-footer"
    className={cn(
      "-mx-4 -mb-4 mt-2 flex flex-col-reverse gap-2 border-t border-border bg-muted/40 p-4 sm:flex-row sm:justify-end",
      className,
    )}
    {...props}
  />;
}

export function AlertDialogTitle({
  className,
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Title>) @{
  <AlertDialogPrimitive.Title
    data-slot="alert-dialog-title"
    className={cn(
      "font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground",
      className,
    )}
    {...props}
  />;
}

export function AlertDialogDescription({
  className,
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Description>) @{
  <AlertDialogPrimitive.Description
    data-slot="alert-dialog-description"
    className={cn("text-[14px] text-muted-foreground", className)}
    {...props}
  />;
}

export function AlertDialogAction({
  className,
  variant = "default",
  size = "default",
  ...props
}: ComponentProps<"button"> & {
  variant?: "default" | "secondary" | "ghost" | "destructive";
  size?: "default" | "icon";
}) @{
  <Button
    data-slot="alert-dialog-action"
    className={cn(className)}
    variant={variant}
    size={size}
    type="button"
    {...props}
  />;
}

export function AlertDialogCancel({
  className,
  ...props
}: ComponentProps<typeof AlertDialogPrimitive.Close>) @{
  <AlertDialogPrimitive.Close
    data-slot="alert-dialog-cancel"
    className={cn(buttonVariants({ variant: "secondary" }), className)}
    {...props}
  />;
}
