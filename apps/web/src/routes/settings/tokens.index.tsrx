import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { PatListItem, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { ChevronDown } from "@octanejs/lucide";
import { PatList } from "@/components/settings/pat-list";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { fetchPatList, fetchSessionMe } from "@/lib/ssr-auth";
import { cn } from "@/lib/utils";

export type TokensLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic; tokens: PatListItem[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function settingsTokensQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsTokens"],
    queryFn: async (): Promise<TokensLoaderData> => {
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
        const listRes = await fetchPatList();
        if (!listRes.ok) {
          return {
            kind: "error",
            message: listRes.error.message || "Could not load tokens.",
          };
        }
        return { kind: "ready", user: res.data, tokens: listRes.data };
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

/** Exported for tokens.integration.test.ts (GIT-11 / D-14). */
export function TokensPage() @{
  <AppPage>
    <TokensContent />
  </AppPage>;
}

export function TokensContent() @{
  const loaderData = useQuery(settingsTokensQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/tokens");
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

  const [tokens, setTokens] = useState<PatListItem[] | undefined>(() =>
    loaderData?.kind === "ready" ? loaderData.tokens : undefined,
  );

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      setPhase({ kind: "ready", user: loaderData.user });
      setTokens(loaderData.tokens);
      return;
    }
    if (loaderData.kind === "error") {
      setPhase({ kind: "error", message: loaderData.message });
    }
  }, [loaderData]);

  // Client fallback when loader payload is missing (tests / soft nav edge).
  useEffect(() => {
    if (loaderData) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await apiClient.auth.me();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign("/login?returnTo=/settings/tokens");
          return;
        }
        const listRes = await apiClient.pat.list();
        if (cancelled) return;
        if (!listRes.ok) {
          setPhase({
            kind: "error",
            message: listRes.error.message || "Could not load tokens.",
          });
          return;
        }
        setTokens(listRes.data);
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
      <AccountSettingsLayout active="tokens" data-testid="settings-tokens-page">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              Personal access tokens
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              Tokens authenticate git over HTTPS. Your account password is never accepted for git.
            </p>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger
              disabled={!verified}
              aria-label="Generate new token"
              className={cn(
                buttonVariants({ variant: "default" }),
                "inline-flex h-11 items-center gap-2 disabled:pointer-events-none disabled:opacity-50",
              )}
            >
              Generate new token
              <ChevronDown aria-hidden size={16} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-48">
              <DropdownMenuItem
                onClick={() => {
                  window.location.assign("/settings/tokens/new");
                }}
              >
                Classic token
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  window.location.assign("/settings/tokens/new/fine-grained");
                }}
              >
                Fine-grained token
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        @if (!verified) {
          <p className="mt-3 text-[14px] text-muted-foreground">
            Verify your email to create a token.
          </p>;
        }
        <PatList enabled={true} initialData={tokens} />
      </AccountSettingsLayout>;
    }
  }
}
