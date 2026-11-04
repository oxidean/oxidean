import type { UserContributionMonth, UserContributionSection } from "@oxidean/api-client";
import { ChevronDown } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { contributionMonthHeading, contributionSectionSummary } from "@/lib/user-contributions";

function itemizedKind(kind: string): boolean {
  return kind === "pull_requests" || kind === "issues" || kind === "reviews";
}

/** GitHub's "Contribution activity" — month-grouped sections with
 * expandable per-repo detail. */
export function ContributionActivity(props: { months: UserContributionMonth[] }) @{
  <section className="mt-10" aria-labelledby="contribution-activity-heading">
    <h2
      id="contribution-activity-heading"
      className="border-b border-border pb-2 text-[16px] font-semibold text-foreground"
    >
      Contribution activity
    </h2>
    @for (const month of props.months; key month.month) {
      <div className="mt-6" data-testid="contribution-month">
        <h3 className="text-[14px] font-semibold text-foreground">
          {contributionMonthHeading(month.month) as string}
        </h3>
        <div className="mt-2 flex flex-col gap-1.5">
          @for (const section of month.sections; key section.kind) {
            <ContributionSectionRow section={section} />;
          }
        </div>
      </div>;
    }
  </section>;
}

function ContributionSectionRow(props: { section: UserContributionSection }) @{
  const section = props.section;
  const itemized = itemizedKind(section.kind);
  // Group itemized rows under their repo slug so each repo gets a header.
  const itemsBySlug = new Map<string, typeof section.items>();
  for (const item of section.items) {
    const list = itemsBySlug.get(item.slug) ?? [];
    list.push(item);
    itemsBySlug.set(item.slug, list);
  }

  <details className="group rounded-md border border-border" data-kind={section.kind as string}>
    <summary className="flex cursor-pointer list-none items-center gap-3 px-3 py-2 text-[14px] text-foreground marker:hidden [&::-webkit-details-marker]:hidden">
      <ChevronDown
        aria-hidden
        size={14}
        className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180"
      />
      <span className="min-w-0 flex-1">
        {contributionSectionSummary(section.kind, section.total, section.repo_count) as string}
      </span>
      @if (section.truncated) {
        <span className="text-[12px] text-muted-foreground">truncated</span>;
      }
    </summary>
    <div className="border-t border-border px-3 py-2">
      @if (itemized) {
        <div className="flex flex-col gap-2">
          @for (const entry of Array.from(itemsBySlug.entries()); key entry[0]) {
            <div>
              <AppLink
                href={"/" + (entry[0] as string)}
                className="text-[13px] font-semibold text-primary no-underline hover:underline"
              >
                {entry[0] as string}
              </AppLink>
              <ul className="mt-1 flex flex-col gap-0.5">
                @for (const item of entry[1]; key item.href) {
                  <li className="text-[13px]">
                    <AppLink
                      href={item.href as string}
                      className="text-primary no-underline hover:underline"
                    >
                      {"#" + String(item.number)}
                    </AppLink>
                    <span className="ml-1.5 text-muted-foreground">
                      {item.title as string}
                    </span>
                  </li>;
                }
              </ul>
            </div>;
          }
        </div>;
      } @else {
        <ul className="flex flex-col gap-0.5">
          @for (const repo of section.repos; key repo.slug) {
            <li className="flex items-baseline gap-2 text-[13px]">
              <AppLink
                href={"/" + (repo.slug as string)}
                className="text-primary no-underline hover:underline"
              >
                {repo.slug as string}
              </AppLink>
              @if (section.kind === "commits") {
                <span className="text-muted-foreground">
                  {
                    (repo.count === 1
                      ? "1 commit"
                      : `${repo.count} commits`) as string
                  }
                </span>;
              }
            </li>;
          }
        </ul>;
      }
    </div>
  </details>;
}
