import { createFileRoute, redirect, useLoaderData } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { ProviderMode } from "@oxidean/api-client";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { syncSessionPresenceHint } from "@/lib/session-hint";
import { fetchBootstrapStatus } from "@/lib/ssr-auth";

export type SetupLoaderData = {
  loadError: string;
};

export const Route = createFileRoute("/setup/")({
  component: SetupPage,
  head: () => ({ meta: [{ title: "Set up · Oxidean" }] }),
  beforeLoad: async () => {
    let needsSetup: boolean | null = null;
    try {
      const status = await fetchBootstrapStatus();
      if (status.ok) needsSetup = status.data.needs_setup;
    } catch {
      /* API unreachable — loader still renders form */
    }
    if (needsSetup === false) {
      throw redirect({ href: "/" });
    }
  },
  loader: async (): Promise<SetupLoaderData> => {
    try {
      const status = await fetchBootstrapStatus();
      if (!status.ok) {
        return {
          loadError: rpcErrorMessage(status.error),
        };
      }
      if (!status.data.needs_setup) {
        throw redirect({ href: "/" });
      }
      return { loadError: "" };
    } catch (e) {
      if (e && typeof e === "object" && "href" in e) throw e;
      return {
        loadError: networkErrorMessage(),
      };
    }
  },
});

const NETWORK_ERROR = networkErrorMessage();
const UNAVAILABLE_ERROR = "Setup is no longer available. Sign in or create an account instead.";

const PROVIDER_OPTIONS: { value: ProviderMode; label: string }[] = [
  { value: "local", label: "Local accounts" },
  { value: "workos", label: "WorkOS" },
  { value: "oidc", label: "OIDC / SSO" },
];

