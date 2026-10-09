import {
  Activity,
  Bell,
  BookMarked,
  Building2,
  CircleDot,
  Home,
  LogOut,
  Menu,
  Plus,
  Settings,
  ShieldCheck,
  Tag,
  Users,
  X,
} from "@octanejs/lucide";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useEffectEvent, useState } from "octane";
import { GlobalSearch } from "@/components/global-search";
import { OxideanMark } from "@/components/oxidean-mark";
import { ThemeSelect } from "@/components/theme-select";
import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { notificationUnreadCountQueryOptions } from "@/lib/notification-queries";
import { clearSessionQueries, orgListMineQueryOptions } from "@/lib/session-queries";
import { syncSessionPresenceHint } from "@/lib/session-hint";
import { repoParamsFromPathname } from "@/lib/route-params";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";
import { AppLink } from "@/components/ui/app-link";

function initialsFor(user: UserPublic): string {
  const source = user.display_name?.trim() || user.username || "?";
  const parts = source.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[0]![0] ?? ""}${parts[1]![0] ?? ""}`.toUpperCase();
  }
  return source.slice(0, 2).toUpperCase();
}

function AccountAvatar({ user, size = "sm" }: { user: UserPublic; size?: "sm" | "md" }) @{
  const box = size === "md" ? "size-9" : "size-7";
  const text = size === "md" ? "text-[12px]" : "text-[11px]";
  const px = size === "md" ? 36 : 28;
  const dim = `${px}px`;

  <div
    className={cn("oxidean-squircle relative block shrink-0 overflow-hidden bg-muted", box)}
    style={{ width: dim, height: dim, minWidth: dim, minHeight: dim }}
  >
    @if (user.avatar_url) {
      <img
        src={user.avatar_url as string}
        alt=""
        width={px}
        height={px}
        className="block size-full object-cover"
      />;
    } @else {
      <span
        aria-hidden
        className={cn(
          "flex size-full items-center justify-center font-semibold leading-none text-foreground",
          text,
        )}
      >
        {initialsFor(user) as string}
      </span>;
    }
  </div>;
}

function useRepoContext(): { owner: string; repo: string } | null {
  return repoParamsFromPathname();
}

function CreateMenu({ onNavigated }: { onNavigated?: () => void }) @{
  const repo = useRepoContext();

  <DropdownMenu>
    <DropdownMenuTrigger
      className={cn(
        "inline-flex size-9 items-center justify-center rounded-md text-foreground outline-none",
        "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring",
      )}
      aria-label="Create new…"
    >
      <Plus aria-hidden size={18} />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" className="min-w-52">
      <DropdownMenuItem
        onClick={() => {
          onNavigated?.();
          window.location.assign("/new");
        }}
      >
        New repository
      </DropdownMenuItem>
      <DropdownMenuItem
        onClick={() => {
          onNavigated?.();
          window.location.assign("/orgs/new");
        }}
      >
        New organization
      </DropdownMenuItem>
      @if (repo) {
        <>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={() => {
              onNavigated?.();
              window.location.assign(`/${repo.owner}/${repo.repo}/issues/new`);
            }}
          >
            New issue
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => {
              onNavigated?.();
              window.location.assign(`/${repo.owner}/${repo.repo}/releases/new`);
            }}
          >
            New release
          </DropdownMenuItem>
        </>;
      }
    </DropdownMenuContent>
  </DropdownMenu>;
}

function AccountMenu({
  user,
  onNavigated,
}: {
  user: UserPublic;
  onNavigated?: () => void;
}) @{
  const queryClient = useQueryClient();
  const orgsQ = useQuery(orgListMineQueryOptions());
  const orgs = orgsQ.data ?? [];

  async function logout() {
    try {
      await apiClient.auth.logout();
    } catch {
      /* still clear local chrome */
    }
    syncSessionPresenceHint(false);
    clearSessionQueries(queryClient);
    onNavigated?.();
    window.location.assign("/");
  }

  const displayName = (user.display_name?.trim() || user.username) as string;

  <DropdownMenu>
    <DropdownMenuTrigger
      className={cn(
        "inline-flex h-9 items-center gap-2 rounded-md px-2 text-[14px] font-normal text-foreground outline-none",
        "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring",
      )}
      aria-label="Account menu"
    >
      <AccountAvatar user={user} />
      <span className="hidden text-muted-foreground md:inline">{"@" + user.username}</span>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" className="min-w-56">
      <DropdownMenuItem
        className="h-auto cursor-pointer flex-col items-start gap-0.5 py-2"
        aria-label={`View @${user.username}'s profile`}
        onClick={() => {
          onNavigated?.();
          window.location.assign(`/${user.username}`);
        }}
      >
        <span className="truncate text-[14px] font-medium text-foreground">{displayName}</span>
        <span className="truncate text-[13px] text-muted-foreground">{"@" + user.username}</span>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem
        onClick={() => {
          onNavigated?.();
          window.location.assign("/");
        }}
      >
        Your repositories
      </DropdownMenuItem>
      <DropdownMenuItem
        onClick={() => {
          onNavigated?.();
          window.location.assign("/settings/general");
        }}
      >
        Settings
      </DropdownMenuItem>
      @if (user.role === "sys-admin") {
        <DropdownMenuItem
          onClick={() => {
            onNavigated?.();
            window.location.assign("/admin/auth");
          }}
        >
          Admin
        </DropdownMenuItem>;
      }
      @if (orgs.length > 0) {
        <>
          <DropdownMenuSeparator />
          <div
            className="px-2 py-1.5 text-[12px] font-medium text-muted-foreground"
            role="presentation"
          >
            Organizations
          </div>
          @for (const org of orgs; key org.id) {
            <DropdownMenuItem
              onClick={() => {
                onNavigated?.();
                window.location.assign(`/${org.slug}`);
              }}
            >
              {(org.display_name?.trim() || org.slug) as string}
            </DropdownMenuItem>;
          }
        </>;
      }
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={() => void logout()}>Sign out</DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>;
}

