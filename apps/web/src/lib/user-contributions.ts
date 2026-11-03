import type { UserContributionDay } from "@oxidean/api-client";

/** A resolved contribution range — `from`/`to` are `YYYY-MM-DD` or absent
 * for the rolling "last year" default. */
export type ContributionRange = {
  from?: string;
  to?: string;
  /** Selected year, or "last_year" for the default rolling range. */
  selected: "last_year" | number;
};

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

/** `?from`/`?to` URL state — year links set `from=YYYY-01-01`; anything
 * malformed falls back to the default view rather than erroring the RPC. */
export function resolveContributionRange(searchStr: string): ContributionRange {
  const sp = new URLSearchParams(searchStr.replace(/^\?/, ""));
  const from = sp.get("from") ?? undefined;
  const to = sp.get("to") ?? undefined;
  if (from && DAY_RE.test(from)) {
    return {
      from,
      to: to && DAY_RE.test(to) ? to : undefined,
      selected: Number(from.slice(0, 4)),
    };
  }
  return { selected: "last_year" };
}

/** `/{username}` year-link href; absent params = the default rolling view. */
export function contributionRangeHref(username: string, year: number | "last_year"): string {
  if (year === "last_year") return `/${username}`;
  return `/${username}?from=${year}-01-01&to=${year}-12-31`;
}

/** Header phrasing — "N contributions in the last year" vs "in YYYY". */
export function contributionRangeLabel(range: ContributionRange): string {
  return range.selected === "last_year" ? "the last year" : String(range.selected);
}

function parseContributionDay(date: string): Date {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}

/** Heatmap intensity 0–4 — quartiles scaled to the user's busiest day. */
export function contributionLevel(count: number, max: number): number {
  if (count <= 0 || max <= 0) return 0;
  return Math.min(4, Math.ceil((count * 4) / max));
}

const MONTH_SHORT = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** One column = one week; index 0 = Sunday. `null` cells pad the first
 * week so the first real day lands on its weekday row. */
export function buildContributionWeeks(
  days: UserContributionDay[],
): (UserContributionDay | null)[][] {
  const cells: (UserContributionDay | null)[] = [];
  if (days.length > 0) {
    const first = parseContributionDay(days[0].date);
    for (let i = 0; i < first.getUTCDay(); i++) cells.push(null);
    for (const d of days) cells.push(d);
  }
  const weeks: (UserContributionDay | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  return weeks;
}

/** Month label per week index — emitted where the week's first day enters
 * a new month (GitHub's label placement). */
export function contributionMonthLabels(
  weeks: (UserContributionDay | null)[][],
): { index: number; label: string }[] {
  const out: { index: number; label: string }[] = [];
  let lastMonth = -1;
  weeks.forEach((week, index) => {
    const first = week.find((d) => d !== null);
    if (!first) return;
    const month = parseContributionDay(first.date).getUTCMonth();
    if (month !== lastMonth) {
      out.push({ index, label: MONTH_SHORT[month] });
      lastMonth = month;
    }
  });
  return out;
}

/** GitHub tooltip phrasing for one cell. */
export function contributionDayTooltip(date: string, count: number): string {
  const when = parseContributionDay(date).toLocaleDateString("en-US", {
    month: "long",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
  if (count === 0) return `No contributions on ${when}`;
  return `${count} contribution${count === 1 ? "" : "s"} on ${when}`;
}

/** `YYYY-MM` → "November 2026". */
export function contributionMonthHeading(month: string): string {
  const [y, m] = month.split("-").map(Number);
  const name = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ][m - 1];
  return `${name ?? month} ${y}`;
}

/** GitHub's per-kind summary phrasing inside a monthly section. */
export function contributionSectionSummary(kind: string, total: number, repoCount: number): string {
  const repos = repoCount === 1 ? "repository" : "repositories";
  switch (kind) {
    case "commits":
      return `Created ${total} commit${total === 1 ? "" : "s"} in ${repoCount} ${repos}`;
    case "pull_requests":
      return `Opened ${total} pull request${total === 1 ? "" : "s"} in ${repoCount} ${repos}`;
    case "issues":
      return `Opened ${total} issue${total === 1 ? "" : "s"} in ${repoCount} ${repos}`;
    case "reviews":
      return `Reviewed ${total} pull request${total === 1 ? "" : "s"} in ${repoCount} ${repos}`;
    default:
      return `Created ${total} ${repos}`;
  }
}
