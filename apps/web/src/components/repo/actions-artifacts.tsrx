import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { Archive, Download, Trash2 } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { actionsArtifactsQuery } from "@/lib/actions-queries";
import { formatRelative } from "@/lib/relative-time";
import { toastError } from "@/lib/toast";

/** Human-readable byte size ("1.4 MiB"). */
function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  const mib = kib / 1024;
  if (mib < 1024) return `${mib.toFixed(1)} MiB`;
  return `${(mib / 1024).toFixed(2)} GiB`;
}

/** Run artifacts — name, size, expiry, download + delete (writers). */
export function ActionsArtifacts(props: {
  owner: string;
  name: string;
  runId: string;
  canWrite: boolean;
}) @{
  const qc = useQueryClient();
  const q = useQuery(actionsArtifactsQuery(props.owner, props.name, props.runId));
  const artifacts = q.data?.artifacts ?? [];
  const delM = useMutation({
    mutationFn: async (artifactId: string) => {
      const res = await apiClient.repo.actions.deleteArtifact({
        owner: props.owner,
        name: props.name,
        artifact_id: artifactId,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    onSuccess: async () => {
      await qc.invalidateQueries({
        queryKey: ["repo", "actions", "artifacts", props.owner, props.name, props.runId],
      });
    },
    onError: (e) => toastError(e instanceof Error ? e.message : "Could not delete artifact."),
  });

  @if (!q.isPending && artifacts.length === 0) {
    <></>;
  } @else {
    <section className="mt-6" data-testid="actions-artifacts" aria-label="Artifacts">
      <h3 className="mb-2 flex items-center gap-2 text-[14px] font-semibold text-foreground">
        <Archive aria-hidden size={15} className="text-muted-foreground" />
        Artifacts
      </h3>
      <List>
        @for (const a of artifacts; key a.id) {
          <ListRow className="gap-3">
            <div className="min-w-0 flex-1">
              <p className="truncate font-mono text-[13px] text-foreground">
                {a.name as string}
              </p>
              <p className="text-[12px] text-muted-foreground">
                {humanSize(a.size_bytes) as string}
                @if (a.expires_at) {
                  <span>
                    {(" · expires " + formatRelative(a.expires_at)) as string}
                  </span>;
                }
                @if (a.created_at) {
                  <span>{(" · " + formatRelative(a.created_at)) as string}</span>;
                }
              </p>
            </div>
            <Badge variant="default" className="shrink-0 font-mono text-[10px]">
              {a.digest_sha256.slice(0, 8) as string}
            </Badge>
            {/* raw-anchor-ok: binary download endpoint — AppLink would
client-navigate instead of triggering the file save. */}
            <a
              href={
                `/api/repos/${props.owner}/${props.name}/actions/artifacts/${a.id}` as string
              }
              download
              data-testid={`actions-artifact-download-${a.name}`}
              className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-[12px] text-foreground no-underline hover:bg-muted/60"
            >
              <Download aria-hidden size={12} />
              Download
            </a>
            @if (props.canWrite) {
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="shrink-0 px-1.5 text-muted-foreground hover:text-destructive"
                aria-label={"Delete artifact " + a.name}
                title="Delete artifact"
                data-testid={`actions-artifact-delete-${a.name}`}
                disabled={delM.isPending}
                onClick={() => delM.mutate(a.id)}
              >
                <Trash2 aria-hidden size={13} />
              </Button>;
            }
          </ListRow>;
        }
      </List>
    </section>;
  }
}