/** Exported for AUTH-07 integration tests (wizard Switch + CTA). */
export function SetupPage() @{
  const data = useLoaderData({ from: "/setup/" }) as SetupLoaderData | undefined;
  const [error, setError] = useState(data?.loadError ?? "");

  const form = useForm({
    defaultValues: {
      email: "",
      username: "",
      password: "",
      confirm: "",
      allowSignup: false,
      providerMode: "local" as ProviderMode,
      oidcIssuer: "http://127.0.0.1:9090/default",
      oidcClientId: "oxidean-dev",
      workosClientId: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (value.password !== value.confirm) {
        setError("Passwords do not match. Fix the highlighted fields and try again.");
        return;
      }
      try {
        const res = await apiClient.auth.bootstrapSetup({
          email: value.email.trim(),
          username: value.username.trim(),
          password: value.password,
          allow_signup: value.allowSignup,
          provider_mode: value.providerMode,
          oidc_issuer: value.providerMode === "oidc" ? value.oidcIssuer.trim() || null : null,
          oidc_client_id: value.providerMode === "oidc" ? value.oidcClientId.trim() || null : null,
          workos_client_id:
            value.providerMode === "workos" ? value.workosClientId.trim() || null : null,
        });
        if (!res.ok) {
          const code = res.error.code;
          if (code === "auth.setup_unavailable") setError(UNAVAILABLE_ERROR);
          else if (code === "auth.reserved_username") {
            setError("That username is reserved. Choose a different username.");
          } else if (
            code === "auth.invalid_username" ||
            code === "auth.invalid_email" ||
            code === "auth.weak_password" ||
            code === "rpc.bad_input"
          ) {
            setError(`${res.error.message}. Fix the highlighted fields and try again.`);
          } else {
            setError(rpcErrorMessage(res.error));
          }
          return;
        }
        syncSessionPresenceHint(true);
        window.location.assign("/");
      } catch {
        setError(NETWORK_ERROR);
      }
    },
  });

  <AuthShell
    title="Set up this instance"
    support="Create the system admin account and choose how people will sign in."
  >
    @if (error) {
      <AuthErrorBanner message={error} />;
    }

    <form
      className="flex flex-col gap-4"
      method="post"
      action="#"
      onSubmit={(e) => {
        e.preventDefault();
        void form.handleSubmit();
      }}
    >
      <form.Field name="email">
        {(field) => (
          <div className="flex flex-col gap-2">
            <Label htmlFor="setup-email">Email</Label>
            <Input
              id="setup-email"
              name="email"
              type="email"
              autoComplete="email"
              value={field.state.value}
              onBlur={field.handleBlur}
              onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              required
            />
          </div>
        )}
      </form.Field>
      <form.Field name="username">
        {(field) => (
          <div className="flex flex-col gap-2">
            <Label htmlFor="setup-username">Username</Label>
            <Input
              id="setup-username"
              name="username"
              autoComplete="username"
              value={field.state.value}
              onBlur={field.handleBlur}
              onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              required
            />
            <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
              1–39 characters; letters, numbers, and hyphens. Can't start or end with a hyphen.
            </p>
          </div>
        )}
      </form.Field>
      <form.Field name="password">
        {(field) => (
          <div className="flex flex-col gap-2">
            <Label htmlFor="setup-password">Password</Label>
            <Input
              id="setup-password"
              name="password"
              type="password"
              autoComplete="new-password"
              value={field.state.value}
              onBlur={field.handleBlur}
              onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              required
            />
          </div>
        )}
      </form.Field>
      <form.Field name="confirm">
        {(field) => (
          <div className="flex flex-col gap-2">
            <Label htmlFor="setup-confirm">Confirm password</Label>
            <Input
              id="setup-confirm"
              name="confirm"
              type="password"
              autoComplete="new-password"
              value={field.state.value}
              onBlur={field.handleBlur}
              onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              required
            />
          </div>
        )}
      </form.Field>

      <form.Field name="providerMode">
        {(field) => (
          <div className="flex flex-col gap-2">
            <Label htmlFor="setup-provider">Auth stack</Label>
            <select
              id="setup-provider"
              className="flex h-11 w-full rounded-md border border-border bg-background px-3 text-[16px] text-foreground"
              value={field.state.value}
              onChange={(e) =>
                field.handleChange((e.currentTarget as HTMLSelectElement).value as ProviderMode)
              }
            >
              @for (const opt of PROVIDER_OPTIONS; key opt.value) {
                <option value={opt.value}>{opt.label as string}</option>;
              }
            </select>
            <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
              Secrets (OIDC client secret, WorkOS API key) stay in environment variables — only
              public IDs are stored here.
            </p>
          </div>
        )}
      </form.Field>

      {/* octane-dom-race-ok: setup provider panels — reviewed; keepMounted radios
          already used; panel swap restructure deferred from PAT mint work. */}
      <form.Subscribe selector={(s) => s.values.providerMode}>
        {(providerMode) => (
          <>
            @if (providerMode === "workos") {
              <form.Field name="workosClientId">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="setup-workos-client-id">WorkOS client ID</Label>
                    <Input
                      id="setup-workos-client-id"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(ev) =>
                        field.handleChange((ev.currentTarget as HTMLInputElement).value)
                      }
                      autoComplete="off"
                    />
                  </div>
                )}
              </form.Field>;
            }

            @if (providerMode === "oidc") {
              <div className="flex flex-col gap-4">
                <form.Field name="oidcIssuer">
                  {(field) => (
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="setup-oidc-issuer">OIDC issuer</Label>
                      <Input
                        id="setup-oidc-issuer"
                        value={field.state.value}
                        onBlur={field.handleBlur}
                        onInput={(ev) =>
                          field.handleChange((ev.currentTarget as HTMLInputElement).value)
                        }
                        autoComplete="off"
                      />
                    </div>
                  )}
                </form.Field>
                <form.Field name="oidcClientId">
                  {(field) => (
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="setup-oidc-client-id">OIDC client ID</Label>
                      <Input
                        id="setup-oidc-client-id"
                        value={field.state.value}
                        onBlur={field.handleBlur}
                        onInput={(ev) =>
                          field.handleChange((ev.currentTarget as HTMLInputElement).value)
                        }
                        autoComplete="off"
                      />
                    </div>
                  )}
                </form.Field>
              </div>;
            }
          </>
        )}
      </form.Subscribe>

      <form.Field name="allowSignup">
        {(field) => (
          <div className="flex flex-col gap-2">
            <div className="flex min-h-11 items-center gap-2">
              <Switch
                id="setup-allow-signup"
                checked={field.state.value}
                onCheckedChange={(checked) => field.handleChange(checked === true)}
              />
              <Label htmlFor="setup-allow-signup">Allow open signup</Label>
            </div>
            <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
              When off, new accounts can’t self-register. You can change this later in Auth
              settings.
            </p>
          </div>
        )}
      </form.Field>
      <form.Subscribe selector={(s) => s.isSubmitting}>
        {(isSubmitting) => (
          <Button type="submit" className="w-full" disabled={isSubmitting}>
            {(isSubmitting ? "Working…" : "Create system admin") as string}
          </Button>
        )}
      </form.Subscribe>
    </form>
  </AuthShell>;
}
