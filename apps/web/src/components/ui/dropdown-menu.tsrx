import { Menu as MenuPrimitive } from "@octanejs/base-ui/menu";
import type { ComponentProps } from "octane";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

export function DropdownMenu(props: ComponentProps<typeof MenuPrimitive.Root>) @{
  <MenuPrimitive.Root data-slot="dropdown-menu" {...props} />;
}

export function DropdownMenuTrigger(props: ComponentProps<typeof MenuPrimitive.Trigger>) @{
  <MenuPrimitive.Trigger data-slot="dropdown-menu-trigger" {...props} />;
}

export function DropdownMenuContent({
  className,
  align = "end",
  alignOffset = 0,
  side = "bottom",
  sideOffset = 4,
  ...props
}: ComponentProps<typeof MenuPrimitive.Popup> &
  Pick<
    ComponentProps<typeof MenuPrimitive.Positioner>,
    "align" | "alignOffset" | "side" | "sideOffset"
  >) @{
  <MenuPrimitive.Portal keepMounted>
    <MenuPrimitive.Positioner
      className="isolate z-50 outline-none"
      align={align}
      alignOffset={alignOffset}
      side={side}
      sideOffset={sideOffset}
    >
      <MenuPrimitive.Popup
        data-slot="dropdown-menu-content"
        className={cn(
          "z-50 min-w-40 overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-lg outline-none",
          className,
        )}
        {...props}
      />
    </MenuPrimitive.Positioner>
  </MenuPrimitive.Portal>;
}

export function DropdownMenuItem({
  className,
  ...props
}: ComponentProps<typeof MenuPrimitive.Item>) @{
  <MenuPrimitive.Item
    data-slot="dropdown-menu-item"
    className={cn(
      "flex h-11 cursor-default select-none items-center gap-2 rounded-sm px-2 text-[14px] font-normal outline-none",
      "data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground",
      "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
      className,
    )}
    {...props}
  />;
}

/**
 * Menu item rendered as a real anchor — internal navigation goes through
 * <AppLink> (client-side routing + intent preload) so picking a menu item no
 * longer full-reloads the app. `closeOnClick` is forced: LinkItem defaults to
 * false because an anchor click does not necessarily mean "done".
 */
export function DropdownMenuLinkItem({
  className,
  href,
  children,
  ...props
}: ComponentProps<typeof MenuPrimitive.LinkItem>) @{
  <MenuPrimitive.LinkItem
    data-slot="dropdown-menu-item"
    className={cn(
      "flex h-11 cursor-default select-none items-center gap-2 rounded-sm px-2 text-[14px] font-normal no-underline outline-none",
      "data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground",
      "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
      className,
    )}
    {...props}
    closeOnClick
    render={<AppLink href={href as string} />}
  >
    {children}
  </MenuPrimitive.LinkItem>;
}

export function DropdownMenuSeparator({
  className,
  ...props
}: ComponentProps<typeof MenuPrimitive.Separator>) @{
  <MenuPrimitive.Separator
    data-slot="dropdown-menu-separator"
    className={cn("-mx-1 my-1 h-px bg-border", className)}
    {...props}
  />;
}
