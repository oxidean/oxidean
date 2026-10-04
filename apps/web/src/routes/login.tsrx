import { createFileRoute, redirect, useLoaderData } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { ProviderMode } from "@oxidean/api-client";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { readReturnToFromLocation, safeReturnTo } from "@/lib/return-to";
import { fetchProviderConfig, fetchSessionMe } from "@/lib/ssr-auth";
import { AppLink } from "@/components/ui/app-link";

export type LoginLoaderData = {
  mode: ProviderMode;
  allow_signup: boolean;
  loadError: string;
};

const LOGIN_ERROR = "Incorrect email/username or password. Check your details and try again.";
const NETWORK_ERROR = networkErrorMessage();

export const Route = createFileRoute("/login")({
  component: LoginPage,
  head: () => ({ meta: [{ title: "Sign in · Oxidean" }] }),
  loader: async (): Promise<LoginLoaderData> => {
    try {
      const me = await fetchSessionMe();
      if (me.ok) {
        const dest = me.data.must_change_credentials ? "/setup/credentials" : "/";
        throw redirect({ href: dest });
      }
    } catch (e) {
      if (e && typeof e === "object" && "href" in e) throw e;
      /* anonymous */
    }

    try {
      const cfg = await fetchProviderConfig();
      if (!cfg.ok) {
        return { mode: "local", allow_signup: false, loadError: rpcErrorMessage(cfg.error) };
      }
      return {
        mode: cfg.data.mode,
        allow_signup: cfg.data.allow_signup === true,
        loadError: "",
      };
    } catch {
      return { mode: "local", allow_signup: false, loadError: NETWORK_ERROR };
    }
  },
});

/** Exported for login.integration.test.ts (RESEARCH P1 / D-QH-03). */
export function LoginPage() @{
  const data = useLoaderData({ from: "/login" }) as LoginLoaderData;
  const mode = data.mode;
  const allowSignup = data.allow_signup;
  const loadError = data.loadError;
  const [error, setError] = useState("");

  const returnTo = readReturnToFromLocation();
  const signupHref = `/signup?returnTo=${encodeURIComponent(returnTo)}`;

  const form = useForm({
    defaultValues: {
      identifier: "",
      password: "",
      rememberMe: false,
    },
    onSubmit: async ({ value }) => {
      setError("");
      try {
        const res = await apiClient.auth.login({
          identifier: value.identifier.trim(),
          password: value.password,
          remember_me: value.rememberMe,
        });
        if (!res.ok) {
          setError(LOGIN_ERROR);
          return;
        }
        const dest = res.data.must_change_credentials
          ? `/setup/credentials?returnTo=${encodeURIComponent(returnTo)}`
          : safeReturnTo(returnTo);
        window.location.assign(dest);
      } catch {
        setError(NETWORK_ERROR);
      }
    },
  });

  function startSso(kind: "workos" | "oidc") {
    const path = kind === "workos" ? "/api/auth/workos/start" : "/api/auth/oidc/start";
    window.location.assign(`${path}?returnTo=${encodeURIComponent(returnTo)}`);
  }

  const support =
    mode === "workos"
      ? "Continue with WorkOS to access this Oxidean instance."
      : mode === "oidc"
        ? "Continue with your organization’s SSO to access this instance."
        : "Use your email or username to continue.";

  <AuthShell title="Sign in" support={support}>
    @if (loadError) {
      <AuthErrorBanner message={loadError} />;
    }
    @if (error) {
      <AuthErrorBanner message={error} />;
    }

    @if (mode === "local") {
      <form
        className="flex flex-col gap-4"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        <form.Field name="identifier">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="login-identifier">Email or username</Label>
              <Input
                id="login-identifier"
                name="identifier"
                autoComplete="username"
                placeholder="Email or username"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLInputElement).value)
                }
                required
              />
            </div>
          )}
        </form.Field>
        <form.Field name="password">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="login-password">Password</Label>
              <Input
                id="login-password"
                name="password"
                type="password"
                autoComplete="current-password"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLInputElement).value)
                }
                required
              />
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                <AppLink
                  href="/reset-password"
                  className="font-normal text-foreground underline-offset-4 hover:underline"
                >
                  Forgot password?
                </AppLink>
              </p>
            </div>
          )}
        </form.Field>
        <form.Field name="rememberMe">
          {(field) => (
            <div className="flex min-h-11 items-center gap-3">
              <Checkbox
                id="login-remember"
                checked={field.state.value}
                onCheckedChange={(checked) => field.handleChange(checked === true)}
              />
              <Label htmlFor="login-remember">Remember me</Label>
            </div>
          )}
        </form.Field>
        <form.Subscribe selector={(s) => s.isSubmitting}>
          {(isSubmitting) => (
            <Button type="submit" className="w-full" disabled={isSubmitting}>
              {(isSubmitting ? "Working…" : "Sign in") as string}
            </Button>
          )}
        </form.Subscribe>
      </form>;
    }

    @if (mode === "workos") {
      <Button type="button" className="w-full" onClick={() => startSso("workos")}>
        Continue with WorkOS
      </Button>;
    }

    @if (mode === "oidc") {
      <Button type="button" className="w-full" onClick={() => startSso("oidc")}>
        Continue with SSO
      </Button>;
    }

    @if (allowSignup) {
      <p className="text-[14px] text-muted-foreground">
        New to Oxidean?{" "}
        <AppLink
          href={signupHref as string}
          className="font-normal text-foreground underline-offset-4 hover:underline"
        >
          Create an account
        </AppLink>
      </p>;
    }
  </AuthShell>;
}
