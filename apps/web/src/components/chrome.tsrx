import { Bell, Menu, Plus, X } from "@octanejs/lucide";
import { Link, useNavigate, useParams } from "@octanejs/tanstack-router";
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
  DropdownMenuLinkItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { notificationUnreadCountQueryOptions } from "@/lib/notification-queries";
import { clearSessionQueries, orgListMineQueryOptions } from "@/lib/session-queries";
import { syncSessionPresenceHint } from "@/lib/session-hint";
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
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  if (params.owner && params.repo) {
    return { owner: params.owner, repo: params.repo };
  }
  return null;
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
      <DropdownMenuLinkItem href="/new" onClick={onNavigated}>
        New repository
      </DropdownMenuLinkItem>
      <DropdownMenuLinkItem href="/orgs/new" onClick={onNavigated}>
        New organization
      </DropdownMenuLinkItem>
      @if (repo) {
        <>
          <DropdownMenuSeparator />
          <DropdownMenuLinkItem
            href={`/${repo.owner}/${repo.repo}/issues/new`}
            onClick={onNavigated}
          >
            New issue
          </DropdownMenuLinkItem>
          <DropdownMenuLinkItem
            href={`/${repo.owner}/${repo.repo}/releases/new`}
            onClick={onNavigated}
          >
            New release
          </DropdownMenuLinkItem>
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
    // raw-nav-ok — post-auth teardown: session destroyed, reload clears all chrome state
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
      <DropdownMenuLinkItem
        className="h-auto cursor-pointer flex-col items-start gap-0.5 py-2"
        aria-label={`View @${user.username}'s profile`}
        href={`/${user.username}`}
        onClick={onNavigated}
      >
        <span className="truncate text-[14px] font-medium text-foreground">{displayName}</span>
        <span className="truncate text-[13px] text-muted-foreground">{"@" + user.username}</span>
      </DropdownMenuLinkItem>
      <DropdownMenuSeparator />
      <DropdownMenuLinkItem href="/" onClick={onNavigated}>
        Your repositories
      </DropdownMenuLinkItem>
      <DropdownMenuLinkItem href="/settings/general" onClick={onNavigated}>
        Settings
      </DropdownMenuLinkItem>
      @if (user.role === "sys-admin") {
        <DropdownMenuLinkItem href="/admin/auth" onClick={onNavigated}>
          Admin
        </DropdownMenuLinkItem>;
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
            <DropdownMenuLinkItem href={`/${org.slug}`} onClick={onNavigated}>
              {(org.display_name?.trim() || org.slug) as string}
            </DropdownMenuLinkItem>;
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

  <Link
    to="/notifications"
    preload="intent"
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
  </Link>;
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
    return (
      <div
        role="group"
        aria-label="Account"
        aria-busy="true"
        className={cn("flex h-9 shrink-0 items-center justify-end gap-1", className)}
      >
        <Skeleton className="size-9" />
        <Skeleton className="h-9 w-24" />
      </div>
    );
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
    return (
      <div
        role="group"
        aria-label="Account"
        className={cn("flex h-9 shrink-0 items-center justify-end gap-1", className)}
      >
        <CreateMenu onNavigated={onNavigated} />
        <NotificationsBell onNavigated={onNavigated} />
        <AccountMenu user={user} onNavigated={onNavigated} />
      </div>
    );
  }

  return (
    <div
      role="group"
      aria-label="Account"
      className={cn("flex h-9 shrink-0 items-center justify-end gap-2", className)}
    >
      <Link
        to="/login"
        preload="intent"
        className={cn(buttonVariants({ variant: "ghost" }), "h-9 px-3")}
        onClick={onNavigated}
      >
        Sign in
      </Link>
      @if (allowSignup) {
        <Link
          to="/signup"
          preload="intent"
          className={cn(buttonVariants({ variant: "secondary" }), "h-9 px-3")}
          onClick={onNavigated}
        >
          Sign up
        </Link>;
      }
    </div>
  );
}

const mobileNavItemClass =
  "flex h-11 w-full items-center rounded-md px-3 text-[14px] font-medium text-foreground no-underline outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring";

