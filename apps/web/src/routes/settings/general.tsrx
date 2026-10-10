import { AppPage } from "@/components/app-page";
import { useForm } from "@octanejs/tanstack-form";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { ThemeSelect } from "@/components/theme-select";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { clearSessionQueries, setAuthMeCache } from "@/lib/session-queries";
import { syncSessionPresenceHint } from "@/lib/session-hint";
import { fetchUserGetProfile } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";

export type GeneralLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const BIO_MAX = 160;
const LOGIN_RETURN = "/login?returnTo=/settings/general";

function settingsGeneralQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsGeneral"],
    queryFn: async (): Promise<GeneralLoaderData> => {
      try {
        const res = await fetchUserGetProfile();
        if (!res.ok) {
          if (res.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: res.error.message || "Could not load settings.",
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

/** Account General: theme, default branch, session logout. */
export function GeneralPage() @{
  <AppPage>
    <GeneralContent />
  </AppPage>;
}

export function GeneralContent() @{
  const queryClient = useQueryClient();
  const loaderData = useQuery(settingsGeneralQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign(LOGIN_RETURN);
    }
  }, [loaderData]);

  const initialUser = loaderData?.kind === "ready" ? loaderData.user : null;

  const [phase, setPhase] = useState<Phase>(() => {
    if (loaderData?.kind === "ready") return { kind: "ready", user: loaderData.user };
    if (loaderData?.kind === "error") {
      return { kind: "error", message: loaderData.message };
    }
    if (loaderData?.kind === "unauthenticated") {
      return { kind: "loading" };
    }
    return { kind: "loading" };
  });
  const [branchPending, setBranchPending] = useState(false);
  const [logoutPending, setLogoutPending] = useState(false);
  const [confirmLogoutAll, setConfirmLogoutAll] = useState(false);

  const branchForm = useForm({
    defaultValues: {
      default_branch: initialUser?.default_branch?.trim() || "main",
    },
    onSubmit: async ({ value }) => {
      if (phase.kind !== "ready") return;
      setBranchPending(true);
      const user = phase.user;
      try {
        const res = await apiClient.user.updateProfile({
          display_name: user.display_name?.trim() ?? "",
          username: user.username?.trim() ?? "",
          bio: (user.bio ?? "").slice(0, BIO_MAX),
          default_branch: value.default_branch.trim() || "main",
        });
        if (!res.ok) {
          toastError(res.error.message || "Could not save default branch. Try again.");
          setBranchPending(false);
          return;
        }
        setPhase({ kind: "ready", user: res.data });
        branchForm.setFieldValue("default_branch", res.data.default_branch?.trim() || "main");
        setAuthMeCache(queryClient, res.data);
        toastSuccess("Default branch saved.");
        setBranchPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setBranchPending(false);
      }
    },
  });

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      const user = loaderData.user;
      branchForm.setFieldValue("default_branch", user.default_branch?.trim() || "main");
      setPhase({ kind: "ready", user });
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
        const res = await apiClient.user.getProfile();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign(LOGIN_RETURN);
          return;
        }
        const user = res.data;
        branchForm.setFieldValue("default_branch", user.default_branch?.trim() || "main");
        setPhase({ kind: "ready", user });
      } catch {
        if (cancelled) return;
        setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loaderData]);

  async function logoutThisDevice() {
    setLogoutPending(true);
    try {
      await apiClient.auth.logout();
    } catch {
      /* still leave */
    }
    syncSessionPresenceHint(false);
    clearSessionQueries(queryClient);
    window.location.assign("/");
  }

  async function logoutAllDevices() {
    setLogoutPending(true);
    try {
      await apiClient.auth.logoutAll();
    } catch {
      /* still leave */
    }
    syncSessionPresenceHint(false);
    clearSessionQueries(queryClient);
    window.location.assign("/");
  }

  @if (phase.kind === "loading") {
    <></>;
  } @else {
    @if (phase.kind === "error") {
      <SettingsShell bare>
        <p className="text-[16px] text-destructive">{phase.message as string}</p>
      </SettingsShell>;
    } @else {
      <AccountSettingsLayout active="general" data-testid="settings-general-page">
        <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
          General
        </h1>
        <p className="mt-2 text-[16px] text-muted-foreground">
          Appearance, defaults for new repositories, and session controls.
        </p>

        <div className="mt-8 flex flex-col gap-8">
          <section aria-labelledby="theme-heading" className="flex flex-col gap-3">
            <h2 id="theme-heading" className="text-[18px] font-semibold text-foreground">
              Appearance
            </h2>
            <p className="text-[14px] text-muted-foreground">
              Choose light, dark, or match your system preference.
            </p>
            <ThemeSelect layout="panel" className="max-w-sm" />
          </section>

          <hr className="border-border" />

          <form
            method="post"
            action="#"
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              void branchForm.handleSubmit();
            }}
          >
            <h2 className="text-[18px] font-semibold text-foreground">Repositories</h2>
            <branchForm.Field name="default_branch">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="default-branch">Default branch name</Label>
                  <Input
                    id="default-branch"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
                    autoComplete="off"
                  />
                  <p className="text-[14px] text-muted-foreground">
                    Applies to new repositories you create. Existing repos keep their current
                    default branch.
                  </p>
                </div>
              )}
            </branchForm.Field>
            <Button type="submit" disabled={branchPending} className="w-full sm:w-auto">
              {(branchPending ? "Working…" : "Save default branch") as string}
            </Button>
          </form>

          <hr className="border-border" />

          <section aria-labelledby="session-heading" className="flex flex-col gap-3">
            <h2 id="session-heading" className="text-[18px] font-semibold text-foreground">
              Session
            </h2>
            <div className="flex flex-wrap gap-3">
              <Button
                type="button"
                variant="ghost"
                disabled={logoutPending}
                onClick={() => void logoutThisDevice()}
              >
                Log out
              </Button>
              <Button
                type="button"
                variant="ghost"
                disabled={logoutPending}
                className={cn("border border-destructive text-destructive hover:bg-destructive/10")}
                onClick={() => setConfirmLogoutAll(true)}
              >
                Log out all devices
              </Button>
            </div>
          </section>
        </div>

        @if (confirmLogoutAll) {
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="logout-all-title"
            className="fixed inset-0 z-50 flex items-center justify-center bg-foreground/40 p-4"
          >
            <div className="w-full max-w-md rounded-md border border-border bg-card p-6 shadow-lg">
              <h2
                id="logout-all-title"
                className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground"
              >
                Log out all devices
              </h2>
              <p className="mt-3 text-[16px] text-muted-foreground">
                Sign out everywhere? You'll need to sign in again on each device.
              </p>
              <div className="mt-6 flex flex-wrap gap-3">
                <Button
                  type="button"
                  disabled={logoutPending}
                  className="bg-destructive text-white hover:brightness-110"
                  onClick={() => void logoutAllDevices()}
                >
                  Log out all devices
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={logoutPending}
                  onClick={() => setConfirmLogoutAll(false)}
                >
                  Stay signed in
                </Button>
              </div>
            </div>
          </div>;
        }
      </AccountSettingsLayout>;
    }
  }
}
