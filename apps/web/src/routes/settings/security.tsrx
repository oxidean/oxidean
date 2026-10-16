import { twoFactorStatusQueryOptions, type TwoFactorStatusPublic } from "@oxidean/api-client";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import { PasskeySection } from "@/components/settings/passkey-section";
import { RecoveryCodesView } from "@/components/settings/recovery-codes-view";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { TotpEnrollFlow } from "@/components/settings/totp-enroll-flow";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";
import { useEffect, useState } from "octane";

const NETWORK_ERROR = networkErrorMessage();

type SecurityLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; status: TwoFactorStatusPublic };

function settingsSecurityQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsSecurity"],
    queryFn: async (): Promise<SecurityLoaderData> => {
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
        const statusRes = await apiClient.twoFactor.status();
        if (!statusRes.ok) {
          return {
            kind: "error",
            message: statusRes.error.message || "Could not load security settings.",
          };
        }
        return { kind: "ready", status: statusRes.data };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

type TotpPanel = "closed" | "enroll" | "disable";

/** Exported for security.integration.test.ts (#130). */
export function SecuritySettingsPage() @{
  <AppPage>
    <SecurityContent />
  </AppPage>;
}

function SecurityContent() @{
  const queryClient = useQueryClient();
  const loaderData = useQuery(settingsSecurityQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/security");
    }
  }, [loaderData]);

  const statusQ = useQuery({
    ...twoFactorStatusQueryOptions(apiClient),
    enabled: loaderData?.kind === "ready" || loaderData === undefined,
    ...(loaderData?.kind === "ready" ? { initialData: loaderData.status } : {}),
  });
  const status = statusQ.data;

  const [totpPanel, setTotpPanel] = useState<TotpPanel>("closed");
  const [disablePending, setDisablePending] = useState(false);
  const [regenCodes, setRegenCodes] = useState<string[] | null>(null);
  const [regenOpen, setRegenOpen] = useState(false);
  const [regenPending, setRegenPending] = useState(false);

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: ["twoFactor", "status"] });
  }

  async function disableTotp() {
    setDisablePending(true);
    try {
      const res = await apiClient.twoFactor.totpDisable();
      if (!res.ok) {
        toastError(rpcErrorMessage(res.error));
        setDisablePending(false);
        return;
      }
      setTotpPanel("closed");
      setDisablePending(false);
      toastSuccess("Authenticator app removed.");
      await refresh();
    } catch {
      toastError(NETWORK_ERROR);
      setDisablePending(false);
    }
  }

  async function regenerateCodes() {
    setRegenPending(true);
    try {
      const res = await apiClient.twoFactor.recoveryRegenerate();
      if (!res.ok) {
        toastError(rpcErrorMessage(res.error));
        setRegenPending(false);
        return;
      }
      setRegenCodes(res.data.recovery_codes);
      setRegenOpen(false);
      setRegenPending(false);
      await refresh();
    } catch {
      toastError(NETWORK_ERROR);
      setRegenPending(false);
    }
  }

  const phase: "loading" | "error" | "ready" =
    loaderData?.kind === "error"
      ? "error"
      : status
        ? "ready"
        : statusQ.isError
          ? "error"
          : loaderData?.kind === "unauthenticated"
            ? "loading"
            : "loading";

  @if (phase === "loading") {
    <></>;
  } @else {
    @if (phase === "error") {
      <SettingsShell bare>
        <p role="alert" className="text-[16px] text-destructive">
          {
            (loaderData?.kind === "error"
              ? loaderData.message
              : "Could not load security settings.") as string
          }
        </p>
      </SettingsShell>;
    } @else {
      <AccountSettingsLayout active="security" data-testid="settings-security-page">
        <div className="min-w-0">
          <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Security
          </h1>
          <p className="mt-2 text-[16px] text-muted-foreground">
            Two-factor methods, recovery codes, and passkeys for your account.
          </p>
        </div>

        @if (status?.enroll_required) {
          <div
            role="status"
            className="mt-6 rounded-md border border-secondary bg-secondary/10 p-4 text-[14px] text-foreground"
          >
            This instance or an organization you belong to requires two-factor authentication.
            Enroll a method below — some actions stay limited until you do.
          </div>;
        }

        <section className="mt-10">
          <h2 className="text-[18px] font-semibold text-foreground">Two-factor authentication</h2>
          <p className="mt-1 text-[14px] text-muted-foreground">
            A second factor keeps your account safe even if your password leaks.
          </p>

          <div className="mt-4 overflow-hidden rounded-lg border border-border bg-card">
            <div className="flex flex-wrap items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="text-[14px] font-medium text-foreground">Authenticator app</p>
                <p className="text-[13px] text-muted-foreground">
                  {
                    (status?.totp_enabled
                      ? "Configured — codes from your app are required at sign-in."
                      : "TOTP codes from 1Password, Authy, Google Authenticator, and similar.") as string
                  }
                </p>
              </div>
              @if (status?.totp_enabled) {
                <>
                  <span className="text-[13px] font-medium text-secondary">Enabled</span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setTotpPanel("disable")}
                  >
                    Disable
                  </Button>
                </>;
              } @else {
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setTotpPanel(totpPanel === "enroll" ? "closed" : "enroll")}
                >
                  {(totpPanel === "enroll" ? "Close" : "Set up") as string}
                </Button>;
              }
            </div>
            @if (totpPanel === "enroll" && !status?.totp_enabled) {
              <div className="border-t border-border px-4 py-4">
                <TotpEnrollFlow
                  onFinished={() => {
                    setTotpPanel("closed");
                    void refresh();
                  }}
                  onCancel={() => setTotpPanel("closed")}
                />
              </div>;
            }
          </div>
        </section>

        <section className="mt-10">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h2 className="text-[18px] font-semibold text-foreground">Passkeys</h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                Sign in with Face ID, Touch ID, Windows Hello, or a hardware security key.
              </p>
            </div>
          </div>
          <div className="mt-4">
            <PasskeySection passkeys={status?.passkeys ?? []} onChanged={() => void refresh()} />
          </div>
        </section>

        <section className="mt-10">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h2 className="text-[18px] font-semibold text-foreground">Recovery codes</h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                One-time codes that get you back in if you lose every factor.
              </p>
            </div>
            @if (
              (status?.recovery_codes_remaining ?? 0) > 0 ||
              status?.totp_enabled ||
              (status?.passkeys.length ?? 0) > 0
            ) {
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setRegenOpen(true)}
              >
                Generate new codes
              </Button>;
            }
          </div>
          <div className="mt-4">
            @if (regenCodes) {
              <RecoveryCodesView
                codes={regenCodes}
                onDone={() => setRegenCodes(null)}
                doneLabel="Done"
              />;
            } @else {
              <p className="text-[14px] text-muted-foreground">
                {
                  (status?.totp_enabled || (status?.passkeys.length ?? 0) > 0
                    ? `${status?.recovery_codes_remaining ?? 0} unused codes remaining.`
                    : "Recovery codes appear after you enroll a two-factor method.") as string
                }
              </p>;
            }
          </div>
        </section>

        <AlertDialog
          open={totpPanel === "disable"}
          onOpenChange={(open) => {
            if (!open) setTotpPanel("closed");
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Disable authenticator app?</AlertDialogTitle>
              <AlertDialogDescription className="break-words whitespace-normal">
                You will no longer need an app code to sign in. Recovery codes and passkeys are not
                affected.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={disablePending}>Keep app</AlertDialogCancel>
              <Button
                type="button"
                variant="destructive"
                disabled={disablePending}
                onClick={() => void disableTotp()}
              >
                {(disablePending ? "Disabling…" : "Disable app") as string}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog
          open={regenOpen}
          onOpenChange={(open) => {
            if (!open) setRegenOpen(false);
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Generate new recovery codes?</AlertDialogTitle>
              <AlertDialogDescription className="break-words whitespace-normal">
                All existing recovery codes stop working immediately. Make sure you can save the new
                set.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={regenPending}>Keep current codes</AlertDialogCancel>
              <Button type="button" disabled={regenPending} onClick={() => void regenerateCodes()}>
                {(regenPending ? "Generating…" : "Generate codes") as string}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </AccountSettingsLayout>;
    }
  }
}
