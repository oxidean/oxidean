import { createFileRoute, useLoaderData, useSearch } from "@octanejs/tanstack-router";
import type { OAuthAuthorizeInfo, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { fetchOAuthAuthorizeInfo, fetchSessionMe } from "@/lib/ssr-auth";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type ConsentSearch = {
  client_id?: string;
  redirect_uri?: string;
  response_type?: string;
  scope?: string;
  state?: string;
};

export type ConsentLoaderData =
  | { kind: "unauthenticated"; returnTo: string }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic; info: OAuthAuthorizeInfo };

const SCOPE_LABELS: Record<string, string> = {
  "read:user": "Read your profile",
  "user:email": "View your email address",
  repo: "Read and write your repositories over HTTPS",
  "package:read": "Download packages you can access",
  "package:write": "Publish and delete packages as you",
};

export const Route = createFileRoute("/oauth/consent")({
  component: OAuthConsentPage,
  validateSearch: (search: Record<string, unknown>): ConsentSearch => ({
    client_id: typeof search.client_id === "string" ? search.client_id : undefined,
    redirect_uri: typeof search.redirect_uri === "string" ? search.redirect_uri : undefined,
    response_type: typeof search.response_type === "string" ? search.response_type : undefined,
    scope: typeof search.scope === "string" ? search.scope : undefined,
    state: typeof search.state === "string" ? search.state : undefined,
  }),
  head: () => ({ meta: [{ title: "Authorize application · Oxidean" }] }),
  loader: async ({ location }): Promise<ConsentLoaderData> => {
    const search = location.search as ConsentSearch;
    const returnTo = `/oauth/authorize${location.searchStr ?? ""}`;
    try {
      const me = await fetchSessionMe();
      if (!me.ok) {
        if (me.error.code === "auth.unauthenticated") {
          return { kind: "unauthenticated", returnTo };
        }
        return { kind: "error", message: me.error.message || "Could not load session." };
      }
      const info = await fetchOAuthAuthorizeInfo({
        data: {
          client_id: search.client_id ?? "",
          redirect_uri: search.redirect_uri,
          scope: search.scope,
        },
      });
      if (!info.ok) {
        return { kind: "error", message: info.error.message || "Invalid authorization request." };
      }
      return { kind: "ready", user: me.data, info: info.data };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Consent screen for `GET /oauth/authorize` (API-03): approve or deny a grant. */
export function OAuthConsentPage() @{
  const loaderData = useLoaderData({ from: "/oauth/consent" }) as ConsentLoaderData | undefined;
  const search = useSearch({ strict: false }) as ConsentSearch;
  const [pending, setPending] = useState<"approve" | "deny" | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=" + encodeURIComponent(loaderData.returnTo));
    }
  }, [loaderData]);

  async function decide(approve: boolean) {
    if (pending || loaderData?.kind !== "ready") return;
    setError("");
    setPending(approve ? "approve" : "deny");
    try {
      const res = await apiClient.oauthApp.authorize({
        client_id: loaderData.info.client_id,
        redirect_uri: loaderData.info.redirect_uri,
        scope: search.scope,
        state: search.state,
        approve,
      });
      if (!res.ok) {
        setError(res.error.message || "Couldn't complete authorization.");
        setPending(null);
        return;
      }
      window.location.assign(res.data.redirect_to);
    } catch {
      setError(NETWORK_ERROR);
      setPending(null);
    }
  }

  @if (loaderData?.kind === "unauthenticated") {
    <main className="mx-auto flex min-h-[60vh] w-full max-w-xl flex-col justify-center px-4 py-10">
      <p className="text-[14px] text-muted-foreground">Redirecting to sign in…</p>
    </main>;
  } @else {
    <main className="mx-auto flex min-h-[60vh] w-full max-w-xl flex-col justify-center px-4 py-10">
      @if (loaderData?.kind === "error") {
        <p role="alert" className="text-[16px] text-destructive">
          {loaderData.message as string}
        </p>;
      } @else {
        @if (loaderData) {
          <div className="rounded-md border border-border bg-card p-6">
            <h1 className="font-[family-name:var(--font-display)] text-[22px] font-semibold text-foreground">
              {"Authorize " + loaderData.info.app_name}
            </h1>
            <p className="mt-2 text-[14px] text-muted-foreground">
              {
                (loaderData.info.owner_username
                  ? loaderData.info.app_name +
                    " (by " +
                    loaderData.info.owner_username +
                    ") wants to act on your behalf."
                  : loaderData.info.app_name + " wants to act on your behalf.") as string
              }
            </p>
            <p className="mt-4 text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
              This application will be able to
            </p>
            <ul className="mt-2 flex flex-col gap-2">
              @for (const scope of loaderData.info.scopes; key scope) {
                <li className="flex items-center gap-2 text-[14px] text-foreground">
                  <Badge variant="outline">{scope as string}</Badge>
                  <span>
                    {(SCOPE_LABELS[scope] ?? "Use this permission") as string}
                  </span>
                </li>;
              }
            </ul>
            <p className="mt-4 break-all font-mono text-[12px] text-muted-foreground">
              {"Redirecting to " + loaderData.info.redirect_uri}
            </p>
            @if (error) {
              <p role="alert" className="mt-4 text-[14px] text-destructive">
                {error as string}
              </p>;
            }
            <div className="mt-6 flex flex-wrap gap-3">
              <Button
                type="button"
                disabled={pending !== null}
                onClick={() => void decide(true)}
                data-testid="oauth-consent-approve"
              >
                {(pending === "approve" ? "Authorizing…" : "Authorize") as string}
              </Button>
              <Button
                type="button"
                variant="outline"
                disabled={pending !== null}
                onClick={() => void decide(false)}
                data-testid="oauth-consent-deny"
              >
                {(pending === "deny" ? "Denying…" : "Deny") as string}
              </Button>
            </div>
          </div>;
        }
      }
    </main>;
  }
}
