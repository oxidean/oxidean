import type { OAuthAppPublic, OAuthGrantPublic, UserPublic } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { oauthAppListQueryOptions } from "@oxidean/api-client";
import { OAuthAppCreateForm } from "@/components/settings/oauth-app-create-form";
import { OAuthAppItem } from "@/components/settings/oauth-app-item";
import { OAuthGrants } from "@/components/settings/oauth-grants";
import { OAuthSecretReveal } from "@/components/settings/oauth-secret-reveal";
import { AccountSettingsLayout } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { KeyRound } from "@octanejs/lucide";
import { apiClient } from "@/lib/api-client";
import { fetchOAuthAppList, fetchOAuthGrantList, fetchSessionMe } from "@/lib/ssr-auth";

export type ApplicationsLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic; apps: OAuthAppPublic[]; grants: OAuthGrantPublic[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function settingsApplicationsQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsApplications"],
    queryFn: async (): Promise<ApplicationsLoaderData> => {
      try {
        const res = await fetchSessionMe();
        if (!res.ok) {
          if (res.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: res.error.message || "Could not load session.",
          };
        }
        const [apps, grants] = await Promise.all([fetchOAuthAppList(), fetchOAuthGrantList()]);
        return {
          kind: "ready",
          user: res.data,
          apps: apps.ok ? apps.data : [],
          grants: grants.ok ? grants.data : [],
        };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** Exported for applications.integration.test.ts (API-03). */
export function ApplicationsPage() @{
  <AppPage>
    <ApplicationsContent />
  </AppPage>;
}

export function ApplicationsContent() @{
  const loaderData = useQuery(settingsApplicationsQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/applications");
    }
  }, [loaderData]);

  const initialApps = loaderData?.kind === "ready" ? loaderData.apps : undefined;
  const appsQuery = useQuery({
    ...oauthAppListQueryOptions(apiClient),
    enabled: loaderData?.kind !== "unauthenticated",
    ...(initialApps !== undefined ? { initialData: initialApps } : {}),
  });

  const [grants, setGrants] = useState<OAuthGrantPublic[]>(
    loaderData?.kind === "ready" ? loaderData.grants : [],
  );
  // loaderData arrives async (query) — sync grants once ready.
  useEffect(() => {
    if (loaderData?.kind === "ready") setGrants(loaderData.grants);
  }, [loaderData]);
  const [showCreate, setShowCreate] = useState(false);
  const [freshSecret, setFreshSecret] = useState("");

  async function refreshGrants() {
    const res = await apiClient.oauthApp.listGrants();
    if (res.ok) setGrants(res.data);
  }

  @if (loaderData?.kind === "error") {
    <AccountSettingsLayout active="applications">
      <p role="alert" className="text-[16px] text-destructive">
        {loaderData.message as string}
      </p>
    </AccountSettingsLayout>;
  } @else {
    <AccountSettingsLayout active="applications">
      <PageHeader
        title="OAuth applications"
        description="Third-party apps that can act on your behalf, and developer apps you own."
      />

      <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
        Authorized applications
      </h2>
      <OAuthGrants grants={grants} onRevoked={() => void refreshGrants()} />

      <div className="mt-10 flex items-center justify-between gap-3">
        <h2 className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
          Developer applications
        </h2>
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            setShowCreate(!showCreate);
            setFreshSecret("");
          }}
        >
          {(showCreate ? "Close" : "Register new application") as string}
        </Button>
      </div>
      <p className="mt-1 text-[14px] text-muted-foreground">
        Register an app to let external tools authenticate users with Oxidean.
      </p>

      @if (freshSecret) {
        <div className="mt-4">
          <OAuthSecretReveal secret={freshSecret} />
        </div>;
      }

      @if (showCreate) {
        <div className="mt-4">
          <OAuthAppCreateForm
            onCreated={(_app, secret) => {
              setFreshSecret(secret);
              setShowCreate(false);
            }}
          />
        </div>;
      }

      @if (appsQuery.isPending && initialApps === undefined) {
        <p className="mt-6 text-[14px] text-muted-foreground">Loading…</p>;
      } @else {
        @if ((appsQuery.data ?? []).length === 0) {
          <EmptyState
            className="mt-6"
            icon={<KeyRound aria-hidden size={22} />}
            title="No OAuth applications"
            description="Register an application to mint a client ID and secret."
          />;
        } @else {
          <List className="mt-6">
            @for (const app of appsQuery.data ?? []; key app.id) {
              <OAuthAppItem app={app} />;
            }
          </List>;
        }
      }
    </AccountSettingsLayout>;
  }
}
