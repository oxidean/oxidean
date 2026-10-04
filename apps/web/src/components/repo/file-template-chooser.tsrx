import { FileText } from "@octanejs/lucide";
import type { RepoFileTemplate } from "@oxidean/api-client";
import { cn } from "@/lib/utils";

export type FileTemplateChooserProps = {
  /** Templates from `repo.templates.list` (issues or pulls). */
  templates: RepoFileTemplate[];
  /** Lowercase noun for copy — "issue" or "pull request". */
  kindLabel: string;
  /** `null` = the blank option was picked. */
  onPick: (template: RepoFileTemplate | null) => void;
  /** Page-scoped test id (`issue-template-chooser` / `pull-template-chooser`). */
  testId: string;
  /** Test id for the blank escape hatch (`issue-template-blank`, …). */
  blankTestId: string;
};

/**
 * Multi-template chooser for issue/PR create pages (COL-02). Mirrors the
 * GitHub "get started" screen: one card per template (name + about), plus a
 * blank escape hatch. Rendered only when the repo has >1 template.
 */
export function FileTemplateChooser({
  templates,
  kindLabel,
  onPick,
  testId,
  blankTestId,
}: FileTemplateChooserProps) @{
  <div data-testid={testId} className="flex max-w-2xl flex-col gap-4">
    <p className="text-[14px] text-muted-foreground">
      {"Get started with a template, or open a blank " as string}
      {kindLabel as string}
      {"." as string}
    </p>
    <div role="group" aria-label={`${kindLabel} templates`} className="flex flex-col gap-2">
      @for (const t of templates; key t.filename) {
        <button
          type="button"
          data-template-filename={t.filename as string}
          className={cn(
            "flex w-full items-start gap-3 rounded-md border border-border px-4 py-3 text-left outline-none transition-colors",
            "hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring",
          )}
          onClick={() => onPick(t)}
        >
          <FileText aria-hidden size={18} className="mt-0.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1">
            <span className="block text-[14px] font-semibold text-foreground">
              {t.name as string}
            </span>
            <span className="mt-0.5 block text-[13px] leading-snug text-muted-foreground">
              {(t.description || t.filename) as string}
            </span>
          </span>
        </button>;
      }
      <button
        type="button"
        data-testid={blankTestId}
        className="mt-1 self-start text-[14px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
        onClick={() => onPick(null)}
      >
        {"Open a blank " as string}
        {kindLabel as string}
      </button>
    </div>
  </div>;
}
