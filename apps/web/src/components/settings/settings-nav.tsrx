import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

export type SettingsNavActive = "general" | "profile" | "tokens" | "ssh-keys";

type SettingsNavProps = {
  active: SettingsNavActive;
};

const LINKS: { id: SettingsNavActive; href: string; label: string }[] = [
  { id: "general", href: "/settings/general", label: "General" },
  { id: "profile", href: "/settings/profile", label: "Account" },
  {
    id: "tokens",
    href: "/settings/tokens",
    label: "Personal access tokens",
  },
  { id: "ssh-keys", href: "/settings/ssh-keys", label: "SSH and GPG keys" },
];

function linkClass(isActive: boolean): string {
  return cn(
    "block rounded-md px-3 py-2 text-[14px] no-underline transition-colors",
    isActive
      ? "bg-muted font-semibold text-foreground"
      : "font-normal text-muted-foreground hover:bg-muted/60 hover:text-foreground",
  );
}

/** Account settings sidebar: General | Account | Personal access tokens | SSH and GPG keys. */
export function SettingsNav({ active }: SettingsNavProps) @{
  <nav className="flex flex-col gap-1" aria-label="Account settings">
    <p className="mb-2 px-3 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
      Settings
    </p>
    @for (const link of LINKS; key link.id) {
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
