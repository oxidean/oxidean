import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

/** Repo settings sub-pages under `/{owner}/{repo}/settings/{section}`. */
export type RepoSettingsNavActive =
  | "general"
  | "access"
  | "branches"
  | "actions"
  | "storage"
  | "features"
  | "integrations"
  | "danger";

const SECTIONS: { id: RepoSettingsNavActive; label: string }[] = [
  { id: "general", label: "General" },
  { id: "access", label: "Access" },
  { id: "branches", label: "Branches" },
  { id: "actions", label: "Actions" },
  { id: "storage", label: "Storage" },
  { id: "features", label: "Features" },
  { id: "integrations", label: "Integrations" },
  { id: "danger", label: "Danger zone" },
];

function linkClass(isActive: boolean, mobile: boolean): string {
  return cn(
    mobile
      ? "inline-flex shrink-0 items-center whitespace-nowrap rounded-full border px-3 py-1 text-[13px] no-underline transition-colors"
      : "block rounded-md px-3 py-2 text-[14px] no-underline transition-colors",
    isActive
      ? mobile
        ? "border-primary/40 bg-primary/10 font-medium text-foreground"
        : "bg-muted font-semibold text-foreground"
      : mobile
        ? "border-border text-muted-foreground hover:text-foreground"
        : "font-normal text-muted-foreground hover:bg-muted/60 hover:text-foreground",
  );
}

type RepoSettingsNavProps = {
  owner: string;
  repo: string;
  active: RepoSettingsNavActive;
  mobile?: boolean;
};

/**
 * Repo settings sub-page nav — real links to `/{owner}/{repo}/settings/{section}`.
 * Sidebar on desktop, horizontally scrollable chips on mobile.
 */
export function RepoSettingsNav(props: RepoSettingsNavProps) @{
  const links = SECTIONS.map((s) => ({
    ...s,
    href: `/${props.owner}/${props.repo}/settings/${s.id}`,
  }));

  @if (props.mobile) {
    <nav
      aria-label="Repository settings"
      className="flex gap-2 overflow-x-auto pb-1 [scrollbar-width:thin]"
    >
      @for (const link of links; key link.id) {
        <AppLink
          href={link.href as string}
          className={linkClass(props.active === link.id, true) as string}
          aria-current={props.active === link.id ? "page" : undefined}
        >
          {link.label as string}
        </AppLink>;
      }
    </nav>;
  } @else {
    <nav
      aria-label="Repository settings"
      className="flex max-h-[calc(100vh-9rem)] flex-col gap-1 overflow-y-auto"
    >
      <p className="mb-2 px-3 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
        Repository
      </p>
      @for (const link of links; key link.id) {
        <AppLink
          href={link.href as string}
          className={linkClass(props.active === link.id, false) as string}
          aria-current={props.active === link.id ? "page" : undefined}
        >
          {link.label as string}
        </AppLink>;
      }
    </nav>;
  }
}