function NotificationsBell({ onNavigated }: { onNavigated?: () => void }) @{
  const unreadQ = useQuery(notificationUnreadCountQueryOptions());
  const countNum = Number(unreadQ.data ?? 0);
  const count = Number.isFinite(countNum) ? countNum : 0;
  const label =
    count > 0
      ? (("Notifications (" + String(count) + " unread)") as string)
      : ("Notifications" as string);
  const badgeText = (count > 99 ? "99+" : String(count)) as string;

  <AppLink
    href="/notifications"
    aria-label={label}
    className={cn(
      "relative inline-flex size-9 items-center justify-center rounded-md text-foreground outline-none",
      "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring",
    )}
    onClick={onNavigated}
  >
    <Bell aria-hidden size={18} />
    @if (count > 0) {
      <span
        aria-hidden
        className="absolute right-1 top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-semibold leading-none text-primary-foreground"
      >
        {badgeText}
      </span>;
    }
  </AppLink>;
}

/** Resolved signed-in cluster — `gap-1`; mirrors `AccountClusterPending`'s authed candidate. */
function AccountClusterSignedIn({
  user,
  onNavigated,
  className,
}: {
  user: UserPublic;
  onNavigated?: () => void;
  className?: string;
}) @{
  <div
    role="group"
    aria-label="Account"
    className={cn("flex h-9 shrink-0 items-center justify-end gap-1", className)}
  >
    <CreateMenu onNavigated={onNavigated} />
    <NotificationsBell onNavigated={onNavigated} />
    <AccountMenu user={user} onNavigated={onNavigated} />
  </div>;
}

/** Resolved signed-out cluster — `gap-2`; mirrors `AccountClusterPending`'s anon candidate. */
function AccountClusterAnon({
  allowSignup,
  onNavigated,
  className,
}: {
  allowSignup: boolean;
  onNavigated?: () => void;
  className?: string;
}) @{
  <div
    role="group"
    aria-label="Account"
    className={cn("flex h-9 shrink-0 items-center justify-end gap-2", className)}
  >
    <AppLink
      href="/login"
      className={cn(buttonVariants({ variant: "ghost" }), "h-9 px-3")}
      onClick={onNavigated}
    >
      Sign in
    </AppLink>
    @if (allowSignup) {
      <AppLink
        href="/signup"
        className={cn(buttonVariants({ variant: "secondary" }), "h-9 px-3")}
        onClick={onNavigated}
      >
        Sign up
      </AppLink>;
    }
  </div>;
}

