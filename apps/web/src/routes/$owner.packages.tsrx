import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Package } from "@octanejs/lucide";
import {
  packagesDeleteVersionMutationOptions,
  packagesListQueryOptions,
  type PackagePublic,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { AppPage } from "@/components/app-page";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { formatRelative } from "@/lib/relative-time";
import { PublishQuickstart } from "@/components/packages/publish-quickstart";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { DeleteVersionDialog } from "@/components/packages/delete-version-dialog";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** `/{owner}/packages` — registry package list (title via shell). */
export function OwnerPackagesPage() @{
  <AppPage>
    <OwnerPackagesContent />
  </AppPage>;
}

/** Owner packages list (D-PKG-11) — formats oci/npm/generic. */
export function OwnerPackagesContent() @{
  const pathname = usePathname();
  const owner = matchPath("/$owner/packages", pathname)?.owner ?? "";
  const qc = useQueryClient();
  const listQ = useQuery(packagesListQueryOptions(apiClient, { owner }));
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

  const packages = listQ.data?.packages ?? [];
  const loadError = listQ.isError ? String(listQ.error?.message || NETWORK_ERROR) : "";

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