function MobileNavSheet({ onClose }: { onClose: () => void }) @{
  const navigate = useNavigate();
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
    // raw-nav-ok — post-auth teardown: session destroyed, reload clears all chrome state
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
              <button
                type="button"
                className={cn(mobileNavItemClass, "text-left")}
                onClick={() => {
                  onClose();
                  void navigate({ to: "/" });
                }}
              >
                Home
              </button>
              <p className="px-3 pt-2 pb-1 text-[12px] font-medium text-muted-foreground">
                Create
              </p>
              <Link
                to="/new"
                preload="intent"
                className={mobileNavItemClass}
                onClick={onClose}
              >
                New repository
              </Link>
              <Link
                to="/orgs/new"
                preload="intent"
                className={mobileNavItemClass}
                onClick={onClose}
              >
                New organization
              </Link>
              @if (repo) {
                <>
                  <AppLink
                    href={`/${repo.owner}/${repo.repo}/issues/new`}
                    className={mobileNavItemClass}
                    onClick={onClose}
                  >
                    New issue
                  </AppLink>
                  <AppLink
                    href={`/${repo.owner}/${repo.repo}/releases/new`}
                    className={mobileNavItemClass}
                    onClick={onClose}
                  >
                    New release
                  </AppLink>
                </>;
              }
              <p className="px-3 pt-2 pb-1 text-[12px] font-medium text-muted-foreground">
                Account
              </p>
              <Link
                to="/notifications"
                preload="intent"
                className={mobileNavItemClass}
                onClick={onClose}
              >
                Notifications
              </Link>
              <Link to="/" preload="intent" className={mobileNavItemClass} onClick={onClose}>
                Your repositories
              </Link>
              <Link
                to="/settings/general"
                preload="intent"
                className={mobileNavItemClass}
                onClick={onClose}
              >
                Settings
              </Link>
              @if (user.role === "sys-admin") {
                <Link
                  to="/admin/auth"
                  preload="intent"
                  className={mobileNavItemClass}
                  onClick={onClose}
                >
                  Admin
                </Link>;
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
                      {(org.display_name?.trim() || org.slug) as string}
                    </AppLink>;
                  }
                </>;
              }
              <Link
                to="/status"
                preload="intent"
                className={mobileNavItemClass}
                activeProps={{
                  className: cn(mobileNavItemClass, "text-primary"),
                }}
                onClick={onClose}
              >
                Status
              </Link>
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
                Sign out
              </button>
            </div>
          </div>;
        } @else {
          <div className="flex flex-col gap-2">
            <Link
              to="/status"
              preload="intent"
              className={mobileNavItemClass}
              activeProps={{
                className: cn(mobileNavItemClass, "text-primary"),
              }}
              onClick={onClose}
            >
              Status
            </Link>
            @if (!needsSetup) {
              <div
                className={cn(
                  "grid gap-2 pt-1",
                  allowSignup ? "grid-cols-2" : "grid-cols-1",
                )}
              >
                <Link
                  to="/login"
                  preload="intent"
                  className={cn(
                    buttonVariants({ variant: "ghost" }),
                    "h-11 w-full justify-center",
                  )}
                  onClick={onClose}
                >
                  Sign in
                </Link>
                @if (allowSignup) {
                  <Link
                    to="/signup"
                    preload="intent"
                    className={cn(
                      buttonVariants({ variant: "secondary" }),
                      "h-11 w-full justify-center",
                    )}
                    onClick={onClose}
                  >
                    Sign up
                  </Link>;
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
      <Link
        to="/"
        preload="intent"
        className="col-start-1 row-start-1 flex shrink-0 items-center gap-2.5 no-underline"
        onClick={closeMenu}
      >
        <OxideanMark size={32} />
        <span className="hidden font-[family-name:var(--font-display)] text-[22px] font-semibold tracking-tight text-foreground md:inline">
          Oxidean
        </span>
      </Link>

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
      <Link
        to="/status"
        preload="intent"
        className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
        activeProps={{
          className: "font-medium text-primary underline underline-offset-4",
        }}
      >
        Status
      </Link>
    </div>
  </footer>;
}
