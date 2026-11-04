import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { ChevronDown } from "@octanejs/lucide";
import type { UserContributions } from "@oxidean/api-client";
import { useSearchString } from "@/lib/pathname.tsrx";
import { apiClient } from "@/lib/api-client";
import { AppLink } from "@/components/ui/app-link";
import { Skeleton } from "@/components/ui/skeleton";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { setAuthMeCache } from "@/lib/session-queries";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import { useMounted } from "@/lib/use-mounted.tsrx";
import { ContributionGraph } from "@/components/profile/contribution-graph";
import { ContributionActivity } from "@/components/profile/contribution-activity";
import {
  contributionRangeHref,
  contributionRangeLabel,
  resolveContributionRange,
} from "@/lib/user-contributions";

function userContributionsQueryOptions(username: string, from?: string, to?: string) {
  return queryOptions({
    queryKey: ["user", "contributions", username, from ?? "", to ?? ""],
    queryFn: async (): Promise<UserContributions | null> => {
      const res = await apiClient.user.getContributions({
        username,
        from: from ?? null,
        to: to ?? null,
      });
      return res.ok ? res.data : null;
    },
    enabled: !!username,
    retry: false,
    staleTime: 60_000,
  });
}

const YEAR_LINK_ACTIVE =
  "block rounded-md px-2 py-0.5 text-[13px] font-semibold text-primary no-underline";
const YEAR_LINK_IDLE =
  "block rounded-md px-2 py-0.5 text-[13px] text-muted-foreground no-underline hover:bg-muted hover:text-foreground";

const TYPE_LABELS: { key: keyof UserContributions["by_type"]; label: string }[] = [
  { key: "commits", label: "Commits" },
  { key: "pull_requests", label: "Pull requests" },
  { key: "issues", label: "Issues" },
  { key: "reviews", label: "Reviews" },
];

/**
 * Contribution graph + year rail + "Contribution activity" for `/{username}`
 * (COL-12). Hidden entirely for zero-contribution users (GitHub parity).
 */
export function UserContributionsSection(props: { username: string; isSelf?: boolean }) @{
  const searchStr = useSearchString();
  const range = resolveContributionRange(searchStr);
  const q = useQuery(userContributionsQueryOptions(props.username, range.from, range.to));
  const c = q.data;

  @if (q.isPending) {
    <section className="mt-10" aria-busy="true" aria-label="Loading contributions">
      <Skeleton className="h-5 w-64" />
      <Skeleton className="mt-4 h-28 w-full max-w-2xl" />
    </section>;
  } @else if (c && c.total > 0) {
    <section className="mt-10" aria-label="Contributions" data-testid="user-contributions">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-[16px] font-semibold text-foreground">
          {
            `${c.total} contribution${c.total === 1 ? "" : "s"} in ${contributionRangeLabel(range)}` as string
          }
        </h2>
        <div className="flex items-center gap-3">
          @if (c.includes_private) {
            <span className="text-[12px] text-muted-foreground">
              Includes private contributions
            </span>;
          }
          @if (props.isSelf) {
            <ContributionSettingsMenu />;
          }
        </div>
      </div>
      <div className="mt-1 text-[12px] text-muted-foreground" data-testid="contribution-breakdown">
        @for (const t of TYPE_LABELS; key t.key) {
          @if (c.by_type[t.key] > 0) {
            <span className="mr-3">
              {`${t.label} ${Math.round((c.by_type[t.key] * 100) / c.total)}%` as string}
            </span>;
          }
        }
      </div>
      @if (c.longest_streak > 0) {
        <div
          className="mt-1 text-[12px] text-muted-foreground"
          data-testid="contribution-streaks"
        >
          {
            // `current_streak` is measured at the range end — for a past
            // year that isn't "now", so label it accordingly.
            `${c.to >= new Date().toISOString().slice(0, 10) ? "Current" : "End-of-range"} streak: ${c.current_streak} day${c.current_streak === 1 ? "" : "s"} · Longest streak: ${c.longest_streak} day${c.longest_streak === 1 ? "" : "s"}` as string
          }
        </div>;
      }
      <div className="mt-4 flex gap-4">
        <ContributionGraph days={c.days} />
        <nav
          className="ml-auto flex shrink-0 flex-col gap-0.5"
          aria-label="Contribution years"
          data-testid="contribution-years"
        >
          @for (const year of c.years; key year) {
            <AppLink
              href={contributionRangeHref(props.username, year) as string}
              className={
                (range.selected === year ? YEAR_LINK_ACTIVE : YEAR_LINK_IDLE) as string
              }
              aria-current={range.selected === year ? "true" : undefined}
            >
              {String(year) as string}
            </AppLink>;
          }
        </nav>
      </div>
      <ContributionActivity months={c.months} />
    </section>;
  } @else {
    <></>;
  }
}

/**
 * GitHub "Contribution settings" dropdown — owner-only, sits beside the
 * contributions heading. The private-contributions checkbox writes
 * `show_private_contributions` through `user.updateProfile` (same path as
 * /settings/profile), then refetches the contribution view.
 */
function ContributionSettingsMenu() @{
  const queryClient = useQueryClient();
  const account = useChromeAccountState();
  const mounted = useMounted();
  const [busy, setBusy] = useState(false);
  const user = mounted ? account.user : undefined;

  async function togglePrivate(next: boolean) {
    if (!user || busy) return;
    setBusy(true);
    try {
      const res = await apiClient.user.updateProfile({
        display_name: user.display_name,
        username: user.username,
        bio: user.bio,
        show_private_contributions: next,
      });
      if (res.ok) {
        setAuthMeCache(queryClient, res.data);
        void queryClient.invalidateQueries({ queryKey: ["user", "contributions"] });
      }
    } finally {
      setBusy(false);
    }
  }

  @if (user) {
    <DropdownMenu>
      <DropdownMenuTrigger
        className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[12px] font-semibold text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
        disabled={busy}
        data-testid="contribution-settings-trigger"
      >
        Contribution settings
        <ChevronDown aria-hidden size={13} />
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuCheckboxItem
          checked={user.show_private_contributions !== false}
          disabled={busy}
          onCheckedChange={(checked) => void togglePrivate(checked === true)}
          data-testid="contribution-settings-private"
        >
          Private contributions
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem>
          <AppLink href="/settings/profile" className="flex-1 no-underline">
            Profile settings
          </AppLink>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>;
  } @else {
    <></>;
  }
}
