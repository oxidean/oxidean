import { useForm } from "@octanejs/tanstack-form";
import type { ProviderMode } from "@oxidean/api-client";
import { useEffect, useRef, useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AppPage } from "@/components/app-page";
import { AuthFormSkeleton } from "@/components/page-skeletons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { InputOtp } from "@/components/ui/input-otp";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import {
  INVALID_EXPIRED_CODE,
  INVALID_EXPIRED_TOKEN,
  MISMATCH,
  NETWORK_ERROR,
  PASSWORD_HELPER,
  REDEEM_SUPPORT_CODE,
  REDEEM_SUPPORT_TOKEN,
  REQUEST_SUPPORT,
  SSO_MODE_BODY,
  SSO_ONLY,
  SUCCESS_BODY,
  SUCCESS_HEADING,
  TOO_SHORT,
} from "@/lib/reset-password-copy";
import { readReturnToFromLocation, safeReturnTo } from "@/lib/return-to";
import { AppLink } from "@/components/ui/app-link";

function readTokenFromLocation(): string | null {
  if (typeof window === "undefined") return null;
  const raw = new URLSearchParams(window.location.search).get("token");
  if (!raw) return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
}

type View =
  | { kind: "loading" }
  | { kind: "sso"; mode: "workos" | "oidc" }
  | { kind: "request" }
  | { kind: "request-sent" }
  | { kind: "redeem"; token: string | null };

/** `/reset-password` — request + redeem flow (title/meta via shell). */
export function ResetPasswordPage() @{
  <AppPage>
    <ResetPasswordContent />
  </AppPage>;
}

