import {
  createFileRoute,
  isNotFound,
  notFound,
  redirect,
  useLoaderData,
} from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { ProviderMode } from "@oxidean/api-client";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { readReturnToFromLocation, safeReturnTo } from "@/lib/return-to";
import { fetchProviderConfig, fetchSessionMe } from "@/lib/ssr-auth";
import { AppLink } from "@/components/ui/app-link";

export type SignupLoaderData = {
  mode: ProviderMode;
  loadError: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const TAKEN_ERROR = "That email or username is already taken. Try another or sign in.";
const RESERVED_ERROR = "That username is reserved. Choose a different username.";
const SETUP_ERROR = "This instance needs setup first. Redirecting…";

export const Route = createFileRoute("/signup")({
  component: SignupPage,
  head: () => ({ meta: [{ title: "Sign up · Oxidean" }] }),
  beforeLoad: async () => {
    // D-06 / UI-SPEC: closed signup is SSR 404 — never soft AuthShell.
    try {
      const me = await fetchSessionMe();
      if (me.ok) {
        throw redirect({ href: "/" });
      }
    } catch (e) {
      if (e && typeof e === "object" && "href" in e) throw e;
    }
    try {
      const cfg = await fetchProviderConfig();
      if (cfg.ok && cfg.data.allow_signup === false) {
        throw notFound();
      }
    } catch (e) {
      if (isNotFound(e)) throw e;
      /* API unreachable — leave to loader fallback */
    }
  },
  loader: async (): Promise<SignupLoaderData> => {
    try {
      const cfg = await fetchProviderConfig();
      if (!cfg.ok) {
        return { mode: "local", loadError: NETWORK_ERROR };
      }
      return { mode: cfg.data.mode, loadError: "" };
    } catch {
      return { mode: "local", loadError: NETWORK_ERROR };
    }
  },
});

/** Exported for AUTH-05 integration tests (no invite UI). */
export function SignupPage() @{
  const data = useLoaderData({ from: "/signup" }) as SignupLoaderData | undefined;
  const mode = data?.mode ?? "local";
  const loadError = data?.loadError ?? "";
  const [error, setError] = useState("");

  const returnTo = readReturnToFromLocation();
  const loginHref = `/login?returnTo=${encodeURIComponent(returnTo)}`;

  const form = useForm({
    defaultValues: {
      email: "",
      username: "",
      password: "",
      confirm: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (value.password !== value.confirm) {
        setError("Passwords do not match. Fix the highlighted fields and try again.");
        return;
      }
      try {
        const res = await apiClient.auth.signup({
          email: value.email.trim(),
          username: value.username.trim(),
          password: value.password,
        });
        if (!res.ok) {
          const code = res.error.code;
          if (code === "auth.taken") setError(TAKEN_ERROR);
          else if (code === "auth.reserved_username") setError(RESERVED_ERROR);
          else if (code === "auth.setup_required") {
            setError(SETUP_ERROR);
            window.location.assign("/setup");
            return;
          } else if (
            code === "auth.invalid_username" ||
            code === "auth.invalid_email" ||
            code === "rpc.bad_input"
          ) {
            setError(`${res.error.message}. Fix the highlighted fields and try again.`);
          } else {
            setError(NETWORK_ERROR);
          }
          return;
        }
        window.location.assign(safeReturnTo(returnTo));
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
    mode === "workos" || mode === "oidc"
      ? "You'll finish signup with your identity provider."
      : "Email, username, and password — handles are letters, digits, and hyphens.";

  <AuthShell title="Create your account" support={support}>
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
        <form.Field name="email">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="signup-email">Email</Label>
              <Input
                id="signup-email"
                name="email"
                type="email"
                autoComplete="email"
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
        <form.Field name="username">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="signup-username">Username</Label>
              <Input
                id="signup-username"
                name="username"
                autoComplete="username"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLInputElement).value)
                }
                required
              />
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                1–39 characters; letters, numbers, and hyphens. Can't start or end with a
                hyphen.
              </p>
            </div>
          )}
        </form.Field>
        <form.Field name="password">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="signup-password">Password</Label>
              <Input
                id="signup-password"
                name="password"
                type="password"
                autoComplete="new-password"
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
        <form.Field name="confirm">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="signup-confirm">Confirm password</Label>
              <Input
                id="signup-confirm"
                name="confirm"
                type="password"
                autoComplete="new-password"
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
        <form.Subscribe selector={(s) => s.isSubmitting}>
          {(isSubmitting) => (
            <Button type="submit" className="w-full" disabled={isSubmitting}>
              {(isSubmitting ? "Working…" : "Create account") as string}
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

    <p className="text-[14px] text-muted-foreground">
      Already have an account?{" "}
      <AppLink
        href={loginHref as string}
        className="font-normal text-foreground underline-offset-4 hover:underline"
      >
        Sign in
      </AppLink>
    </p>
  </AuthShell>;
}
