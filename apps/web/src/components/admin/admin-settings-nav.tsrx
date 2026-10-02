import { cn } from "@/lib/utils";

export type AdminSettingsNavActive =
  | "auth"
  | "users"
  | "lfs"
  | "packages"
  | "mcp"
  | "runners"
  | "templates";

type AdminSettingsNavProps = {
  active: AdminSettingsNavActive;
};

const LINKS: {
  id: AdminSettingsNavActive;
  href: string;
  label: string;
}[] = [
  { id: "auth", href: "/admin/auth", label: "Auth" },
  { id: "users", href: "/admin/users", label: "Users" },
  { id: "lfs", href: "/admin/lfs", label: "LFS quotas" },
  { id: "packages", href: "/admin/packages", label: "Package storage" },
  { id: "mcp", href: "/admin/mcp", label: "MCP endpoint" },
  { id: "templates", href: "/admin/templates", label: "Templates" },
  { id: "runners", href: "/admin/runners", label: "Actions runners" },
];

function linkClass(isActive: boolean): string {
  return cn(
    "block rounded-md px-3 py-2 text-[14px] no-underline transition-colors",
    isActive
      ? "bg-muted font-semibold text-foreground"
      : "font-normal text-muted-foreground hover:bg-muted/60 hover:text-foreground",
  );
}

/** Instance admin settings sidebar: Auth | Users | LFS | Packages | MCP | Templates | Actions runners. */
export function AdminSettingsNav({ active }: AdminSettingsNavProps) @{
  <nav className="flex flex-col gap-1" aria-label="Admin settings">
    <p className="mb-2 px-3 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
      Admin
    </p>
    @for (const link of LINKS; key link.id) {
      <a
        href={link.href as string}
        className={linkClass(active === link.id) as string}
        aria-current={active === link.id ? "page" : undefined}
      >
        {link.label as string}
      </a>;
    }
  </nav>;
}
