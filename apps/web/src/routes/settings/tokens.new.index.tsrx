import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AuthShell } from "@/components/auth-shell";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { PatClassicForm } from "@/components/settings/pat-classic-form";
import { PatReveal } from "@/components/settings/pat-reveal";
import { buttonVariants } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

type TokensNewLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function settingsTokensNewQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsTokensNew"],
    queryFn: async (): Promise<TokensNewLoaderData> => {
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
        return { kind: "ready", user: res.data };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

type Phase =
  | { kind: "loading" }
  | { kind: "ready"; user: UserPublic }
  | { kind: "error"; message: string };

/** Exported for tokens.integration.test.ts (GIT-11 / D-05 / D-15 / D-25). */
export function TokensNewPage() @{
  <AppPage>
    <TokensNewContent />
  </AppPage>;
}

function TokensNewContent() @{
  const loaderData = useQuery(settingsTokensNewQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/tokens/new");
    }
  }, [loaderData]);

  const [phase, setPhase] = useState<Phase>(() => {
    if (loaderData?.kind === "ready") {
      return { kind: "ready", user: loaderData.user };
    }
    if (loaderData?.kind === "error") {
      return { kind: "error", message: loaderData.message };
    }
    if (loaderData?.kind === "unauthenticated") {
      return { kind: "loading" };
    }
    return { kind: "loading" };
  });

  const [plaintext, setPlaintext] = useState<string | null>(null);

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      setPhase({ kind: "ready", user: loaderData.user });
      return;
    }
    if (loaderData.kind === "error") {
      setPhase({ kind: "error", message: loaderData.message });
    }
  }, [loaderData]);

  useEffect(() => {
    if (loaderData) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await apiClient.auth.me();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign("/login?returnTo=/settings/tokens/new");
          return;
        }
        setPhase({ kind: "ready", user: res.data });
      } catch {
        if (cancelled) return;
        setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loaderData]);

  const verified = phase.kind === "ready" ? phase.user.email_verified === true : false;
  const showWall = phase.kind === "ready" && !verified;

  @if (phase.kind === "loading") {
    <></>;
  } @else {
    @if (phase.kind === "error") {
      <SettingsShell bare>
        <p role="alert" className="text-[16px] text-destructive">
          {phase.message as string}
        </p>
      </SettingsShell>;
    } @else {
      @if (showWall) {
        <AuthShell
          title="Verify your email"
          support="Verify your email before creating a personal access token."
        >
          <AppLink href="/verify" className={cn(buttonVariants({ variant: "default" }))}>
            Verify email
          </AppLink>
        </AuthShell>;
      } @else {
        <AccountSettingsLayout active="tokens">
          <AppLink
            href="/settings/tokens"
            className="text-[14px] text-muted-foreground no-underline hover:text-foreground"
          >
            ← Back
          </AppLink>
          @if (plaintext) {
            <div className="mt-6">
              <PatReveal token={plaintext as string} />
            </div>;
          } @else {
            <>
              <h1 className="mt-6 font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
                New classic token
              </h1>
              <PatClassicForm
                onCreated={(token) => {
                  setPlaintext(token);
                }}
              />
            </>;
          }
        </AccountSettingsLayout>;
      }
    }
  }
}
