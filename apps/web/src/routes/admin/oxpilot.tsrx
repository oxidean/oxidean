import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import type {
  OxpilotProviderDescriptor,
  OxpilotSettingsResponse,
  OxpilotSettingsSetRequest,
  UserPublic,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { Check, ChevronDown } from "@octanejs/lucide";
import { AppPage } from "@/components/app-page";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { apiClient } from "@/lib/api-client";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import {
  fetchAdminOxpilotProviders,
  fetchAdminOxpilotSettings,
  fetchSessionMe,
} from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";

export type AdminOxpilotLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: UserPublic;
      settings: OxpilotSettingsResponse;
      providers: OxpilotProviderDescriptor[];
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage OxPilot settings.";

const settingsQueryKey = ["admin", "oxpilot", "settings"] as const;
const providersQueryKey = ["admin", "oxpilot", "providers"] as const;

function adminOxpilotQueryOptions() {
  return queryOptions({
    queryKey: ["admin", "adminOxpilot"],
    queryFn: async (): Promise<AdminOxpilotLoaderData> => {
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
        const settingsRes = await fetchAdminOxpilotSettings();
        if (!settingsRes.ok) {
          if (settingsRes.error.code === "admin.forbidden") {
            return { kind: "forbidden" };
          }
          return {
            kind: "error",
            message: settingsRes.error.message || FORBIDDEN,
          };
        }
        const providersRes = await fetchAdminOxpilotProviders();
        return {
          kind: "ready",
          me: meRes.data,
          settings: settingsRes.data,
          providers: providersRes.ok ? providersRes.data.providers : [],
        };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

type FormValues = {
  enabled: boolean;
  provider_id: string;
  model: string;
  base_url: string;
  api_key: string;
  user_budget: string;
  instance_budget: string;
};

/** Provider Select value for "no admin override" — writes `null` so
 *  resolution falls back to `OXIDEAN_OXPILOT_PROVIDER` (stub when unset).
 *  `stub` itself is never a concrete selection: the server refuses it on
 *  release builds (it is sys-admin-only preview mode, not a real backend). */
const ENV_PROVIDER_DEFAULT = "__env__";

function formDefaultsFromSettings(s: OxpilotSettingsResponse | null | undefined): FormValues {
  return {
    enabled: s?.enabled === true,
    provider_id:
      s?.provider_id_overridden === true && s?.provider_id ? s.provider_id : ENV_PROVIDER_DEFAULT,
    model: s?.model ?? "",
    base_url: s?.base_url ?? "",
    api_key: "",
    user_budget: s?.user_budget != null ? String(s.user_budget) : "",
    instance_budget: s?.instance_budget != null ? String(s.instance_budget) : "",
  };
}

/** "(from environment)" provenance badge — same treatment as PAT policy. */
function EnvSourceBadge({ overridden }: { overridden: boolean }) @{
  @if (!overridden) {
    <span className="text-[13px] text-muted-foreground">(from environment)</span>;
  } @else {
    <span className="text-[13px] text-muted-foreground">(admin override)</span>;
  }
}

const PROVIDER_LABELS: Record<string, string> = {
  stub: "Stub (deterministic, sys-admin only)",
  openai: "OpenAI",
  anthropic: "Anthropic",
  ollama: "Ollama",
  "openai-compatible": "OpenAI-compatible (LiteLLM, vLLM, NIM, …)",
};

function providerLabel(d: OxpilotProviderDescriptor): string {
  return PROVIDER_LABELS[d.id] ?? d.id;
}

export function AdminOxpilotPage() @{
  <AppPage>
    <AdminOxpilotContent />
  </AppPage>;
}

export function AdminOxpilotContent() @{
  const queryClient = useQueryClient();
  const loaderData = useQuery(adminOxpilotQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/oxpilot");
    }
  }, [loaderData, queryClient]);

  // Sync each downstream cache only when its own loader field changes —
  // `writeSettingsCaches` bumps the loader query, and re-writing
  // `me`/`providers` off that snapshot would clobber caches that
  // recovered independently (e.g. providersQ after a loader-time 503).
  const loaderMe = loaderData?.kind === "ready" ? loaderData.me : null;
  const loaderSettings = loaderData?.kind === "ready" ? loaderData.settings : null;
  const loaderProviders = loaderData?.kind === "ready" ? loaderData.providers : null;
  useEffect(() => {
    if (loaderMe) setAuthMeCache(queryClient, loaderMe);
  }, [loaderMe, queryClient]);
  useEffect(() => {
    if (loaderSettings) queryClient.setQueryData(settingsQueryKey, loaderSettings);
  }, [loaderSettings, queryClient]);
  useEffect(() => {
    if (loaderProviders) queryClient.setQueryData(providersQueryKey, loaderProviders);
  }, [loaderProviders, queryClient]);

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
    queryKey: settingsQueryKey,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData: loaderData?.kind === "ready" ? loaderData.settings : undefined,
    queryFn: async (): Promise<OxpilotSettingsResponse> => {
      const res = await fetchAdminOxpilotSettings();
      if (!res.ok) {
        throw Object.assign(new Error(res.error.message || FORBIDDEN), {
          code: res.error.code,
        });
      }
      return res.data;
    },
  });

  const providersQ = useQuery({
    queryKey: providersQueryKey,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData: loaderData?.kind === "ready" ? loaderData.providers : undefined,
    queryFn: async (): Promise<OxpilotProviderDescriptor[]> => {
      const res = await fetchAdminOxpilotProviders();
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data.providers;
    },
  });

  const initialSettings = loaderData?.kind === "ready" ? loaderData.settings : null;
  const [pending, setPending] = useState(false);
  const [draftReady, setDraftReady, getDraftReady] = useState(() => !!initialSettings);

  const form = useForm({
    defaultValues: formDefaultsFromSettings(initialSettings),
    onSubmit: async ({ value }) => {
      const userBudget = value.user_budget.trim();
      const instanceBudget = value.instance_budget.trim();
      for (const raw of [userBudget, instanceBudget]) {
        if (raw !== "" && (!Number.isInteger(Number(raw)) || Number(raw) <= 0)) {
          toastError("Budgets must be positive whole numbers, or blank to clear.");
          return;
        }
      }
      if (value.provider_id === "openai-compatible" && value.base_url.trim() === "") {
        toastError("An OpenAI-compatible provider needs a Base URL for its endpoint.");
        return;
      }
      const req: OxpilotSettingsSetRequest = {
        enabled: value.enabled,
        provider_id: value.provider_id === ENV_PROVIDER_DEFAULT ? null : value.provider_id || null,
        model: value.model.trim() || null,
        base_url: value.base_url.trim() || null,
        user_budget: userBudget === "" ? null : Number(userBudget),
        instance_budget: instanceBudget === "" ? null : Number(instanceBudget),
      };
      const key = value.api_key.trim();
      if (key !== "") req.api_key = key;
      setPending(true);
      try {
        const res = await apiClient.admin.oxpilot.settingsSet(req);
        if (!res.ok) {
          toastError(res.error.message || "Could not save OxPilot settings.");
          setPending(false);
          return;
        }
        applySettings(res.data);
        writeSettingsCaches(res.data);
        toastSuccess("OxPilot settings saved.");
        setPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  function applySettings(next: OxpilotSettingsResponse) {
    const v = formDefaultsFromSettings(next);
    form.setFieldValue("enabled", v.enabled);
    form.setFieldValue("provider_id", v.provider_id);
    form.setFieldValue("model", v.model);
    form.setFieldValue("base_url", v.base_url);
    form.setFieldValue("api_key", "");
    form.setFieldValue("user_budget", v.user_budget);
    form.setFieldValue("instance_budget", v.instance_budget);
    setDraftReady(true);
  }

  // Settings writes must also update the loader query — a remount inside
  // its staleTime seeds the form from loaderData, and a stale loader
  // snapshot would silently restore the pre-save values.
  function writeSettingsCaches(next: OxpilotSettingsResponse) {
    // Cancel an in-flight loader first: its pre-save snapshot would
    // otherwise resolve after this write and clobber the saved
    // settings cache on a later remount.
    void queryClient.cancelQueries({ queryKey: adminOxpilotQueryOptions().queryKey });
    queryClient.setQueryData(settingsQueryKey, next);
    queryClient.setQueryData<AdminOxpilotLoaderData>(adminOxpilotQueryOptions().queryKey, (old) =>
      old?.kind === "ready" ? { ...old, settings: next } : old,
    );
  }

  // Seed the draft once — later refetches must not clobber unsaved
  // edits; save/clear handlers re-apply from the server response.
  useEffect(() => {
    // Read the live flag: Octane may run a stale-closure effect after user
    // edits land, and the captured `draftReady` would still read false.
    if (!getDraftReady() && settingsQ.isSuccess && settingsQ.data) {
      applySettings(settingsQ.data);
    }
  }, [settingsQ.isSuccess, settingsQ.dataUpdatedAt, draftReady]);

  async function clearOverrides() {
    setPending(true);
    try {
      const res = await apiClient.admin.oxpilot.settingsSet({ clear_overrides: true });
      if (!res.ok) {
        toastError(res.error.message || "Could not clear the overrides.");
        setPending(false);
        return;
      }
      applySettings(res.data);
      writeSettingsCaches(res.data);
      toastSuccess("Reverted to the OXIDEAN_OXPILOT_* environment defaults.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function clearKey() {
    setPending(true);
    try {
      const res = await apiClient.admin.oxpilot.settingsSet({ clear_key: true });
      if (!res.ok) {
        toastError(res.error.message || "Could not clear the API key.");
        setPending(false);
        return;
      }
      applySettings(res.data);
      writeSettingsCaches(res.data);
      toastSuccess("API key cleared.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  useEffect(() => {
    if (loaderData) return;
    if (me.isPending) return;
    if (me.isError) return;
    if (me.data === null) {
      window.location.assign("/login?returnTo=/admin/oxpilot");
    }
  }, [loaderData, me.isPending, me.isError, me.data]);

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
  const providers: OxpilotProviderDescriptor[] = providersQ.data ?? [];
  const loadError =
    !!loaderError || me.isError || (settingsQ.isError && settingsErrorCode !== "admin.forbidden");
  // Hold the form until the draft is seeded no matter which path supplies
  // it — a late-arriving ready loader must not lift the gate between
  // `isSuccess` and the seed effect, or the Switch paints unchecked first
  // and a click in that window toggles the field ON instead of OFF.
  const waitingForDraft =
    isSysAdmin && (settingsQ.isPending || (settingsQ.isSuccess && !draftReady));
  const anyOverridden =
    settings != null &&
    (settings.enabled_overridden ||
      settings.provider_id_overridden ||
      settings.model_overridden ||
      settings.base_url_overridden ||
      settings.user_budget_overridden ||
      settings.instance_budget_overridden ||
      settings.key_source === "db");
  /** Resolved provider — the env default (`__env__`) is not a registry id,
   *  so hints fall back to the server's resolved slot. */
  function describeProvider(selectedId: string) {
    return (
      providers.find((p) => p.id === selectedId) ??
      providers.find((p) => p.id === settings?.provider_id)
    );
  }

  @if (!sessionKnown || waitingForDraft) {
    <></>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage OxPilot settings.
          </p>
          <AppLink
            href="/login?returnTo=/admin/oxpilot"
            className="mt-4 inline-block text-primary underline-offset-2 hover:underline"
          >
            Sign in
          </AppLink>
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
          <AdminSettingsLayout active="oxpilot" data-testid="admin-oxpilot-page">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              OxPilot
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              The agentic chat, builder, and debugger surface at{" "}
              <AppLink href="/oxpilot" className="text-foreground underline underline-offset-2">
                /oxpilot
              </AppLink>
              . Fields marked (from environment) follow the <code>OXIDEAN_OXPILOT_*</code> defaults
              until overridden here.
            </p>

            <section className="mt-8 flex flex-col gap-4" aria-labelledby="oxpilot-status-heading">
              <h2 id="oxpilot-status-heading" className="text-[18px] font-semibold">
                Status
              </h2>
              <p className="text-[14px] text-foreground" data-testid="admin-oxpilot-status">
                Status:{" "}
                <strong>
                  {
                    (settings?.enabled
                      ? `Enabled — provider: ${settings.provider_id ?? "none"}`
                      : "Disabled") as string
                  }
                </strong>
                {(settings?.disabled_reason ? ` (${settings.disabled_reason})` : "") as string}
              </p>
              @if (
                settings?.enabled &&
                settings.provider_id === "stub"
              ) {
                <p className="text-[14px] leading-[1.4] text-muted-foreground">
                  The stub provider is the deterministic built-in — sessions run end-to-end
                  with canned answers and no API key. It is restricted to system
                  administrators; regular users see OxPilot as absent.
                </p>;
              }
              @if (settings?.disabled_reason === "misconfigured") {
                <p role="alert" className="text-[14px] text-destructive">
                  The configured provider is not recognized — pick a provider below and save.
                </p>;
              }
            </section>

            {/* octane-dom-race-ok: admin oxpilot settings — single-column
                form, Select popup is anchor-scoped under the root. */}
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
                    <div className="flex flex-col gap-2">
                      <div className="flex min-h-11 items-center gap-2">
                        <Switch
                          id="admin-oxpilot-enabled"
                          data-testid="admin-oxpilot-enabled"
                          aria-label="Enable OxPilot"
                          checked={values.enabled}
                          disabled={pending}
                          onCheckedChange={(checked) =>
                            form.setFieldValue("enabled", checked === true)
                          }
                        />
                        <Label htmlFor="admin-oxpilot-enabled">Enable OxPilot</Label>
                        <EnvSourceBadge overridden={settings?.enabled_overridden === true} />
                      </div>
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        When off, every <code>oxpilot.*</code> call answers{" "}
                        <code>oxpilot.disabled</code> and the page shows the disabled state.
                      </p>
                    </div>

                    <div className="flex flex-col gap-2">
                      <Label htmlFor="admin-oxpilot-provider">Provider</Label>
                      <SelectRoot
                        value={values.provider_id}
                        onValueChange={(next) => {
                          if (typeof next === "string") {
                            form.setFieldValue("provider_id", next);
                          }
                        }}
                      >
                        <SelectTrigger
                          id="admin-oxpilot-provider"
                          className="w-full min-w-[16rem]"
                          aria-label="Provider"
                          data-testid="admin-oxpilot-provider"
                        >
                          <SelectValue>
                            {
                              (values.provider_id === ENV_PROVIDER_DEFAULT
                                ? `Environment default (${
                                    PROVIDER_LABELS[settings?.provider_id ?? ""] ??
                                    settings?.provider_id ??
                                    "none"
                                  })`
                                : (PROVIDER_LABELS[values.provider_id] ??
                                  values.provider_id)) as string
                            }
                          </SelectValue>
                          <SelectIcon>
                            <ChevronDown aria-hidden size={16} />
                          </SelectIcon>
                        </SelectTrigger>
                        {/* octane-dom-race-ok: Base UI SelectPortal has no
                            keepMounted escape hatch; popup is
                            anchor-scoped under the Select root. */}
                        <SelectPortal>
                          <SelectPositioner sideOffset={4}>
                            <SelectPopup>
                              <SelectList>
                                <SelectItem
                                  value={ENV_PROVIDER_DEFAULT}
                                  data-testid="admin-oxpilot-provider-option-env"
                                >
                                  <SelectItemIndicator>
                                    <Check aria-hidden size={16} className="text-primary" />
                                  </SelectItemIndicator>
                                  <SelectItemText>{"Environment default" as string}</SelectItemText>
                                </SelectItem>
                                @for (const p of providers; key p.id) {
                                  <SelectItem
                                    value={p.id}
                                    data-testid={`admin-oxpilot-provider-option-${p.id}`}
                                  >
                                    <SelectItemIndicator>
                                      <Check aria-hidden size={16} className="text-primary" />
                                    </SelectItemIndicator>
                                    <SelectItemText>
                                      {providerLabel(p) as string}
                                    </SelectItemText>
                                  </SelectItem>;
                                }
                                @if (
                                  values.provider_id !== ENV_PROVIDER_DEFAULT &&
                                  !providers.some((p) => p.id === values.provider_id)
                                ) {
                                  <SelectItem
                                    value={values.provider_id}
                                    data-testid="admin-oxpilot-provider-option-current"
                                  >
                                    <SelectItemText>
                                      {
                                        (PROVIDER_LABELS[values.provider_id] ??
                                          values.provider_id) as string
                                      }
                                    </SelectItemText>
                                  </SelectItem>;
                                }
                              </SelectList>
                            </SelectPopup>
                          </SelectPositioner>
                        </SelectPortal>
                      </SelectRoot>
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        <EnvSourceBadge overridden={settings?.provider_id_overridden === true} />{" "}
                        {
                          (values.provider_id === ENV_PROVIDER_DEFAULT
                            ? "Follows OXIDEAN_OXPILOT_PROVIDER — stub (sys-admin only) when unset."
                            : describeProvider(values.provider_id)?.requires_key === true
                              ? "This provider needs an API key."
                              : "No API key required.") as string
                        }
                      </p>
                    </div>

                    <div className="flex flex-col gap-2">
                      <Label htmlFor="admin-oxpilot-model">Model</Label>
                      <Input
                        id="admin-oxpilot-model"
                        data-testid="admin-oxpilot-model"
                        value={values.model}
                        onInput={(e) =>
                          form.setFieldValue("model", (e.currentTarget as HTMLInputElement).value)
                        }
                        autoComplete="off"
                        placeholder={
                          (describeProvider(values.provider_id)?.models?.[0] ??
                            "provider default") as string
                        }
                        disabled={pending}
                      />
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        <EnvSourceBadge overridden={settings?.model_overridden === true} /> Blank
                        keeps the provider default.
                      </p>
                    </div>

                    <div className="flex flex-col gap-2">
                      <Label htmlFor="admin-oxpilot-base-url">Base URL</Label>
                      <Input
                        id="admin-oxpilot-base-url"
                        data-testid="admin-oxpilot-base-url"
                        value={values.base_url}
                        onInput={(e) =>
                          form.setFieldValue(
                            "base_url",
                            (e.currentTarget as HTMLInputElement).value,
                          )
                        }
                        autoComplete="off"
                        placeholder="https://… (Ollama or an OpenAI-compatible endpoint)"
                        disabled={pending}
                      />
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        <EnvSourceBadge overridden={settings?.base_url_overridden === true} />{" "}
                        {
                          (values.provider_id === "openai-compatible"
                            ? "Required — the chat-completions endpoint your server exposes, e.g. http://litellm.local:4000. Private/LAN hosts are allowed."
                            : "Only used by providers that take a custom endpoint.") as string
                        }
                      </p>
                    </div>

                    <div className="flex flex-col gap-2">
                      <Label htmlFor="admin-oxpilot-key">API key</Label>
                      <Input
                        id="admin-oxpilot-key"
                        data-testid="admin-oxpilot-key"
                        type="password"
                        value={values.api_key}
                        onInput={(e) =>
                          form.setFieldValue("api_key", (e.currentTarget as HTMLInputElement).value)
                        }
                        autoComplete="new-password"
                        placeholder={
                          (settings?.key_set
                            ? `Configured — ends ${settings.key_last4 ?? "????"}`
                            : "Not set") as string
                        }
                        disabled={pending}
                      />
                      <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                        {
                          (settings?.key_source === "db"
                            ? "Stored in the database (encrypted)."
                            : settings?.key_source === "env"
                              ? "Read from OXIDEAN_OXPILOT_API_KEY."
                              : "No key configured.") as string
                        }{" "}
                        Leave blank to keep the current key; enter a new one to replace it. The key
                        is never shown after saving.
                      </p>
                      @if (settings?.key_set === true) {
                        <div>
                          <Button
                            type="button"
                            variant="outline"
                            disabled={pending}
                            data-testid="admin-oxpilot-clear-key"
                            onClick={() => void clearKey()}
                          >
                            Clear stored key
                          </Button>
                        </div>;
                      }
                    </div>

                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                      <div className="flex flex-col gap-2">
                        <Label htmlFor="admin-oxpilot-user-budget">Per-user daily cap</Label>
                        <Input
                          id="admin-oxpilot-user-budget"
                          data-testid="admin-oxpilot-user-budget"
                          inputMode="numeric"
                          value={values.user_budget}
                          onInput={(e) =>
                            form.setFieldValue(
                              "user_budget",
                              (e.currentTarget as HTMLInputElement).value,
                            )
                          }
                          autoComplete="off"
                          placeholder="unbounded"
                          disabled={pending}
                        />
                        <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                          <EnvSourceBadge overridden={settings?.user_budget_overridden === true} />{" "}
                          Requests per UTC day; blank = unbounded.
                        </p>
                      </div>
                      <div className="flex flex-col gap-2">
                        <Label htmlFor="admin-oxpilot-instance-budget">Instance daily cap</Label>
                        <Input
                          id="admin-oxpilot-instance-budget"
                          data-testid="admin-oxpilot-instance-budget"
                          inputMode="numeric"
                          value={values.instance_budget}
                          onInput={(e) =>
                            form.setFieldValue(
                              "instance_budget",
                              (e.currentTarget as HTMLInputElement).value,
                            )
                          }
                          autoComplete="off"
                          placeholder="unbounded"
                          disabled={pending}
                        />
                        <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                          <EnvSourceBadge
                            overridden={settings?.instance_budget_overridden === true}
                          />{" "}
                          Requests per UTC day; blank = unbounded.
                        </p>
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-3">
                      <Button
                        type="submit"
                        disabled={pending}
                        className="w-full sm:w-auto"
                        data-testid="admin-oxpilot-save"
                      >
                        {(pending ? "Working…" : "Save OxPilot settings") as string}
                      </Button>
                      @if (anyOverridden) {
                        <Button
                          type="button"
                          variant="secondary"
                          disabled={pending}
                          className="w-full sm:w-auto"
                          data-testid="admin-oxpilot-clear-overrides"
                          onClick={() => void clearOverrides()}
                        >
                          Use environment defaults
                        </Button>;
                      }
                    </div>
                  </form>
                );
              }}
            </form.Subscribe>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
