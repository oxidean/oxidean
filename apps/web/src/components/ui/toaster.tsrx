import { Toast } from "@octanejs/base-ui/toast";
import { X } from "@octanejs/lucide";
import { cva, type VariantProps } from "class-variance-authority";
import { portalRoot } from "@/lib/portal-root";
import { appToastManager, type ToastVariant } from "@/lib/toast";
import { cn } from "@/lib/utils";

/** Toast card chrome — mirrors verify callout sizing/typography for form feedback. */
export const toastVariants = cva(
  [
    "oct-toast group relative w-full origin-bottom rounded-lg border bg-card/95 p-4 text-foreground shadow-lg backdrop-blur-sm",
    "outline-none transition-[transform,opacity] duration-200 ease-out",
    "data-[starting-style]:translate-y-2 data-[starting-style]:opacity-0",
    "data-[ending-style]:translate-y-1 data-[ending-style]:opacity-0",
  ].join(" "),
  {
    variants: {
      variant: {
        success: "border-primary/40",
        info: "border-border",
        warning: "border-secondary/45",
        error: "border-destructive/50",
      },
    },
    defaultVariants: { variant: "info" },
  },
);

export const toastAccentVariants = cva("mt-0.5 w-1 shrink-0 self-stretch rounded-full", {
  variants: {
    variant: {
      success: "bg-primary",
      info: "bg-muted-foreground/40",
      warning: "bg-secondary",
      error: "bg-destructive",
    },
  },
  defaultVariants: { variant: "info" },
});

export type ToastVariantProps = VariantProps<typeof toastVariants>;

function toastVariantFromType(type: string | undefined): ToastVariant {
  if (type === "success" || type === "error" || type === "warning" || type === "info") {
    return type;
  }
  return "info";
}

/**
 * Oxidean toast chrome — bottom-right stack (same corner as verify callout).
 * CVA variants: success / info / warning / error.
 */
function ToastList() @{
  const { toasts } = Toast.useToastManager();

  // octane-dom-race-ok: app-root singleton — mounts once at boot and never
  // unmounts; toast items inside change but the portal node itself is stable.
  // container: persisted host, so view-transition swaps don't detach it.
  <Toast.Portal container={portalRoot()}>
    <Toast.Viewport
      className={cn(
        "oct-toast-viewport fixed right-4 bottom-4 z-[100] flex w-[min(100%-2rem,22rem)] flex-col-reverse gap-2 outline-none",
      )}
    >
      @for (const toast of toasts; key toast.id) {
        <Toast.Root
          toast={toast}
          className={toastVariants({
            variant: toastVariantFromType(
              typeof toast.type === "string" ? toast.type : undefined,
            ),
          })}
        >
          <Toast.Content className="flex gap-3">
            <span
              aria-hidden="true"
              className={toastAccentVariants({
                variant: toastVariantFromType(
                  typeof toast.type === "string" ? toast.type : undefined,
                ),
              })}
            />
            <div className="relative min-w-0 flex-1 pr-7">
              <Toast.Title className="m-0 text-[14px] font-medium leading-snug text-foreground" />
              <Toast.Description className="m-0 mt-1 text-[13px] font-normal leading-snug break-words text-muted-foreground" />
              <Toast.Close
                className={cn(
                  "absolute -top-1.5 -right-1.5 inline-flex size-8 items-center justify-center rounded-md text-muted-foreground",
                  "hover:bg-muted hover:text-foreground",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                )}
                aria-label="Dismiss"
              >
                <X aria-hidden size={16} />
              </Toast.Close>
            </div>
          </Toast.Content>
        </Toast.Root>;
      }
    </Toast.Viewport>
  </Toast.Portal>;
}

/** Root provider + viewport. Wrap the app once in `__root`. */
export function AppToaster({ children }: { children?: unknown }) @{
  <Toast.Provider toastManager={appToastManager} timeout={5000} limit={3}>
    {children}
    <ToastList />
  </Toast.Provider>;
}