/** Exported for integration tests (AUTH-12 anti-enumeration + redeem UX). */
function ResetPasswordContent() @{
  const [view, setView] = useState<View>({ kind: "loading" });
  const [error, setError] = useState("");
  const [mismatchFields, setMismatchFields] = useState(false);
  const submitLock = useRef(false);

  const requestForm = useForm({
    defaultValues: { email: "" },
    onSubmit: async ({ value }) => {
      const trimmed = value.email.trim();
      if (!trimmed) return;
      setError("");
      try {
        const res = await apiClient.auth.requestPasswordReset({ email: trimmed });
        if (!res.ok) {
          if (res.error.code === "rpc.bad_input" || res.error.code === "auth.invalid_email") {
            setError(`${res.error.message}. Fix the highlighted fields and try again.`);
          } else {
            setError(NETWORK_ERROR);
          }
          return;
        }
        setView({ kind: "request-sent" });
      } catch {
        setError(NETWORK_ERROR);
      }
    },
  });

  const redeemForm = useForm({
    defaultValues: { code: "", password: "", confirm: "" },
    onSubmit: async ({ value }) => {
      if (submitLock.current || view.kind !== "redeem") return;
      const pwd = value.password;
      const conf = value.confirm;
      if (pwd.length < 8) {
        setError(TOO_SHORT);
        setMismatchFields(false);
        return;
      }
      if (pwd !== conf) {
        setError(MISMATCH);
        setMismatchFields(true);
        return;
      }
      const token = view.token;
      const nextCode = value.code.trim();
      if (!token && (nextCode.length !== 8 || !/^\d{8}$/.test(nextCode))) {
        setError(INVALID_EXPIRED_CODE);
        return;
      }

      submitLock.current = true;
      setError("");
      setMismatchFields(false);
      try {
        const res = await apiClient.auth.resetPassword({
          token: token || null,
          code: token ? null : nextCode.length === 8 ? nextCode : null,
          password: pwd,
        });
        if (!res.ok) {
          setError(mapRedeemError(res.error.code, res.error.message, Boolean(token)));
          submitLock.current = false;
          return;
        }
        window.location.assign(safeReturnTo(readReturnToFromLocation()));
      } catch {
        setError(NETWORK_ERROR);
        submitLock.current = false;
      }
    },
  });

  useEffect(() => {
    let cancelled = false;
    const token = readTokenFromLocation();
    void (async () => {
      try {
        const cfg = await apiClient.auth.providerConfig();
        if (cancelled) return;
        if (!cfg.ok) {
          setError(NETWORK_ERROR);
          setView(token ? { kind: "redeem", token } : { kind: "request" });
          return;
        }
        const mode: ProviderMode = cfg.data.mode;
        if (mode === "workos" || mode === "oidc") {
          setView({ kind: "sso", mode });
          return;
        }
        setView(token ? { kind: "redeem", token } : { kind: "request" });
      } catch {
        if (cancelled) return;
        setError(NETWORK_ERROR);
        setView(token ? { kind: "redeem", token } : { kind: "request" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  function startSso(kind: "workos" | "oidc") {
    const returnTo = readReturnToFromLocation();
    const path = kind === "workos" ? "/api/auth/workos/start" : "/api/auth/oidc/start";
    window.location.assign(`${path}?returnTo=${encodeURIComponent(returnTo)}`);
  }

  function mapRedeemError(code: string, message: string, viaToken: boolean): string {
    if (code === "auth.sso_only") return SSO_ONLY;
    if (code === "auth.weak_password") return TOO_SHORT;
    if (code === "auth.invalid_token") {
      return viaToken ? INVALID_EXPIRED_TOKEN : INVALID_EXPIRED_CODE;
    }
    return message || (viaToken ? INVALID_EXPIRED_TOKEN : INVALID_EXPIRED_CODE);
  }

  async function redeemWithCode(nextCode: string) {
    redeemForm.setFieldValue("code", nextCode);
    await redeemForm.handleSubmit();
  }

  const viaToken = view.kind === "redeem" ? Boolean(view.token) : false;

  @if (view.kind === "loading") {
    <AuthShell title="Reset your password" support="Preparing reset…">
      <AuthFormSkeleton fields={1} />
    </AuthShell>;
  } @else {
    @if (view.kind === "sso") {
      <AuthShell title="Reset your password" support={SSO_MODE_BODY}>
        @if (error) {
          <AuthErrorBanner message={error} />;
        }
        <Button type="button" className="w-full" onClick={() => startSso(view.mode)}>
          {(view.mode === "workos" ? "Continue with WorkOS" : "Continue with SSO") as string}
        </Button>
        <p className="text-[14px] text-muted-foreground">
          Remembered it?{" "}
          <AppLink
            href="/login"
            className="font-normal text-foreground underline-offset-4 hover:underline"
          >
            Sign in
          </AppLink>
        </p>
      </AuthShell>;
    } @else {
      @if (view.kind === "request" || view.kind === "request-sent") {
        <AuthShell title="Reset your password" support={REQUEST_SUPPORT}>
          @if (error) {
            <AuthErrorBanner message={error} />;
          }

          @if (view.kind === "request-sent") {
            <div className="flex max-w-md flex-col gap-2" role="status">
              <p className="text-[16px] font-normal leading-[1.5] text-foreground">
                {SUCCESS_HEADING}
              </p>
              <p className="text-[16px] font-normal leading-[1.5] text-muted-foreground">
                {SUCCESS_BODY}
              </p>
              <Button
                type="button"
                className="mt-2 w-full"
                onClick={() => {
                  setError("");
                  setView({ kind: "redeem", token: null });
                }}
              >
                Enter reset code
              </Button>
            </div>;
          } @else {
            <form
              className="flex flex-col gap-4"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void requestForm.handleSubmit();
              }}
            >
              <requestForm.Field name="email">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="reset-email">Email</Label>
                    <Input
                      id="reset-email"
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
              </requestForm.Field>
              <requestForm.Subscribe selector={(s) => s.isSubmitting}>
                {(isSubmitting) => (
                  <Button type="submit" className="w-full" disabled={isSubmitting}>
                    {(isSubmitting ? "Working…" : "Send reset email") as string}
                  </Button>
                )}
              </requestForm.Subscribe>
            </form>;
          }

          @if (view.kind === "request") {
            <p className="text-[14px] text-muted-foreground">
              Already have a code?{" "}
              <button
                type="button"
                className="font-normal text-foreground underline-offset-4 hover:underline"
                onClick={() => {
                  setError("");
                  setView({ kind: "redeem", token: null });
                }}
              >
                Enter it here
              </button>
            </p>;
          }

          <p className="text-[14px] text-muted-foreground">
            Remembered it?{" "}
            <AppLink
              href="/login"
              className="font-normal text-foreground underline-offset-4 hover:underline"
            >
              Sign in
            </AppLink>
          </p>
        </AuthShell>;
      } @else {
        <AuthShell
          title="Choose a new password"
          support={viaToken ? REDEEM_SUPPORT_TOKEN : REDEEM_SUPPORT_CODE}
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
              void redeemForm.handleSubmit();
            }}
          >
            @if (!viaToken) {
              <redeemForm.Field name="code">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="reset-code">Reset code</Label>
                    <InputOtp
                      id="reset-code"
                      name="code"
                      value={field.state.value}
                      onChange={(next) => {
                        field.handleChange(next);
                        if (error) setError("");
                      }}
                      onComplete={(next) => {
                        void redeemWithCode(next);
                      }}
                      aria-label="Reset code"
                      aria-invalid={Boolean(error)}
                    />
                  </div>
                )}
              </redeemForm.Field>;
            }

            <redeemForm.Field name="password">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="reset-password">New password</Label>
                  <Input
                    id="reset-password"
                    name="password"
                    type="password"
                    autoComplete="new-password"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(ev) => {
                      field.handleChange((ev.currentTarget as HTMLInputElement).value);
                      if (mismatchFields) setMismatchFields(false);
                      if (error) setError("");
                    }}
                    required
                    aria-invalid={mismatchFields || error === TOO_SHORT}
                    aria-describedby="reset-password-helper"
                  />
                  <p
                    id="reset-password-helper"
                    className="text-[14px] font-normal leading-[1.4] text-muted-foreground"
                  >
                    {PASSWORD_HELPER}
                  </p>
                </div>
              )}
            </redeemForm.Field>

            <redeemForm.Field name="confirm">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="reset-confirm">Confirm password</Label>
                  <Input
                    id="reset-confirm"
                    name="confirm"
                    type="password"
                    autoComplete="new-password"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(ev) => {
                      field.handleChange((ev.currentTarget as HTMLInputElement).value);
                      if (mismatchFields) setMismatchFields(false);
                      if (error) setError("");
                    }}
                    required
                    aria-invalid={mismatchFields}
                  />
                </div>
              )}
            </redeemForm.Field>

            <redeemForm.Subscribe
              selector={(s) => ({
                isSubmitting: s.isSubmitting,
                password: s.values.password,
                confirm: s.values.confirm,
                code: s.values.code,
              })}
            >
              {(state) => {
                const canSubmit =
                  state.password.length >= 8 &&
                  state.password === state.confirm &&
                  (viaToken || state.code.length === 8);
                return (
                  <Button
                    type="submit"
                    className="w-full"
                    disabled={state.isSubmitting || !canSubmit}
                  >
                    {(state.isSubmitting ? "Working…" : "Update password") as string}
                  </Button>
                );
              }}
            </redeemForm.Subscribe>
          </form>

          <p className="text-[14px] text-muted-foreground">
            Remembered it?{" "}
            <AppLink
              href="/login"
              className="font-normal text-foreground underline-offset-4 hover:underline"
            >
              Sign in
            </AppLink>
          </p>
        </AuthShell>;
      }
    }
  }
}
