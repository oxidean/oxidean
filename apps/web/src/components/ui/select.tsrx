import { Select } from "@octanejs/base-ui/select";
import type { ComponentProps } from "octane";
import { portalRoot } from "@/lib/portal-root";
import { cn } from "@/lib/utils";

export const SelectRoot = Select.Root;
export const SelectValue = Select.Value;

export function SelectPortal(props: ComponentProps<typeof Select.Portal>) @{
  // octane-dom-race-ok: definition site — Select.Portal has no keepMounted
  // escape hatch; container pins portals to the persisted host.
  <Select.Portal container={portalRoot()} {...props} />;
}
export const SelectList = Select.List;
export const SelectGroup = Select.Group;
export const SelectItemText = Select.ItemText;
export const SelectIcon = Select.Icon;
export const SelectItemIndicator = Select.ItemIndicator;

export function SelectPositioner({
  className,
  ...props
}: ComponentProps<typeof Select.Positioner>) @{
  <Select.Positioner
    data-slot="select-positioner"
    className={cn("isolate z-50 outline-none", className)}
    {...props}
  />;
}

export function SelectTrigger({
  className,
  ...props
}: ComponentProps<typeof Select.Trigger>) @{
  <Select.Trigger
    data-slot="select-trigger"
    className={cn(
      "inline-flex h-11 items-center justify-between gap-2 rounded-md border border-input bg-card px-3 text-[14px] font-normal text-foreground outline-none transition-colors duration-150 hover:border-foreground/25 focus-visible:ring-2 focus-visible:ring-ring",
      className,
    )}
    {...props}
  />;
}

export function SelectPopup({
  className,
  ...props
}: ComponentProps<typeof Select.Popup>) @{
  <Select.Popup
    data-slot="select-popup"
    className={cn(
      "z-50 min-w-[10rem] rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-lg outline-none",
      className,
    )}
    {...props}
  />;
}

export function SelectGroupLabel({
  className,
  ...props
}: ComponentProps<typeof Select.GroupLabel>) @{
  <Select.GroupLabel
    data-slot="select-group-label"
    className={cn(
      "px-2 pb-1 pt-2 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground",
      className,
    )}
    {...props}
  />;
}

export function SelectItem({ className, ...props }: ComponentProps<typeof Select.Item>) @{
  <Select.Item
    data-slot="select-item"
    className={cn(
      "flex h-11 cursor-default select-none items-center gap-2 rounded-sm px-2 text-[14px] font-normal outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[selected]:text-primary",
      className,
    )}
    {...props}
  />;
}
