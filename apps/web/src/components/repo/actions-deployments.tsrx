import { useQuery } from "@octanejs/tanstack-query";
import { Rocket } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { List, ListRow } from "@/components/ui/list";
import { AppLink } from "@/components/ui/app-link";
import { actionsDeploymentsQuery } from "@/lib/actions-queries";
import { formatRelative } from "@/lib/relative-time";

/** Deployment records — run-scoped on the run page, repo-level in settings. */
export function ActionsDeployments(props: {
  owner: string;
  name: string;
  runId?: string;
  environment?: string;
}) @{
  const q = useQuery(
    actionsDeploymentsQuery(props.owner, props.name, {
      runId: props.runId,
      environment: props.environment,
    }),
  );
  const deployments = q.data?.deployments ?? [];

  @if (!q.isPending && deployments.length === 0) {
    <></>;
  } @else {
    <section className="mt-6" data-testid="actions-deployments" aria-label="Deployments">
      <h3 className="mb-2 flex items-center gap-2 text-[14px] font-semibold text-foreground">
        <Rocket aria-hidden size={15} className="text-muted-foreground" />
        Deployments
      </h3>
      <List>
        @for (const d of deployments; key d.id) {
          <ListRow className="gap-3">
            <div className="min-w-0 flex-1">
              <p className="flex flex-wrap items-center gap-2 text-[13px]">
                @if (d.run_id) {
                  <AppLink
                    href={`/${props.owner}/${props.name}/actions/${d.run_id}` as string}
                    className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                  >
                    {d.environment as string}
                  </AppLink>;
                } @else {
                  <span className="font-medium text-foreground">
                    {d.environment as string}
                  </span>;
                }
                <AppLink
                  href={`/${props.owner}/${props.name}/commit/${d.sha}` as string}
                  className="font-mono text-muted-foreground no-underline hover:text-primary hover:underline"
                >
                  {d.sha.slice(0, 7) as string}
                </AppLink>
                @if (d.ref) {
                  <span className="font-mono text-[12px] text-muted-foreground">
                    {d.ref.replace(/^refs\/(heads|tags)\//, "") as string}
                  </span>;
                }
              </p>
              @if (d.url) {
                {
                  /* raw-anchor-ok: deployment url is external (cloud console,
service endpoint) — not an app route. */
                }
                <a
                  href={d.url as string}
                  target="_blank"
                  rel="noreferrer"
                  className="mt-0.5 block truncate text-[12px] text-primary no-underline hover:underline"
                >
                  {d.url as string}
                </a>;
              }
            </div>
            <div className="flex shrink-0 flex-col items-end gap-1">
              <Badge
                variant={
                  (d.state ?? d.job_status) === "success"
                    ? "success"
                    : (d.state ?? d.job_status) === "failure" ||
                        (d.state ?? d.job_status) === "cancelled" ||
                        (d.state ?? d.job_status) === "error"
                      ? "destructive"
                      : "primary"
                }
              >
                {(d.state ?? d.job_status) as string}
              </Badge>
              @if (d.created_at) {
                <span className="text-[11px] text-muted-foreground">
                  {formatRelative(d.created_at) as string}
                </span>;
              }
            </div>
          </ListRow>;
        }
      </List>
    </section>;
  }
}
