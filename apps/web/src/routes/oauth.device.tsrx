import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import type { OAuthDeviceInfo } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AppPage } from "@/components/app-page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { useSearchString } from "@/lib/pathname.tsrx";
import { fetchSessionMe } from "@/lib/ssr-auth";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type DeviceSearch = {
  user_code?: string;
};

export type DeviceLoaderData = { kind: "unauthenticated"; returnTo: string } | { kind: "ready" };

const SCOPE_LABELS: Record<string, string> = {
  "read:user": "Read your profile",
  "user:email": "View your email address",
  repo: "Read and write your repositories over HTTPS",
  "package:read": "Download packages you can access",
  "package:write": "Publish packages as you",
  "package:delete": "Delete packages as you",
};

function parseDeviceSearch(params: URLSearchParams): DeviceSearch {
  return { user_code: params.get("user_code") ?? undefined };
}

function deviceQueryOptions(searchStr: string) {
  return queryOptions({
    queryKey: ["oauth", "device", searchStr],
    queryFn: async (): Promise<DeviceLoaderData> => {
      const returnTo = `/oauth/device${searchStr}`;
      try {
        const me = await fetchSessionMe();
        if (!me.ok) {
          return { kind: "unauthenticated", returnTo };
        }
        return { kind: "ready" };
      } catch {
        return { kind: "unauthenticated", returnTo };
      }
    },
    retry: false,
    staleTime: 0,
  });
}

/** `/oauth/device` — RFC 8628 approval page for `ox auth login --web` (title on the Astro shell). */
export function OAuthDevicePage() @{
  <AppPage>
    <OAuthDeviceContent />
  </AppPage>;
}

type Step =
  | { kind: "enter" }
  | { kind: "loading" }
  | { kind: "ready"; info: OAuthDeviceInfo }
  | { kind: "done"; approved: boolean }
  | { kind: "error"; message: string };

function OAuthDeviceContent() @{
  const searchStr = useSearchString();
  const search = parseDeviceSearch(new URLSearchParams(searchStr));
  const deviceQ = useQuery(deviceQueryOptions(searchStr));
  const loaderData = deviceQ.data;
  const [step, setStep] = useState<Step>({ kind: "enter" });
  const [code, setCode] = useState(search.user_code ?? "");
  const [pending, setPending] = useState<"approve" | "deny" | "lookup" | null>(null);

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=" + encodeURIComponent(loaderData.returnTo));
    }
  }, [loaderData]);

  async function lookup(raw: string) {
    const user_code = raw.trim();
    if (!user_code || pending) return;
    setPending("lookup");
    setStep({ kind: "loading" });
    try {
      const res = await apiClient.oauth.deviceInfo({ user_code });
      setPending(null);
      if (!res.ok) {
        setStep({
          kind: "error",
          message: res.error.message || "No pending sign-in for that code.",
        });
        return;
      }
      setStep({ kind: "ready", info: res.data });
    } catch {
      setPending(null);
      setStep({ kind: "error", message: NETWORK_ERROR });
    }
  }

  useEffect(() => {
    if (loaderData?.kind === "ready" && search.user_code) {
      void lookup(search.user_code);
    }
  }, [loaderData?.kind]);

  async function decide(approve: boolean) {
    if (pending || step.kind !== "ready") return;
    setPending(approve ? "approve" : "deny");
    try {
      const res = await apiClient.oauth.deviceAuthorize({
        user_code: step.info.user_code,
        approve,
      });
      if (!res.ok) {
        setStep({
          kind: "error",
          message: res.error.message || "Couldn't complete authorization.",
        });
        setPending(null);
        return;
      }
      setStep({ kind: "done", approved: approve });
      setPending(null);
    } catch {
      setStep({ kind: "error", message: NETWORK_ERROR });
      setPending(null);
    }
  }

  @if (loaderData?.kind === "unauthenticated") {
    <div className="mx-auto flex min-h-[60vh] w-full max-w-xl flex-col justify-center px-4 py-10">
      <p className="text-[14px] text-muted-foreground">Redirecting to sign in…</p>
    </div>;
  } @else {
    <div className="mx-auto flex min-h-[60vh] w-full max-w-xl flex-col justify-center px-4 py-10">
      @if (step.kind === "done") {
        <div className="rounded-md border border-border bg-card p-6">
          <h1 className="font-[family-name:var(--font-display)] text-[22px] font-semibold text-foreground">
            {(step.approved ? "CLI sign-in authorized" : "Sign-in denied") as string}
          </h1>
          <p className="mt-2 text-[14px] text-muted-foreground">
            {
              (step.approved
                ? "You can return to your terminal — ox is now signed in."
                : "The pending sign-in was denied. You can close this tab.") as string
            }
          </p>
        </div>;
      } @else {
        <div className="rounded-md border border-border bg-card p-6">
          <h1 className="font-[family-name:var(--font-display)] text-[22px] font-semibold text-foreground">
            Authorize CLI sign-in
          </h1>
          <p className="mt-2 text-[14px] text-muted-foreground">
            Enter the one-time code shown by ox auth login --web, or confirm the code that
            brought you here.
          </p>
          @if (step.kind === "ready") {
            <>
              <p className="mt-4 break-all font-mono text-[15px] font-semibold tracking-wide text-foreground">
                {step.info.user_code as string}
              </p>
              <p className="mt-4 text-[13px] font-semibold uppercase tracking-wide text-muted-foreground">
                The CLI will be able to
              </p>
              <ul className="mt-2 flex flex-col gap-2">
                @for (const scope of step.info.scopes; key scope) {
                  <li className="flex items-center gap-2 text-[14px] text-foreground">
                    <Badge variant="outline">{scope as string}</Badge>
                    <span>
                      {(SCOPE_LABELS[scope] ?? "Use this permission") as string}
                    </span>
                  </li>;
                }
              </ul>
              <div className="mt-6 flex flex-wrap gap-3">
                <Button
                  type="button"
                  disabled={pending !== null}
                  onClick={() => void decide(true)}
                  data-testid="oauth-device-approve"
                >
                  {(pending === "approve" ? "Authorizing…" : "Authorize ox") as string}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  disabled={pending !== null}
                  onClick={() => void decide(false)}
                  data-testid="oauth-device-deny"
                >
                  {(pending === "deny" ? "Denying…" : "Deny") as string}
                </Button>
              </div>
            </>;
          } @else {
            <>
              @if (step.kind === "error") {
                <p role="alert" className="mt-4 text-[14px] text-destructive">
                  {step.message as string}
                </p>;
              }
              <form
                method="post"
                action="#"
                className="mt-4 flex items-center gap-3"
                onSubmit={(e: Event) => {
                  e.preventDefault();
                  void lookup(code);
                }}
              >
                <Input
                  value={code}
                  onInput={(e: Event) => setCode((e.target as HTMLInputElement).value)}
                  placeholder="XXXX-XXXX"
                  autoComplete="off"
                  data-testid="oauth-device-code"
                />
                <Button type="submit" disabled={pending !== null || !code.trim()}>
                  {(pending === "lookup" ? "Checking…" : "Continue") as string}
                </Button>
              </form>
              @if (step.kind === "loading") {
                <p className="mt-3 text-[13px] text-muted-foreground">
                  Checking the code…
                </p>;
              }
            </>;
          }
        </div>;
      }
    </div>;
  }
}
