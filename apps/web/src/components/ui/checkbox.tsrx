import { Checkbox as CheckboxPrimitive } from "@octanejs/base-ui/checkbox";
import { Check } from "@octanejs/lucide";
import type { ComponentProps } from "octane";
import { cn } from "@/lib/utils";

export function Checkbox({
  className,
  ...props
}: ComponentProps<typeof CheckboxPrimitive.Root>) @{
  <CheckboxPrimitive.Root
    data-slot="checkbox"
    className={cn(
      "peer relative inline-flex size-5 shrink-0 items-center justify-center rounded-[4px] border border-input bg-card text-primary-foreground outline-none transition-colors",
      "after:absolute after:-inset-y-2 after:-inset-x-3",
      "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
      "data-[checked]:border-primary data-[checked]:bg-primary",
      "disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    {...props}
  >
    <CheckboxPrimitive.Indicator
      keepMounted
      data-slot="checkbox-indicator"
      className="grid place-content-center text-current data-[unchecked]:hidden"
    >
      <Check aria-hidden size={14} />
    </CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>;
}
