import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[12px] font-semibold leading-none",
  {
    variants: {
      variant: {
        default: "border-border bg-muted text-foreground",
        primary: "border-primary/40 bg-primary/10 text-primary",
        secondary: "border-secondary/40 bg-secondary/10 text-secondary",
        outline: "border-border bg-transparent text-muted-foreground",
        // Issue / PR status pills.
        success: "border-success/40 bg-success/10 text-success",
        destructive: "border-destructive/40 bg-destructive/10 text-destructive",
        // Solid status pills used on issue/PR headers.
        open: "border-transparent bg-success text-success-foreground",
        closed: "border-transparent bg-destructive text-destructive-foreground",
        merged: "border-transparent bg-[#8250df] text-white dark:bg-[#a371f7] dark:text-[#0d1117]",
        draft: "border-transparent bg-muted-foreground text-background",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

type BadgeProps = {
  className?: string;
  children?: unknown;
} & VariantProps<typeof badgeVariants>;

export function Badge({ className, variant, children }: BadgeProps) @{
  <span data-slot="badge" className={cn(badgeVariants({ variant }), className)}>
    {children}
  </span>;
}
