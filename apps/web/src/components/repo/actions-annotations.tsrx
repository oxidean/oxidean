import { useQuery } from "@octanejs/tanstack-query";
import { Info, OctagonX, TriangleAlert } from "@octanejs/lucide";
import type { ActionAnnotationPublic } from "@oxidean/api-client";
import { actionsJobAnnotationsQuery } from "@/lib/actions-queries";
import { cn } from "@/lib/utils";

/** `path:line` / `path:line-line` source location, GitHub annotation style. */
function annotationLocation(a: ActionAnnotationPublic): string {
  if (!a.path) return "";
  if (a.start_line != null && a.end_line != null && a.end_line !== a.start_line) {
    return `${a.path}#L${a.start_line}-L${a.end_line}`;
  }
  if (a.start_line != null) return `${a.path}#L${a.start_line}`;
  return a.path;
}

function levelTextClass(level: string): string {
  if (level === "error") return "text-red-600 dark:text-red-400";
  if (level === "warning") return "text-amber-700 dark:text-amber-400";
  return "text-muted-foreground";
}

function AnnotationIcon(props: { level: string }) @{
  const cls = levelTextClass(props.level);
  @if (props.level === "error") {
    <OctagonX aria-hidden size={14} className={cn("mt-0.5 shrink-0", cls)} />;
  } @else {
    @if (props.level === "warning") {
      <TriangleAlert aria-hidden size={14} className={cn("mt-0.5 shrink-0", cls)} />;
    } @else {
      <Info aria-hidden size={14} className={cn("mt-0.5 shrink-0", cls)} />;
    }
  }
}

/**
 * Structured `::error`/`::warning`/`::notice` workflow-command annotations for
 * the selected job — level icon, message, `file#L` location, optional title.
 * Hidden when the job produced none.
 */
export function ActionsAnnotations(props: {
  owner: string;
  name: string;
  runId: string;
  jobId: string;
}) @{
  const q = useQuery(actionsJobAnnotationsQuery(props.owner, props.name, props.runId, props.jobId));
  const annotations = (q.data?.annotations ?? []).map((a, i) => ({ ...a, k: i }));

  @if (q.isPending || annotations.length === 0) {
    <></>;
  } @else {
    <section
      className="mb-3 rounded-md border border-border"
      data-testid="actions-annotations"
      aria-label="Annotations"
    >
      <h3 className="border-b border-border bg-muted/40 px-3 py-1.5 text-[12px] font-medium text-muted-foreground">
        Annotations
      </h3>
      <ul className="divide-y divide-border">
        @for (const a of annotations; key a.k) {
          <li
            className="flex items-start gap-2 px-3 py-2 text-[13px]"
            data-testid={`actions-annotation-${a.annotation_level}`}
          >
            <AnnotationIcon level={a.annotation_level} />
            <div className="min-w-0 flex-1">
              @if (a.title) {
                <p className="font-medium text-foreground">{a.title as string}</p>;
              }
              <p
                className={cn(
                  "whitespace-pre-wrap break-words",
                  a.annotation_level === "notice"
                    ? "text-foreground"
                    : levelTextClass(a.annotation_level),
                )}
              >
                {a.message as string}
              </p>
              @if (annotationLocation(a)) {
                <p className="mt-0.5 font-mono text-[11px] text-muted-foreground">
                  {annotationLocation(a) as string}
                </p>;
              }
            </div>
          </li>;
        }
      </ul>
    </section>;
  }
}
