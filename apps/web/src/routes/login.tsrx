import { twoFactorStatusQueryOptions } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { useQuery } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { TwoFactorChallenge } from "@/components/two-factor-challenge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { AppPage } from "@/components/app-page";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { readReturnToFromLocation, safeReturnTo } from "@/lib/return-to";
import { authProviderConfigQueryOptions, authSessionQueryOptions } from "@/lib/session-queries";
import { AppLink } from "@/components/ui/app-link";
import { navigate } from "@/lib/nav";

const NETWORK_ERROR = networkErrorMessage();

export function LoginPage() @{
  <AppPage>
    <LoginContent />
  </AppPage>;
}

function LoginContent() @{
  const cfgQ = useQuery(authProviderConfigQueryOptions());
  const meQ = useQuery(authSessionQueryOptions());
  const statusQ = useQuery(twoFactorStatusQueryOptions(apiClient));
  const [error, setError] = useState("");

  const returnTo = readReturnToFromLocation();
  const signupHref = `/signup?returnTo=${encodeURIComponent(returnTo)}`;

  // Identifier-first flow (#130): "identifier" collects email/username +
  // remember-me, `auth.login_begin` opens the challenge, then the inline
  // TwoFactorChallenge runs the factor/password/enroll legs. A pending
  // challenge (cookie) resumes mid-flow on reload unless abandoned.
  const [step, setStep] = useState<"identifier" | "challenge">("identifier");
  const [challengeId, setChallengeId] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    const raw = new URLSearchParams(window.location.search).get("challenge");
    return raw && raw.trim() ? raw.trim() : null;
  });
  const [rememberMe, setRememberMe] = useState(false);
  const [abandoned, setAbandoned] = useState(false);

  // Signed-in users bounce off the auth pages (the middleware covers the
  // cold-load case via the presence cookie; this catches stale-cookie and
  // client-initiated states).
  useEffect(() => {
    const me = meQ.data;
    if (me) navigate(me.must_change_credentials ? "/setup/credentials" : "/");
  });

  // Resume a live challenge: a reload mid-flow, or a `?challenge=` link.
  useEffect(() => {
    if (!abandoned && step === "identifier" && statusQ.data?.challenge_pending) {
      setStep("challenge");
    }
  });

  const form = useForm({
    defaultValues: {
      identifier: "",
      rememberMe: false,
    },
    onSubmit: async ({ value }) => {
      setError("");
      try {
        const res = await apiClient.auth.loginBegin({
          identifier: value.identifier.trim(),
          remember_me: value.rememberMe,
        });
        if (!res.ok) {
          setError(rpcErrorMessage(res.error));
          return;
        }
        setRememberMe(value.rememberMe);
        setChallengeId(res.data.challenge);
        setAbandoned(false);
        setStep("challenge");
      } catch {
        setError(NETWORK_ERROR);
      }
    },
  });

  function challengeDone(user: { must_change_credentials?: boolean | null } | null) {
    const dest =
      user?.must_change_credentials === true
        ? `/setup/credentials?returnTo=${encodeURIComponent(returnTo)}`
        : safeReturnTo(returnTo);
    window.location.assign(dest);
  }

  function challengeRestart() {
    setAbandoned(true);
    setChallengeId(null);
    setStep("identifier");
  }

  function startSso(kind: "workos" | "oidc") {
    const path = kind === "workos" ? "/api/auth/workos/start" : "/api/auth/oidc/start";
    window.location.assign(`${path}?returnTo=${encodeURIComponent(returnTo)}`);
  }

  const cfg = cfgQ.data;
  const mode = cfg?.mode ?? "local";
  const allowSignup = cfg?.allow_signup === true;
  const loadError = cfgQ.isError ? rpcErrorMessage(cfgQ.error) : "";

  const support =
    mode === "workos"
      ? "Continue with WorkOS to access this Oxidean instance."
      : mode === "oidc"
        ? "Continue with your organization’s SSO to access this instance."
        : "Use your email or username to continue.";

  @if (step === "challenge") {
    <TwoFactorChallenge
      challengeId={challengeId}
      returnTo={returnTo}
      rememberMe={rememberMe}
      onDone={(user) => challengeDone(user)}
      onRestart={() => challengeRestart()}
    />;
  } @else {
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
                {(isSubmitting ? "Working…" : "Continue") as string}
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
}
