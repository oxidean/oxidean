import { Menu as MenuPrimitive } from "@octanejs/base-ui/menu";
import { createContext, useContext, useState, type ComponentProps } from "octane";
import { portalRoot } from "@/lib/portal-root";
import { cn } from "@/lib/utils";

/**
 * True once this menu has been opened at least once. `keepMounted` keeps the
 * Portal node itself stable (the DOM-race fix), but the floating internals
 * inside — positioner, focus manager, composite list — still mounted eagerly
 * at page load. Gating them on first open skips that work on pages whose
 * menus are never used, while open/close cost stays identical afterwards.
 */
const DropdownMenuOpened = createContext(false);

export function DropdownMenu(props: ComponentProps<typeof MenuPrimitive.Root>) @{
  const [hasOpened, setHasOpened] = useState(Boolean(props.open ?? props.defaultOpen));
  const onOpenChange = props.onOpenChange;

  <DropdownMenuOpened value={hasOpened}>
    <MenuPrimitive.Root
      data-slot="dropdown-menu"
      {...props}
      onOpenChange={(open, details) => {
        if (open) setHasOpened(true);
        onOpenChange?.(open, details);
      }}
    />
  </DropdownMenuOpened>;
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
  const hasOpened = useContext(DropdownMenuOpened);

  <MenuPrimitive.Portal keepMounted container={portalRoot()}>
    @if (hasOpened) {
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
      </MenuPrimitive.Positioner>;
    }
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
 * Menu item that navigates — renders a real `<a>` via Base UI `LinkItem` so
 * clicks get anchor semantics (ClientRouter morph nav, early-nav bridging,
 * middle-click open-in-new-tab) instead of `window.location.assign` reloads.
 * `closeOnClick` defaults true: the header island persists across morphs, so
 * the popup must close itself on navigation.
 */
export function DropdownMenuLinkItem({
  className,
  closeOnClick = true,
  ...props
}: ComponentProps<typeof MenuPrimitive.LinkItem>) @{
  <MenuPrimitive.LinkItem
    data-slot="dropdown-menu-item"
    closeOnClick={closeOnClick}
    className={cn(
      "flex h-11 cursor-default select-none items-center gap-2 rounded-sm px-2 text-[14px] font-normal text-current no-underline outline-none",
      "data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground",
      "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
      className,
    )}
    {...props}
  />;
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