/**
 * Pending cluster — the island is SSR'd before `auth.me` resolves, so emit
 * both candidates and let CSS pick by the `data-oxidean-session` stamp the
 * web tier validated on `<html>` (same dual-render contract as the `/` body):
 * anon button shapes paint by default, the signed-in cluster only when the
 * stamp says a live session is present. Each candidate mirrors its resolved
 * cluster's geometry so hydration swaps shape, not layout.
 */
function AccountClusterPending({ className }: { className?: string }) @{
  <div
    role="group"
    aria-label="Account"
    aria-busy="true"
    className={cn("flex h-9 shrink-0 items-center justify-end", className)}
  >
    <div data-header-skeleton-anon className="items-center gap-2">
      <Skeleton className="h-9 w-[4.25rem]" />
      {/* The Sign-up-shaped bar paints only when the web tier's
          `data-oxidean-signup` stamp says registration is open. */}
      <Skeleton className="h-9 w-[4.5rem]" data-header-skeleton-signup />
    </div>
    <div data-header-skeleton-authed className="items-center gap-1">
      <Skeleton className="size-9" />
      <Skeleton className="size-9" />
      <Skeleton className="h-9 w-28" />
    </div>
  </div>;
}

function AccountActions({
  className,
  onNavigated,
}: {
  className?: string;
  onNavigated?: () => void;
}) {
  const { pending, user, needsSetup, allowSignup } = useChromeAccountState();

  if (pending || user === undefined || needsSetup === null) {
    return <AccountClusterPending className={className} />;
  }

  // UI-SPEC: while needs_setup, omit Sign in / Sign up (mark + theme OK).
  if (needsSetup) {
    return (
      <div
        role="group"
        aria-label="Account"
        className={cn("flex h-9 shrink-0 items-center justify-end gap-1", className)}
      />
    );
  }

  if (user) {
    return <AccountClusterSignedIn user={user} onNavigated={onNavigated} className={className} />;
  }

  return (
    <AccountClusterAnon allowSignup={allowSignup} onNavigated={onNavigated} className={className} />
  );
}

const mobileNavItemClass =
  "flex h-11 w-full items-center gap-3 rounded-md px-3 text-[14px] font-medium text-foreground no-underline outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring";

