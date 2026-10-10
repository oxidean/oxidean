import { cn } from "@/lib/utils";
import {
  AdminSettingsNav,
  type AdminSettingsNavActive,
} from "@/components/admin/admin-settings-nav";
import { OrgSettingsNav, type OrgSettingsNavActive } from "@/components/org/org-settings-nav";
import { SettingsNav, type SettingsNavActive } from "@/components/settings/settings-nav";

/** Shared fixed canvas for account + admin settings (prevents width shift between pages). */
const SETTINGS_SHELL_MAX = "max-w-5xl";

type SettingsShellProps = {
  children?: unknown;
  className?: string;
  "data-testid"?: string;
  /** Loading / error / forbidden — same max-width, no sidebar. */
  bare?: boolean;
};

/**
 * Fixed-width settings canvas. Pair with {@link AccountSettingsLayout} /
 * {@link AdminSettingsLayout} / {@link OrgSettingsLayout} for the sidebar grid.
 */
export function SettingsShell(props: SettingsShellProps) @{
  <div
    className={cn("mx-auto w-full px-4 py-10 sm:px-6", SETTINGS_SHELL_MAX, props.className)}
    data-testid={props["data-testid"]}
  >
    {props.children}
  </div>;
}

type AccountSettingsLayoutProps = {
  active: SettingsNavActive;
  children?: unknown;
  className?: string;
  "data-testid"?: string;
};

/** Account `/settings/*` shell: fixed width + sidebar nav. */
export function AccountSettingsLayout(props: AccountSettingsLayoutProps) @{
  <SettingsShell className={props.className} data-testid={props["data-testid"]}>
    <div className="grid gap-8 md:grid-cols-[14rem_minmax(0,1fr)] md:gap-10">
      <aside className="md:sticky md:top-20 md:self-start">
        <SettingsNav active={props.active} />
      </aside>
      <div className="min-w-0">{props.children}</div>
    </div>
  </SettingsShell>;
}

type AdminSettingsLayoutProps = {
  active: AdminSettingsNavActive;
  children?: unknown;
  className?: string;
  "data-testid"?: string;
};

/** Instance `/admin/*` shell: fixed width + sidebar nav. */
export function AdminSettingsLayout(props: AdminSettingsLayoutProps) @{
  <SettingsShell className={props.className} data-testid={props["data-testid"]}>
    <div className="grid gap-8 md:grid-cols-[14rem_minmax(0,1fr)] md:gap-10">
      <aside className="md:sticky md:top-20 md:self-start">
        <AdminSettingsNav active={props.active} />
      </aside>
      <div className="min-w-0">{props.children}</div>
    </div>
  </SettingsShell>;
}

type OrgSettingsLayoutProps = {
  slug: string;
  active: OrgSettingsNavActive;
  children?: unknown;
  className?: string;
  "data-testid"?: string;
  title?: string;
  subtitle?: string;
};

/** Org `/{owner}/settings/*` shell: fixed width + sidebar nav. */
export function OrgSettingsLayout(props: OrgSettingsLayoutProps) @{
  <SettingsShell className={props.className} data-testid={props["data-testid"]}>
    @if (props.title) {
      <div className="mb-8">
        <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
          {props.title as string}
        </h1>
        @if (props.subtitle) {
          <p className="mt-1 text-[14px] text-muted-foreground">
            {props.subtitle as string}
          </p>;
        }
      </div>;
    }
    <div className="grid gap-8 md:grid-cols-[14rem_minmax(0,1fr)] md:gap-10">
      <aside className="md:sticky md:top-20 md:self-start">
        <OrgSettingsNav slug={props.slug} active={props.active} />
      </aside>
      <div className="min-w-0">{props.children}</div>
    </div>
  </SettingsShell>;
}
