import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Package } from "@octanejs/lucide";
import {
  packagesDeleteVersionMutationOptions,
  packagesListQueryOptions,
  type PackagePublic,
  type PackagesListResponse,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { fetchPackagesList } from "@/lib/ssr-repo";
import { PublishQuickstart } from "@/components/packages/publish-quickstart";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { DeleteVersionDialog } from "@/components/packages/delete-version-dialog";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export type OwnerPackagesLoaderData =
  | { kind: "error"; message: string }
  | { kind: "ready"; list: PackagesListResponse };

export const Route = createFileRoute("/$owner/packages")({
  component: OwnerPackagesPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `${String((params as { owner?: string }).owner ?? "")} packages · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<OwnerPackagesLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    try {
      const res = await fetchPackagesList({ data: { owner } });
      if (!res.ok) {
        return {
          kind: "error",
          message: res.error.message || "Could not load packages.",
        };
      }
      return { kind: "ready", list: res.data };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Owner packages list (D-PKG-11) — formats oci/npm/generic. */
export function OwnerPackagesPage() @{
  const params = useParams({ from: "/$owner/packages" }) as { owner?: string };
  const owner = String(params.owner ?? "");
  const routeLoader = useLoaderData({ from: "/$owner/packages" }) as
    | OwnerPackagesLoaderData
    | undefined;
  const qc = useQueryClient();
  const listQ = useQuery({
    ...packagesListQueryOptions(apiClient, { owner }),
    initialData: routeLoader?.kind === "ready" ? routeLoader.list : undefined,
  });
  const [pendingDelete, setPendingDelete] = useState<{
    pkg: PackagePublic;
    version: string;
  } | null>(null);
  const delM = useMutation({
    ...packagesDeleteVersionMutationOptions(apiClient),
    onSuccess: async () => {
      setPendingDelete(null);
      await qc.invalidateQueries({ queryKey: ["packages", "list"] });
    },
  });

  const packages =
    listQ.data?.packages ?? (routeLoader?.kind === "ready" ? routeLoader.list.packages : []);
  const loadError =
    routeLoader?.kind === "error"
      ? routeLoader.message
      : listQ.isError
        ? String(listQ.error?.message || NETWORK_ERROR)
        : "";

  @{
    <div className="mx-auto max-w-6xl px-4 py-12" data-testid="owner-packages">
      <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold text-foreground">
        Packages
      </h1>
      <p className="mt-1 text-[16px] text-muted-foreground">
        {"Registry packages for @" + (owner as string)}
      </p>
      @if (loadError) {
        <p className="mt-8 text-destructive" role="alert">
          {loadError as string}
        </p>;
      } @else if (listQ.isPending) {
        <p className="mt-8 text-muted-foreground">Loading packages…</p>;
      } @else {
        @if (packages.length === 0) {
          <>
            <EmptyState
              className="mt-8"
              icon={<Package aria-hidden size={22} />}
              title="No packages published yet"
              description="Publish your first package with a package client, then it shows up here."
            />
            <div className="mx-auto max-w-2xl">
              <PublishQuickstart owner={owner} />
            </div>
          </>;
        } @else {
          <List className="mt-8">
            @for (const pkg of packages; key pkg.id) {
              <ListRow className="items-start gap-3 py-3">
                <Package
                  aria-hidden
                  size={16}
                  className="mt-0.5 shrink-0 text-muted-foreground"
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[15px] font-semibold text-foreground">
                      {pkg.name as string}
                    </span>
                    <Badge
                      variant="outline"
                      className="uppercase"
                      data-format={pkg.format as string}
                    >
                      {pkg.format as string}
                    </Badge>
                    <Badge variant="outline">{pkg.visibility as string}</Badge>
                  </div>
                  <ul className="mt-2 space-y-1 text-[14px]">
                    @for (const v of pkg.versions; key v.version) {
                      <li className="flex items-center gap-3">
                        <AppLink
                          className="text-foreground underline-offset-4 hover:underline"
                          href={`/${owner}/packages#${pkg.name}@${v.version}`}
                        >
                          {(pkg.name + "@" + v.version) as string}
                        </AppLink>
                        <span
                          className="text-[12px] text-muted-foreground"
                          title={v.created_at as string}
                        >
                          {formatRelative(v.created_at) as string}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-8 text-[13px]"
                          onClick={() =>
                            setPendingDelete({ pkg, version: v.version })
                          }
                        >
                          Delete
                        </Button>
                      </li>;
                    }
                  </ul>
                </div>
              </ListRow>;
            }
          </List>;
        }
      }
      @if (pendingDelete) {
        <DeleteVersionDialog
          confirmLabel={(pendingDelete.pkg.name + "@" + pendingDelete.version) as string}
          onCancel={() => setPendingDelete(null)}
          onConfirm={(confirm) => {
            delM.mutate({
              package_id: pendingDelete.pkg.id,
              version: pendingDelete.version,
              confirm,
            });
          }}
        />;
      }
    </div>;
    };
}
