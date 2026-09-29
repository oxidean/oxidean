import { Check, ChevronDown } from "@octanejs/lucide";
import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  AuthSettingsPublic,
  EmailProviderKind,
  ProviderMode,
  RepoVisibility,
  UserPublic,
} from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useEffect, useState } from "octane";
import { apiClient } from "@/lib/api-client";
import {
  adminAuthSettingsQueryOptions,
  authSessionQueryOptions,
  clearSessionQueries,
  setAdminAuthSettingsCache,
  setAuthMeCache,
} from "@/lib/session-queries";
import { syncSessionPresenceHint } from "@/lib/session-hint";
import { fetchAdminAuthSettings, fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectList,
  SelectPopup,
  SelectPortal,
  SelectPositioner,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

export type AdminAuthLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: UserPublic;
      settings: AuthSettingsPublic;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage auth settings.";

export const Route = createFileRoute("/admin/auth")({
  component: AdminAuthPage,
  head: () => ({ meta: [{ title: "Auth settings · Oxidean" }] }),
  loader: async (): Promise<AdminAuthLoaderData> => {
    try {
      const meRes = await fetchSessionMe();
      if (!meRes.ok) {
        if (meRes.error.code === "auth.unauthenticated") {
          return { kind: "unauthenticated" };
        }
        return {
          kind: "error",
          message: meRes.error.message || "Could not load session.",
        };
      }
      if (meRes.data.role !== "sys-admin") {
        return { kind: "forbidden" };
      }
      const settingsRes = await fetchAdminAuthSettings();
      if (!settingsRes.ok) {
        if (settingsRes.error.code === "admin.forbidden") {
          return { kind: "forbidden" };
        }
        return {
          kind: "error",
          message: settingsRes.error.message || FORBIDDEN,
        };
      }
      return {
        kind: "ready",
        me: meRes.data,
        settings: settingsRes.data,
      };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

const PROVIDER_OPTIONS: { value: ProviderMode; label: string }[] = [
  { value: "local", label: "Local" },
  { value: "workos", label: "WorkOS" },
  { value: "oidc", label: "OIDC" },
];

const VISIBILITY_OPTIONS: { value: RepoVisibility; label: string }[] = [
  { value: "public", label: "Public" },
  { value: "private", label: "Private" },
];

const EMAIL_OPTIONS: { value: EmailProviderKind; label: string }[] = [
  { value: "log", label: "Log sink" },
  { value: "smtp", label: "SMTP" },
  { value: "resend", label: "Resend" },
];

function formDefaultsFromSettings(s: AuthSettingsPublic | null | undefined) {
  return {
    provider_mode: (s?.provider_mode ?? "local") as ProviderMode,
    allow_signup: s?.allow_signup === true,
    default_visibility: (s?.default_visibility === "private"
      ? "private"
      : "public") as RepoVisibility,
    email_provider: (s?.email_provider ?? "log") as EmailProviderKind,
    from_address: s?.from_address ?? "",
    workos_client_id: s?.workos_client_id ?? "",
    oidc_issuer: s?.oidc_issuer ?? "",
    oidc_client_id: s?.oidc_client_id ?? "",
  };
}

function flagsFromSettings(s: AuthSettingsPublic) {
  return {
    smtp_configured: s.smtp_configured,
    resend_configured: s.resend_configured,
    workos_api_key_configured: s.workos_api_key_configured,
    oidc_client_secret_configured: s.oidc_client_secret_configured,
  };
}

function EnvBadge({ show }: { show: boolean }) @{
  @if (show) {
    <span className="inline-flex h-11 items-center rounded-md border border-border bg-muted/40 px-3 text-[14px] text-muted-foreground">
      Configured via ENV
    </span>;
  }
}

function ModeSelect<T extends string>({
  id,
  label,
  value,
  options,
  onChange,
}: {
  id: string;
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (next: T) => void;
}) @{
  const current = options.find((o) => o.value === value) ?? options[0]!;

  <div className="flex flex-col gap-2">
    <Label htmlFor={id}>{label as string}</Label>
    <SelectRoot
      value={value}
      onValueChange={(next) => {
        if (typeof next === "string") onChange(next as T);
      }}
    >
      <SelectTrigger id={id} className="w-full min-w-[12rem]" aria-label={label}>
        <SelectValue>{current.label as string}</SelectValue>
        <SelectIcon>
          <ChevronDown aria-hidden size={16} />
        </SelectIcon>
      </SelectTrigger>
      {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
          hatch; popup is anchor-scoped under the Select root. */}
      <SelectPortal>
        <SelectPositioner sideOffset={4}>
          <SelectPopup>
            <SelectList>
              @for (const opt of options; key opt.value) {
                <SelectItem value={opt.value}>
                  <SelectItemIndicator>
                    <Check aria-hidden size={16} className="text-primary" />
                  </SelectItemIndicator>
                  <SelectItemText>{opt.label as string}</SelectItemText>
                </SelectItem>;
              }
            </SelectList>
          </SelectPopup>
        </SelectPositioner>
      </SelectPortal>
    </SelectRoot>
  </div>;
}

/** Exported for D-08 integration tests (Allow open signup Switch). */
export function AdminAuthPage() @{
  const queryClient = useQueryClient();
  const loaderData = useLoaderData({ from: "/admin/auth" }) as AdminAuthLoaderData | undefined;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/auth");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
      setAdminAuthSettingsCache(queryClient, loaderData.settings);
    }
  }, [loaderData, queryClient]);

  const me = useQuery({
    ...authSessionQueryOptions(),
    ...(loaderData?.kind === "ready"
      ? { initialData: loaderData.me }
      : loaderData?.kind === "unauthenticated"
        ? { initialData: null as UserPublic | null }
        : {}),
  });
  const isSysAdmin = me.isSuccess && me.data?.role === "sys-admin";

  const settingsQ = useQuery({
    ...adminAuthSettingsQueryOptions(),
    enabled: isSysAdmin || loaderData?.kind === "ready",
    ...(loaderData?.kind === "ready" ? { initialData: loaderData.settings } : {}),
  });

  const initialSettings = loaderData?.kind === "ready" ? loaderData.settings : null;

  const [flags, setFlags] = useState(() =>
    initialSettings
      ? flagsFromSettings(initialSettings)
      : {
          smtp_configured: false,
          resend_configured: false,
          workos_api_key_configured: false,
          oidc_client_secret_configured: false,
        },
  );
  const [pending, setPending] = useState(false);
  const [draftReady, setDraftReady] = useState(() => !!initialSettings);

  const form = useForm({
    defaultValues: formDefaultsFromSettings(initialSettings),
    onSubmit: async ({ value }) => {
      setPending(true);
      try {
        const res = await apiClient.admin.auth.updateSettings({
          provider_mode: value.provider_mode,
          email_provider: value.email_provider,
          from_address: value.from_address.trim() || null,
          workos_client_id:
            value.provider_mode === "workos" ? value.workos_client_id.trim() || null : null,
          oidc_issuer: value.provider_mode === "oidc" ? value.oidc_issuer.trim() || null : null,
          oidc_client_id:
            value.provider_mode === "oidc" ? value.oidc_client_id.trim() || null : null,
          allow_signup: value.allow_signup,
          default_visibility: value.default_visibility,
        });
        if (!res.ok) {
          toastError(res.error.message || "Could not save auth settings.");
          setPending(false);
          return;
        }
        applySettings(res.data);
        setAdminAuthSettingsCache(queryClient, res.data);
        toastSuccess("Auth settings saved.");
        setPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  function applySettings(next: AuthSettingsPublic) {
    form.setFieldValue("provider_mode", next.provider_mode);
    form.setFieldValue("allow_signup", next.allow_signup === true);
    form.setFieldValue(
      "default_visibility",
      next.default_visibility === "private" ? "private" : "public",
    );
    form.setFieldValue("email_provider", next.email_provider);
    form.setFieldValue("from_address", next.from_address ?? "");
    form.setFieldValue("workos_client_id", next.workos_client_id ?? "");
    form.setFieldValue("oidc_issuer", next.oidc_issuer ?? "");
    form.setFieldValue("oidc_client_id", next.oidc_client_id ?? "");
    setFlags(flagsFromSettings(next));
    setDraftReady(true);
  }

  useEffect(() => {
    if (loaderData) return;
    if (me.isPending) return;
    if (me.isError) return;
    if (me.data === null) {
      window.location.assign("/login?returnTo=/admin/auth");
    }
  }, [loaderData, me.isPending, me.isError, me.data]);

  useEffect(() => {
    if (settingsQ.isSuccess && settingsQ.data) {
      applySettings(settingsQ.data);
    }
  }, [settingsQ.isSuccess, settingsQ.dataUpdatedAt]);

  const settingsErrorCode =
    settingsQ.isError && settingsQ.error && typeof settingsQ.error === "object"
      ? (settingsQ.error as { code?: string }).code
      : undefined;

  const readyFromLoader = loaderData?.kind === "ready";
  const sessionKnown =
    readyFromLoader ||
    loaderData?.kind === "unauthenticated" ||
    loaderData?.kind === "forbidden" ||
    loaderData?.kind === "error" ||
    !me.isPending;
  const signedOut =
    loaderData?.kind === "unauthenticated" ||
    (sessionKnown && !me.isPending && (!me.isSuccess || !me.data));
  const forbidden =
    loaderData?.kind === "forbidden" ||
    (sessionKnown && me.isSuccess && !!me.data && !isSysAdmin) ||
    settingsErrorCode === "admin.forbidden";
  const loaderError = loaderData?.kind === "error" ? loaderData.message : "";
  const settings = settingsQ.data ?? (readyFromLoader ? loaderData.settings : undefined);
  const loadError =
    !!loaderError || me.isError || (settingsQ.isError && settingsErrorCode !== "admin.forbidden");
  const waitingForDraft =
    !readyFromLoader && isSysAdmin && (settingsQ.isPending || (settingsQ.isSuccess && !draftReady));

  @if (!sessionKnown || waitingForDraft) {
    <></>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage auth settings.
          </p>
          <a
            href="/login?returnTo=/admin/auth"
            className="mt-4 inline-block text-primary underline-offset-2 hover:underline"
          >
            Sign in
          </a>
        </div>
      </SettingsShell>;
    } @else {
      @if (forbidden) {
        <SettingsShell bare>
          <p role="alert" className="text-center text-[16px] text-destructive">
            {FORBIDDEN as string}
          </p>
        </SettingsShell>;
      } @else {
        @if (loadError && !settings) {
          <SettingsShell bare>
            <p role="alert" className="text-center text-[16px] text-destructive">
              {(loaderError || NETWORK_ERROR) as string}
            </p>
          </SettingsShell>;
        } @else {
          <AdminSettingsLayout active="auth" data-testid="admin-auth-page">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              Auth settings
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              Instance provider mode and email delivery. Secrets stay in environment variables.
            </p>

            {/* octane-dom-race-ok: admin auth settings — reviewed; provider panels
                restructure deferred (PAT mint Subscribe races fixed separately). */}
            <form.Subscribe selector={(s) => s.values} /* octane-dom-race-ok */>
              {(values) => {
                return (
                  <form
                    method="post"
                    action="#"
                    className="mt-8 flex flex-col gap-6"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void form.handleSubmit();
                    }}
                  >
                    <ModeSelect
                      id="auth-provider"
                      label="Auth provider"
                      value={values.provider_mode}
                      options={PROVIDER_OPTIONS}
                      onChange={(v) => form.setFieldValue("provider_mode", v)}
                    />

                    <div className="flex flex-col gap-2">
                      <div className="flex min-h-11 items-center gap-2">
                        <Switch
                          id="admin-allow-signup"
                          checked={values.allow_signup}
                          onCheckedChange={(checked) =>
                            form.setFieldValue("allow_signup", checked === true)
                          }
                        />
                        <Label htmlFor="admin-allow-signup">Allow open signup</Label>
                      </div>
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        When off, new accounts can’t self-register. You can change this later in
                        Auth settings.
                      </p>
                    </div>

                    <ModeSelect
                      id="default-visibility"
                      label="Default repository visibility"
                      value={values.default_visibility}
                      options={VISIBILITY_OPTIONS}
                      onChange={(v) => form.setFieldValue("default_visibility", v)}
                    />
                    <p className="-mt-4 text-[14px] font-normal leading-[1.4] text-muted-foreground">
                      Used when creating a repository if the owner doesn’t pick Public or Private
                      (D-08).
                    </p>

                    {/* octane-dom-race-ok */}
                    @if (values.provider_mode === "workos") {
                      <div className="flex flex-col gap-4 rounded-md border border-border p-4">
                        <div className="flex flex-col gap-2">
                          <Label htmlFor="workos-client-id">WorkOS client ID</Label>
                          <Input
                            id="workos-client-id"
                            value={values.workos_client_id}
                            onInput={(e) =>
                              form.setFieldValue(
                                "workos_client_id",
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                            autoComplete="off"
                          />
                        </div>
                        <div className="flex flex-col gap-2">
                          <Label>WorkOS API key</Label>
                          <EnvBadge show={flags.workos_api_key_configured} />
                          @if (
                            !flags.workos_api_key_configured
                          ) {
                            <p className="text-[14px] text-muted-foreground">
                              Not set — configure WORKOS_API_KEY in the environment.
                            </p>;
                          }
                        </div>
                      </div>;
                    }

                    @if (values.provider_mode === "oidc") {
                      <div className="flex flex-col gap-4 rounded-md border border-border p-4">
                        <div className="flex flex-col gap-2">
                          <Label htmlFor="oidc-issuer">OIDC issuer</Label>
                          <Input
                            id="oidc-issuer"
                            value={values.oidc_issuer}
                            onInput={(e) =>
                              form.setFieldValue(
                                "oidc_issuer",
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                            autoComplete="off"
                          />
                        </div>
                        <div className="flex flex-col gap-2">
                          <Label htmlFor="oidc-client-id">OIDC client ID</Label>
                          <Input
                            id="oidc-client-id"
                            value={values.oidc_client_id}
                            onInput={(e) =>
                              form.setFieldValue(
                                "oidc_client_id",
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                            autoComplete="off"
                          />
                        </div>
                        <div className="flex flex-col gap-2">
                          <Label>OIDC client secret</Label>
                          <EnvBadge show={flags.oidc_client_secret_configured} />
                          @if (
                            !flags.oidc_client_secret_configured
                          ) {
                            <p className="text-[14px] text-muted-foreground">
                              Not set — configure OXIDEAN_OIDC_CLIENT_SECRET in the
                              environment.
                            </p>;
                          }
                        </div>
                      </div>;
                    }

                    <ModeSelect
                      id="email-delivery"
                      label="Email delivery"
                      value={values.email_provider}
                      options={EMAIL_OPTIONS}
                      onChange={(v) => form.setFieldValue("email_provider", v)}
                    />

                    <div className="flex flex-col gap-4 rounded-md border border-border p-4">
                      <div className="flex flex-col gap-2">
                        <Label htmlFor="from-address">From address</Label>
                        <Input
                          id="from-address"
                          value={values.from_address}
                          onInput={(e) =>
                            form.setFieldValue(
                              "from_address",
                              (e.currentTarget as HTMLInputElement).value,
                            )
                          }
                          placeholder="Oxidean <noreply@example.com>"
                          autoComplete="off"
                        />
                      </div>
                      @if (values.email_provider === "smtp") {
                        <div className="flex flex-col gap-2">
                          <Label>SMTP credentials</Label>
                          <EnvBadge show={flags.smtp_configured} />
                          @if (!flags.smtp_configured) {
                            <p className="text-[14px] text-muted-foreground">
                              Not set — configure OXIDEAN_SMTP_URL in the environment.
                            </p>;
                          }
                        </div>;
                      }
                      @if (values.email_provider === "resend") {
                        <div className="flex flex-col gap-2">
                          <Label>Resend API key</Label>
                          <EnvBadge show={flags.resend_configured} />
                          @if (!flags.resend_configured) {
                            <p className="text-[14px] text-muted-foreground">
                              Not set — configure OXIDEAN_RESEND_API_KEY in the
                              environment.
                            </p>;
                          }
                        </div>;
                      }
                    </div>

                    <Button type="submit" disabled={pending} className="w-full sm:w-auto">
                      {(pending ? "Working…" : "Save auth settings") as string}
                    </Button>
                  </form>
                );
              }}
            </form.Subscribe>

            <FactoryResetSection />
          </AdminSettingsLayout>;
        }
      }
    }
  }
}

function FactoryResetSection() @{
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [scope, setScope] = useState<"database_only" | "database_and_repositories">(
    "database_only",
  );
  const [phrase, setPhrase] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  function resetForm() {
    setScope("database_only");
    setPhrase("");
    setError("");
  }

  async function runReset() {
    setError("");
    setPending(true);
    try {
      const res = await apiClient.admin.auth.factoryReset({
        confirmation: phrase.trim(),
        scope,
      });
      if (!res.ok) {
        setError(res.error.message || "Factory reset failed.");
        setPending(false);
        return;
      }
      syncSessionPresenceHint(false);
      clearSessionQueries(queryClient);
      window.location.assign("/setup");
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <div className="mt-12 flex flex-col gap-4 rounded-md border border-destructive/40 p-4">
    <h2 className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-destructive">
      Danger zone
    </h2>
    <p className="text-[14px] leading-[1.4] text-muted-foreground">
      Wipe this instance back to the empty setup wizard. Choose whether repository files on disk are
      kept or deleted. This cannot be undone.
    </p>
    <Button
      type="button"
      variant="destructive"
      className="w-full sm:w-auto"
      onClick={() => {
        resetForm();
        setOpen(true);
      }}
    >
      Reset this instance
    </Button>

    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (pending) return;
        setOpen(next);
        if (!next) resetForm();
      }}
    >
      <DialogContent showCloseButton={!pending}>
        <DialogHeader>
          <DialogTitle>Reset this instance</DialogTitle>
          <DialogDescription>
            Irreversible. Type RESET to confirm. Scope controls whether bare repositories under
            storage are deleted.
          </DialogDescription>
        </DialogHeader>

        <RadioGroup
          value={scope}
          onValueChange={(v) => {
            if (v === "database_only" || v === "database_and_repositories") {
              setScope(v);
            }
          }}
          disabled={pending}
          className="gap-4"
        >
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem value="database_only" className="mt-0.5" />
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-medium text-foreground">Database only</span>
              <span className="text-[14px] leading-[1.4] text-muted-foreground">
                Wipe users, sessions, and auth. Keep repository files on disk.
              </span>
            </span>
          </label>
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem value="database_and_repositories" className="mt-0.5" />
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-medium text-foreground">
                Database and repositories
              </span>
              <span className="text-[14px] leading-[1.4] text-muted-foreground">
                Wipe the database and delete repository files under storage.
              </span>
            </span>
          </label>
        </RadioGroup>

        <div className="flex flex-col gap-2">
          <Label htmlFor="factory-reset-confirm">Confirmation</Label>
          <Input
            id="factory-reset-confirm"
            value={phrase}
            onInput={(e) => setPhrase((e.currentTarget as HTMLInputElement).value)}
            autoComplete="off"
            placeholder="RESET"
            disabled={pending}
          />
        </div>

        @if (error) {
          <p role="alert" className="text-[14px] text-destructive">
            {error as string}
          </p>;
        }

        <DialogFooter>
          <Button
            type="button"
            variant="secondary"
            disabled={pending}
            onClick={() => {
              setOpen(false);
              resetForm();
            }}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={pending || phrase.trim() !== "RESET"}
            onClick={() => void runReset()}
          >
            {(pending ? "Resetting…" : "Reset instance") as string}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>;
}
