import { createToastManager } from "@octanejs/base-ui/toast";

/** Module-scope manager so any client code can enqueue toasts. */
export const appToastManager = createToastManager();

/** Matches {@link toastVariants} in `components/ui/toaster`. */
export type ToastVariant = "success" | "info" | "warning" | "error";

type ToastOptions = {
  title: string;
  description?: string;
  variant?: ToastVariant;
  timeout?: number;
};

const TIMEOUTS: Record<ToastVariant, number> = {
  success: 4000,
  info: 4500,
  warning: 5500,
  error: 6000,
};

/** Enqueue a toast. Prefer {@link toastSuccess} / {@link toastError} / etc. for call sites. */
function toast(options: ToastOptions): string {
  const variant = options.variant ?? "info";
  return appToastManager.add({
    title: options.title,
    description: options.description,
    type: variant,
    timeout: options.timeout ?? TIMEOUTS[variant],
    priority: variant === "error" || variant === "warning" ? "high" : "low",
  });
}

export function toastSuccess(title: string, description?: string): string {
  return toast({ title, description, variant: "success" });
}

export function toastWarning(title: string, description?: string): string {
  return toast({ title, description, variant: "warning" });
}

export function toastError(title: string, description?: string): string {
  return toast({ title, description, variant: "error" });
}