function MobileNavSheet({ onClose }: { onClose: () => void }) @{
  const queryClient = useQueryClient();
  const { pending, user, needsSetup, allowSignup } = useChromeAccountState();
  const repo = useRepoContext();
  const orgsQ = useQuery({
    ...orgListMineQueryOptions(),
    enabled: !!user,
  });
  const orgs = orgsQ.data ?? [];

  async function logout() {
    try {
      await apiClient.auth.logout();
    } catch {
      /* still clear */
    }
    syncSessionPresenceHint(false);
    clearSessionQueries(queryClient);
    onClose();
    window.location.assign("/");
  }

  <nav
    id="mobile-nav"
    aria-label="Site"
    className={cn(
      "fixed inset-x-0 top-14 z-40 max-h-[calc(100dvh-3.5rem)] overflow-y-auto",
      "border-b border-border bg-card shadow-lg md:hidden",
    )}
  >
    <div className="mx-auto flex max-w-6xl flex-col gap-3 px-3 py-3 sm:px-4">
      @if (needsSetup) {
        <ThemeSelect layout="panel" />;
      }

      @if (pending || user === undefined || needsSetup === null) {
        <div className="flex flex-col gap-2" aria-busy="true" aria-label="Account">
          <div className="flex items-center gap-3 px-3 py-2">
            <Skeleton className="size-9 rounded-md" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-28" />
              <Skeleton className="h-3 w-20" />
            </div>
          </div>
          <Skeleton className="h-11 w-full" />
          <Skeleton className="h-11 w-full" />
          <Skeleton className="h-11 w-full" />
        </div>;
      } @else {
        @if (user) {
          <div className="flex flex-col">
            <AppLink
              href={`/${user.username}`}
              className="mb-1 flex items-center gap-3 rounded-md px-3 py-2 no-underline outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
              aria-label={`View @${user.username}'s profile`}
              onClick={onClose}
            >
              <AccountAvatar user={user} size="md" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[14px] font-medium text-foreground">
                  {(user.display_name?.trim() || user.username) as string}
                </p>
                <p className="truncate text-[13px] text-muted-foreground">
                  {"@" + user.username}
                </p>
              </div>
            </AppLink>
            <div className="flex flex-col gap-0.5" role="list">
              <AppLink
                href="/"
                className={cn(mobileNavItemClass, "text-left")}
                onClick={onClose}
              >
                <Home aria-hidden size={16} className="text-muted-foreground" />
                Home
              </AppLink>
              <p className="px-3 pt-2 pb-1 text-[12px] font-medium text-muted-foreground">
                Create
              </p>
              <AppLink href="/new" className={mobileNavItemClass} onClick={onClose}>
                <BookMarked aria-hidden size={16} className="text-muted-foreground" />
                New repository
              </AppLink>
              <AppLink href="/orgs/new" className={mobileNavItemClass} onClick={onClose}>
                <Building2 aria-hidden size={16} className="text-muted-foreground" />
                New organization
              </AppLink>
              @if (repo) {
                <>
                  <AppLink
                    href={`/${repo.owner}/${repo.repo}/issues/new`}
                    className={mobileNavItemClass}
                    onClick={onClose}
                  >
                    <CircleDot
                      aria-hidden
                      size={16}
                      className="text-muted-foreground"
                    />
                    New issue
                  </AppLink>
                  <AppLink
                    href={`/${repo.owner}/${repo.repo}/releases/new`}
                    className={mobileNavItemClass}
                    onClick={onClose}
                  >
                    <Tag aria-hidden size={16} className="text-muted-foreground" />
                    New release
                  </AppLink>
                </>;
              }
              <p className="px-3 pt-2 pb-1 text-[12px] font-medium text-muted-foreground">
                Account
              </p>
              <AppLink href="/notifications" className={mobileNavItemClass} onClick={onClose}>
                <Bell aria-hidden size={16} className="text-muted-foreground" />
                Notifications
              </AppLink>
              <AppLink href="/" className={mobileNavItemClass} onClick={onClose}>
                <BookMarked aria-hidden size={16} className="text-muted-foreground" />
                Your repositories
              </AppLink>
              <AppLink
                href="/settings/general"
                className={mobileNavItemClass}
                onClick={onClose}
              >
                <Settings aria-hidden size={16} className="text-muted-foreground" />
                Settings
              </AppLink>
              @if (user.role === "sys-admin") {
                <AppLink
                  href="/admin/auth"
                  className={mobileNavItemClass}
                  onClick={onClose}
                >
                  <ShieldCheck
                    aria-hidden
                    size={16}
                    className="text-muted-foreground"
                  />
                  Admin
                </AppLink>;
              }
              @if (orgs.length > 0) {
                <>
                  <p className="px-3 pt-2 pb-1 text-[12px] font-medium text-muted-foreground">
                    Organizations
                  </p>
                  @for (const org of orgs; key org.id) {
                    <AppLink
                      href={`/${org.slug}`}
                      className={mobileNavItemClass}
                      onClick={onClose}
                    >
                      <Users
                        aria-hidden
                        size={16}
                        className="text-muted-foreground"
                      />
                      {(org.display_name?.trim() || org.slug) as string}
                    </AppLink>;
                  }
                </>;
              }
              <AppLink
                href="/status"
                className={mobileNavItemClass}
                activeProps={{
                  className: cn(mobileNavItemClass, "text-primary"),
                }}
                onClick={onClose}
              >
                <Activity aria-hidden size={16} className="text-muted-foreground" />
                Status
              </AppLink>
              <div className="my-1 h-px bg-border" role="separator" />
              <button
                type="button"
                className={cn(
                  mobileNavItemClass,
                  "text-left text-muted-foreground hover:text-foreground",
                )}
                onClick={() => {
                  void logout();
                }}
              >
                <LogOut aria-hidden size={16} />
                Sign out
              </button>
            </div>
          </div>;
        } @else {
          <div className="flex flex-col gap-2">
            <AppLink
              href="/status"
              className={mobileNavItemClass}
              activeProps={{
                className: cn(mobileNavItemClass, "text-primary"),
              }}
              onClick={onClose}
            >
              Status
            </AppLink>
            @if (!needsSetup) {
              <div
                className={cn(
                  "grid gap-2 pt-1",
                  allowSignup ? "grid-cols-2" : "grid-cols-1",
                )}
              >
                <AppLink
                  href="/login"
                  className={cn(
                    buttonVariants({ variant: "ghost" }),
                    "h-11 w-full justify-center",
                  )}
                  onClick={onClose}
                >
                  Sign in
                </AppLink>
                @if (allowSignup) {
                  <AppLink
                    href="/signup"
                    className={cn(
                      buttonVariants({ variant: "secondary" }),
                      "h-11 w-full justify-center",
                    )}
                    onClick={onClose}
                  >
                    Sign up
                  </AppLink>;
                }
              </div>;
            }
          </div>;
        }
      }
    </div>
  </nav>;
}

