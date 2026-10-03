import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

export type OrgSettingsNavActive = "general" | "members" | "labels";

type OrgSettingsNavProps = {
  slug: string;
  active: OrgSettingsNavActive;
};

function linkClass(isActive: boolean): string {
  return cn(
    "block rounded-md px-3 py-2 text-[14px] no-underline transition-colors",
    isActive
      ? "bg-muted font-semibold text-foreground"
      : "font-normal text-muted-foreground hover:bg-muted/60 hover:text-foreground",
  );
}

/** Org settings sidebar: General | Members | Labels. */
export function OrgSettingsNav({ slug, active }: OrgSettingsNavProps) @{
  const links: { id: OrgSettingsNavActive; href: string; label: string }[] = [
    { id: "general", href: `/${slug}/settings`, label: "General" },
    { id: "members", href: `/${slug}/settings/members`, label: "Members" },
    { id: "labels", href: `/${slug}/settings/labels`, label: "Labels" },
  ];

  <nav className="flex flex-col gap-1" aria-label="Organization settings">
    <p className="mb-2 px-3 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
      Organization
    </p>
    @for (const link of links; key link.id) {
      <AppLink
        href={link.href as string}
        className={linkClass(active === link.id) as string}
        aria-current={active === link.id ? "page" : undefined}
      >
        {link.label as string}
      </AppLink>;
    }
  </nav>;
}
