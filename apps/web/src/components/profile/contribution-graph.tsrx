import type { UserContributionDay } from "@oxidean/api-client";
import {
  buildContributionWeeks,
  contributionDayTooltip,
  contributionLevel,
  contributionMonthLabels,
} from "@/lib/user-contributions";

const CELL = 10;
const GAP = 3;

/** Weekday row labels — GitHub labels Mon/Wed/Fri only. */
const WEEKDAY_LABELS: Record<number, string> = {
  1: "Mon",
  3: "Wed",
  5: "Fri",
};

function cellStyle(level: number): { backgroundColor: string } {
  return { backgroundColor: `var(--contribution-${level})` };
}

/** GitHub-style 53×7 contribution heatmap — pure divs, no chart dep. */
export function ContributionGraph(props: { days: UserContributionDay[] }) @{
  const max = props.days.reduce((a, d) => Math.max(a, d.count), 0);
  const weeks = buildContributionWeeks(props.days);
  const monthLabels = contributionMonthLabels(weeks);

  <div className="overflow-x-auto" data-testid="contribution-graph">
    <div className="flex" style={{ gap: `${GAP}px` }}>
      <div
        className="flex shrink-0 flex-col"
        style={{ gap: `${GAP}px`, paddingTop: "16px" }}
        aria-hidden="true"
      >
        @for (const i of [0, 1, 2, 3, 4, 5, 6]; key i) {
          <span
            className="text-[10px] leading-none text-muted-foreground"
            style={{ height: `${CELL}px`, width: "28px" }}
          >
            {(WEEKDAY_LABELS[i] ?? "") as string}
          </span>;
        }
      </div>
      <div>
        <div
          className="relative"
          style={{ height: "13px", marginBottom: "3px" }}
          aria-hidden="true"
        >
          @for (const label of monthLabels; key label.index) {
            <span
              className="absolute text-[10px] leading-none text-muted-foreground"
              style={{ left: `${label.index * (CELL + GAP)}px` }}
            >
              {label.label as string}
            </span>;
          }
        </div>
        <div className="flex" style={{ gap: `${GAP}px` }} role="table">
          @for (const week of weeks; key week[0]?.date ?? weeks.indexOf(week)) {
            <div className="flex flex-col" style={{ gap: `${GAP}px` }} role="row">
              @for (const day of week; key day?.date ?? `pad-${week.indexOf(day)}`) {
                @if (day === null) {
                  <div
                    style={{ width: `${CELL}px`, height: `${CELL}px` }}
                    aria-hidden="true"
                  />;
                } @else {
                  <div
                    className="rounded-[2px]"
                    style={{
                      width: `${CELL}px`,
                      height: `${CELL}px`,
                      ...cellStyle(contributionLevel(day.count, max)),
                    }}
                    title={contributionDayTooltip(day.date, day.count) as string}
                    aria-label={contributionDayTooltip(day.date, day.count) as string}
                    data-date={day.date as string}
                    data-count={String(day.count) as string}
                    role="gridcell"
                  />;
                }
              }
            </div>;
          }
        </div>
      </div>
    </div>
    <div
      className="mt-2 flex items-center justify-end gap-1 text-[10px] text-muted-foreground"
      aria-hidden="true"
    >
      <span>Less</span>
      @for (const i of [0, 1, 2, 3, 4]; key i) {
        <div
          className="rounded-[2px]"
          style={{ width: `${CELL}px`, height: `${CELL}px`, ...cellStyle(i) }}
        />;
      }
      <span>More</span>
    </div>
  </div>;
}
