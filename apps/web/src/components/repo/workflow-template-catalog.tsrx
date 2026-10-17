import { Workflow } from "@octanejs/lucide";
import type { WorkflowTemplateSummary, WorkflowTemplatesListResponse } from "@oxidean/api-client";
import { memo, useState } from "octane";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";

type WorkflowTemplateCatalogProps = {
  owner: string;
  repo: string;
  data: WorkflowTemplatesListResponse | undefined;
  loading: boolean;
  error: string | null;
};

/** Sections in GitHub "Choose a workflow" order — language categories feed
 * the search filter only, they do not become sections. */
const HEADLINE_CATEGORIES = ["Deployment", "Continuous integration", "Automation"];

function provenanceLabel(t: WorkflowTemplateSummary): string {
  if (t.provenance === "builtin") return "Official";
  if (t.provenance === "instance") return "Instance";
  return t.source_label ?? "Org";
}

function configureHref(owner: string, repo: string, id: string): string {
  return `/${owner}/${repo}/new/.github/workflows?template=${encodeURIComponent(id)}`;
}

function matches(t: WorkflowTemplateSummary, q: string): boolean {
  if (!q) return true;
  return (
    t.name.toLowerCase().includes(q) ||
    t.description.toLowerCase().includes(q) ||
    t.categories.some((c) => c.toLowerCase().includes(q))
  );
}

const WorkflowTemplateCard = memo(function WorkflowTemplateCard(props: {
  owner: string;
  repo: string;
  template: WorkflowTemplateSummary;
}) @{
  const t = props.template;
  const extraActions = Math.max(0, t.unsupported_actions.length - 3);
  const unsupportedNote =
    t.unsupported_actions.length === 0
      ? ""
      : `Uses actions the bundled runner does not run yet: ${t.unsupported_actions
          .slice(0, 3)
          .join(", ")}${extraActions > 0 ? ` +${extraActions} more` : ""}`;

  <div
    data-testid={`workflow-template-card-${t.id}`}
    className="flex flex-col gap-3 rounded-lg border border-border bg-card p-4"
  >
    <div className="flex items-start gap-3">
      @if (t.icon_url) {
        <img
          src={t.icon_url as string}
          alt=""
          className="h-10 w-10 shrink-0 rounded-md"
          loading="lazy"
        />;
      } @else {
        <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-primary/15 text-primary">
          <Workflow aria-hidden size={20} />
        </span>;
      }
      <div className="min-w-0 flex-1">
        <p className="flex flex-wrap items-center gap-2 text-[14px] font-semibold text-foreground">
          <span className="truncate">{t.name as string}</span>
          <Badge variant="outline">{provenanceLabel(t) as string}</Badge>
        </p>
        <p className="mt-1 line-clamp-2 text-[13px] leading-snug text-muted-foreground">
          {t.description as string}
        </p>
      </div>
    </div>
    @if (t.unsupported_actions.length > 0) {
      <p
        className="text-[12px] text-muted-foreground"
        title={t.unsupported_actions.join(", ") as string}
      >
        {unsupportedNote as string}
      </p>;
    }
    @if (t.parse_error) {
      <p className="text-[12px] text-destructive/80">
        {"Not runnable here: " + (t.parse_error ?? "")}
      </p>;
    }
    <div className="mt-auto flex justify-end">
      <AppLink href={configureHref(props.owner, props.repo, t.id)}>
        <Button
          type="button"
          variant="outline"
          size="sm"
          data-testid={`workflow-template-configure-${t.id}`}
        >
          Configure
        </Button>
      </AppLink>
    </div>
  </div>;
});

function CatalogSection(props: {
  title: string;
  owner: string;
  repo: string;
  items: WorkflowTemplateSummary[];
}) @{
  <section aria-label={props.title} className="flex flex-col gap-3">
    <h2 className="text-[16px] font-semibold text-foreground">{props.title as string}</h2>
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      @for (const t of props.items; key t.id) {
        <WorkflowTemplateCard owner={props.owner} repo={props.repo} template={t} />;
      }
    </div>
  </section>;
}

/** GitHub "Choose a workflow" parity: suggested + headline-category card grids. */
export function WorkflowTemplateCatalog(props: WorkflowTemplateCatalogProps) @{
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();

  const templates = (props.data?.templates ?? []).filter((t) => matches(t, q));
  const suggested = (props.data?.suggested ?? []).filter((t) => matches(t, q));
  const sections = HEADLINE_CATEGORIES.map((cat) => ({
    title: cat,
    items: templates.filter((t) => t.categories.some((c) => c.toLowerCase() === cat.toLowerCase())),
  })).filter((s) => s.items.length > 0);
  const nothingShown = suggested.length === 0 && sections.length === 0;
  const blankHref = `/${props.owner}/${props.repo}/new/.github/workflows?filename=main.yml`;

  <div data-testid="workflow-template-catalog" className="flex flex-col gap-8">
    <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
      <div className="min-w-0">
        <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold text-foreground">
          Choose a workflow
        </h1>
        <p className="mt-1 max-w-2xl text-[14px] text-muted-foreground">
          Build, test, and deploy your code. Make code reviews, branch management, and issue
          triaging work the way you want. Select a workflow to get started.
        </p>
      </div>
      <AppLink
        href={blankHref}
        data-testid="workflow-template-blank"
        className="shrink-0 text-[14px] font-medium text-primary hover:underline"
      >
        {"set up a workflow yourself →" as string}
      </AppLink>
    </div>
    <Input
      type="search"
      value={query}
      onInput={(e) => setQuery((e.currentTarget as HTMLInputElement).value)}
      placeholder="Search workflows"
      aria-label="Search workflows"
      data-testid="workflow-template-search"
      className="max-w-md"
    />
    @if (props.loading) {
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        @for (const i of [0, 1, 2, 3, 4, 5]; key i) {
          <Skeleton className="h-32 rounded-lg" />;
        }
      </div>;
    } @else {
      @if (props.error) {
        <p className="text-[14px] text-destructive" role="alert">
          {props.error as string}
        </p>;
      } @else {
        @if (nothingShown) {
          <p className="py-8 text-center text-[14px] text-muted-foreground">
            No workflows match your search.
          </p>;
        } @else {
          <>
            @if (suggested.length > 0) {
              <CatalogSection
                title="Suggested for this repository"
                owner={props.owner}
                repo={props.repo}
                items={suggested}
              />;
            }
            @for (const section of sections; key section.title) {
              <CatalogSection
                title={section.title}
                owner={props.owner}
                repo={props.repo}
                items={section.items}
              />;
            }
          </>;
        }
      }
    }
  </div>;
}