export function SiteHeader() @{
  const [menuOpen, setMenuOpen] = useState(false);
  const { needsSetup } = useChromeAccountState();

  const closeMenu = useEffectEvent(() => setMenuOpen(false));

  const onDocPointer = useEffectEvent((event: PointerEvent) => {
    const t = event.target;
    if (!(t instanceof Element)) return;
    if (t.closest("[data-site-header]")) return;
    setMenuOpen(false);
  });

  const onDocKey = useEffectEvent((event: KeyboardEvent) => {
    if (event.key === "Escape") setMenuOpen(false);
  });

  useEffect(() => {
    if (!menuOpen) return;
    document.addEventListener("pointerdown", onDocPointer);
    document.addEventListener("keydown", onDocKey);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("pointerdown", onDocPointer);
      document.removeEventListener("keydown", onDocKey);
      document.body.style.overflow = prevOverflow;
    };
  }, [menuOpen]);

  useEffect(() => {
    const mq = window.matchMedia("(min-width: 768px)");
    const onChange = () => {
      if (mq.matches) closeMenu();
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  // Fixed overlay stays inside <header> so we don't insert siblings next to
  // view-transition-named <main> (that caused insertBefore DOM errors).
  <header
    data-site-header
    className="sticky top-0 z-50 border-b border-border bg-header/95 backdrop-blur-md supports-backdrop-filter:bg-header/85"
  >
    <div className="mx-auto grid h-14 max-w-6xl grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 px-3 sm:gap-x-3 sm:px-4 md:h-[3.75rem] md:gap-x-4">
      <AppLink
        href="/"
        className="col-start-1 row-start-1 flex shrink-0 items-center gap-2.5 no-underline"
        onClick={closeMenu}
      >
        <OxideanMark size={32} />
        <span className="hidden font-[family-name:var(--font-display)] text-[22px] font-semibold tracking-tight text-foreground md:inline">
          Oxidean
        </span>
      </AppLink>

      <GlobalSearch className="col-start-2 row-start-1" />

      <div className="col-start-3 row-start-1 hidden items-center justify-end gap-2 md:flex">
        @if (needsSetup) {
          <ThemeSelect />;
        }
        <AccountActions />
      </div>

      <button
        type="button"
        className="col-start-3 row-start-1 inline-flex h-10 w-10 shrink-0 items-center justify-center justify-self-end rounded-md border border-border bg-card text-foreground outline-none transition-colors hover:border-foreground/25 hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring md:hidden"
        aria-label={menuOpen ? "Close menu" : "Open menu"}
        aria-expanded={menuOpen}
        aria-controls="mobile-nav"
        onClick={() => setMenuOpen(!menuOpen)}
      >
        @if (menuOpen) {
          <X aria-hidden size={20} />;
        } @else {
          <Menu aria-hidden size={20} />;
        }
      </button>
    </div>

    @if (menuOpen) {
      <>
        <button
          type="button"
          aria-label="Dismiss menu"
          className="fixed inset-0 z-30 bg-background/70 backdrop-blur-[2px] md:hidden"
          onClick={closeMenu}
        />
        <MobileNavSheet onClose={closeMenu} />
      </>;
    }
  </header>;
}

export function SiteFooter() @{
  <footer className="border-t border-border bg-header py-10 text-[14px] text-muted-foreground">
    <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 px-4">
      <span className="font-[family-name:var(--font-display)] text-[13px] font-medium tracking-tight text-foreground/80">
        © Oxidean
      </span>
      <AppLink
        href="/status"
        className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
        activeProps={{
          className: "font-medium text-primary underline underline-offset-4",
        }}
      >
        Status
      </AppLink>
    </div>
  </footer>;
}
