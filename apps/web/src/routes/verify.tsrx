import { createFileRoute } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useRef, useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AuthFormSkeleton } from "@/components/page-skeletons";
import { Button } from "@/components/ui/button";
import { InputOtp } from "@/components/ui/input-otp";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { useAppNavigate } from "@/lib/app-navigate";
import { safeReturnTo } from "@/lib/return-to";
import { AppLink } from "@/components/ui/app-link";

export const Route = createFileRoute("/verify")({
  component: VerifyPage,
  head: () => ({
    meta: [{ title: "Verify email · Oxidean" }, { name: "referrer", content: "no-referrer" }],
  }),
});

const SUPPORT_DEFAULT =
  "We sent a link and an 8-digit code to your email. Enter the code below, or open the link from the same device after signing in. Codes also work for additional addresses on your account.";
const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const INVALID_EXPIRED =
  "That code or link is invalid or expired. Request a new email and try again.";
const WRONG_USER =
  "You're signed in as a different user. Sign out, then open the link again or enter the code while signed in as the mailbox owner.";
const NEED_SIGN_IN = "Sign in to finish verifying this email.";
const ALREADY_VERIFIED = "Your email is already verified.";
const RESEND_SUCCESS = "Verification email sent. Check your inbox.";
const RESEND_RATE_LIMITED = "Please wait before requesting another email.";
const RESEND_COOLDOWN_HELPER = "Resend available shortly";
const RESEND_COOLDOWN_SECS = 60;

function readTokenFromLocation(): string | null {
  if (typeof window === "undefined") return null;
  const raw = new URLSearchParams(window.location.search).get("token");
  if (!raw) return null;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function loginHrefWithToken(token: string | null): string {
  const verifyPath = token ? `/verify?token=${encodeURIComponent(token)}` : "/verify";
  return `/login?returnTo=${encodeURIComponent(verifyPath)}`;
}

/** After redeem: honor returnTo; else Account when primary was already verified (secondary). */
function verifySuccessHref(primaryAlreadyVerified: boolean): string {
  if (typeof window === "undefined") {
    return primaryAlreadyVerified ? "/settings/profile" : "/";
  }
  const raw = new URLSearchParams(window.location.search).get("returnTo");
  if (raw != null && raw.trim() !== "") {
    return safeReturnTo(raw);
  }
  return primaryAlreadyVerified ? "/settings/profile" : "/";
}

type Phase =
  | { kind: "loading" }
  | { kind: "need-sign-in"; token: string | null }
  | { kind: "already-verified" }
  | {
      kind: "form";
      user: UserPublic;
      token: string | null;
      verifyingToken: boolean;
      primaryAlreadyVerified: boolean;
    };

/** Exported for verify.integration.test.ts (RESEARCH P1 / D-QH-03). */
export function VerifyPage() @{
  const navigateApp = useAppNavigate();
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [error, setError] = useState("");
  const [tokenPending, setTokenPending] = useState(false);
  const [resendPending, setResendPending] = useState(false);
  const [resendNote, setResendNote] = useState("");
  const [cooldownUntil, setCooldownUntil] = useState(0);
  const [cooldownLeft, setCooldownLeft] = useState(0);
  const tokenAttempted = useRef(false);
  const submitLock = useRef(false);

  const form = useForm({
    defaultValues: { code: "" },
    onSubmit: async ({ value }) => {
      if (submitLock.current) return;
      const trimmed = value.code.trim();
      if (trimmed.length !== 8 || !/^\d{8}$/.test(trimmed)) return;
      submitLock.current = true;
      setError("");
      const primaryAlreadyVerified = phase.kind === "form" ? phase.primaryAlreadyVerified : false;
      try {
        const res = await apiClient.auth.verify({ code: trimmed });
        if (!res.ok) {
          setError(
            res.error.code === "auth.invalid_token"
              ? INVALID_EXPIRED
              : res.error.message || INVALID_EXPIRED,
          );
          submitLock.current = false;
          return;
        }
        void navigateApp(verifySuccessHref(primaryAlreadyVerified));
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
        const me = await apiClient.auth.me();
        if (cancelled) return;
        if (!me.ok) {
          setPhase({ kind: "need-sign-in", token });
          return;
        }
        // Primary verified + no token → nothing to redeem here (secondaries use Account OTP
        // or a magic link with ?token=).
        if (me.data.email_verified && !token) {
          setPhase({ kind: "already-verified" });
          return;
        }
        setPhase({
          kind: "form",
          user: me.data,
          token,
          verifyingToken: Boolean(token),
          primaryAlreadyVerified: me.data.email_verified,
        });
      } catch {
        if (cancelled) return;
        setError(NETWORK_ERROR);
        setPhase({ kind: "need-sign-in", token });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (phase.kind !== "form" || !phase.verifyingToken || !phase.token) return;
    if (tokenAttempted.current) return;
    tokenAttempted.current = true;
    const token = phase.token;
    const primaryAlreadyVerified = phase.primaryAlreadyVerified;
    let cancelled = false;
    void (async () => {
      setTokenPending(true);
      setError("");
      try {
        const res = await apiClient.auth.verify({ token });
        if (cancelled) return;
        if (!res.ok) {
          setError(res.error.code === "auth.invalid_token" ? WRONG_USER : INVALID_EXPIRED);
          setPhase({
            kind: "form",
            user: phase.user,
            token: null,
            verifyingToken: false,
            primaryAlreadyVerified,
          });
          setTokenPending(false);
          return;
        }
        void navigateApp(verifySuccessHref(primaryAlreadyVerified));
      } catch {
        if (cancelled) return;
        setError(NETWORK_ERROR);
        setPhase({
          kind: "form",
          user: phase.user,
          token: null,
          verifyingToken: false,
          primaryAlreadyVerified,
        });
        setTokenPending(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [phase]);

  useEffect(() => {
    if (cooldownUntil <= 0) {
      setCooldownLeft(0);
      return;
    }
    const tick = () => {
      const left = Math.max(0, Math.ceil((cooldownUntil - Date.now()) / 1000));
      setCooldownLeft(left);
      if (left <= 0) setCooldownUntil(0);
    };
    tick();
    const id = window.setInterval(tick, 250);
    return () => window.clearInterval(id);
  }, [cooldownUntil]);

  async function resend() {
    if (resendPending || cooldownLeft > 0) return;
    setResendPending(true);
    setResendNote("");
    setError("");
    try {
      const res = await apiClient.auth.resendVerify();
      if (!res.ok) {
        if (res.error.code === "auth.rate_limited") {
          setResendNote(RESEND_RATE_LIMITED);
          setCooldownUntil(Date.now() + RESEND_COOLDOWN_SECS * 1000);
        } else {
          setError(res.error.message || NETWORK_ERROR);
        }
        setResendPending(false);
        return;
      }
      setResendNote(RESEND_SUCCESS);
      setCooldownUntil(Date.now() + RESEND_COOLDOWN_SECS * 1000);
      setResendPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setResendPending(false);
    }
  }

  async function submitCode(nextCode: string) {
    form.setFieldValue("code", nextCode);
    await form.handleSubmit();
  }

  const verifying =
    (phase.kind === "form" && phase.verifyingToken) || tokenPending || form.state.isSubmitting;
  const cooldownHelper =
    cooldownLeft > 0
      ? `You can resend in ${cooldownLeft}s`
      : resendNote === RESEND_RATE_LIMITED
        ? RESEND_COOLDOWN_HELPER
        : "";

  @if (phase.kind === "loading") {
    <AuthShell title="Verify your email" support="Checking your session…">
      <AuthFormSkeleton fields={1} />
    </AuthShell>;
  } @else {
    @if (phase.kind === "need-sign-in") {
      <AuthShell title="Verify your email" support={NEED_SIGN_IN}>
        @if (error) {
          <AuthErrorBanner message={error} />;
        }
        <Button
          type="button"
          className="w-full"
          onClick={() => {
            void navigateApp(loginHrefWithToken(phase.token));
          }}
        >
          Sign in
        </Button>
        <p className="text-[14px] text-muted-foreground">
          Need a different account?{" "}
          <AppLink
            href={loginHrefWithToken(phase.token) as string}
            className="font-normal text-foreground underline-offset-4 hover:underline"
          >
            Sign in
          </AppLink>
        </p>
      </AuthShell>;
    } @else {
      @if (phase.kind === "already-verified") {
        <AuthShell title="Verify your email" support={ALREADY_VERIFIED}>
          <Button
            type="button"
            className="w-full"
            onClick={() => {
              void navigateApp("/");
            }}
          >
            Go to home
          </Button>
        </AuthShell>;
      } @else {
        <AuthShell title="Verify your email" support={SUPPORT_DEFAULT}>
          @if (error) {
            <AuthErrorBanner message={error} />;
          }
          @if (phase.verifyingToken) {
            <p className="text-[16px] text-muted-foreground">Verifying…</p>;
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
            <form.Field name="code">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="verify-code">Verification code</Label>
                  <InputOtp
                    id="verify-code"
                    name="code"
                    value={field.state.value}
                    onChange={(next) => {
                      field.handleChange(next);
                      if (error) setError("");
                    }}
                    onComplete={(next) => {
                      void submitCode(next);
                    }}
                    disabled={verifying}
                    aria-label="Verification code"
                    aria-invalid={Boolean(error)}
                  />
                </div>
              )}
            </form.Field>
            <form.Subscribe
              selector={(s) => ({ isSubmitting: s.isSubmitting, code: s.values.code })}
            >
              {(state) => (
                <Button
                  type="submit"
                  className="w-full"
                  disabled={verifying || state.code.length !== 8}
                >
                  {(verifying ? "Working…" : "Verify email") as string}
                </Button>
              )}
            </form.Subscribe>
          </form>

          <div className="flex flex-col gap-2">
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              disabled={verifying || resendPending || cooldownLeft > 0}
              onClick={() => {
                void resend();
              }}
            >
              {(resendPending ? "Working…" : "Resend email") as string}
            </Button>
            @if (resendNote && resendNote !== RESEND_RATE_LIMITED) {
              <p className="text-[14px] text-muted-foreground" role="status">
                {resendNote as string}
              </p>;
            }
            @if (cooldownHelper) {
              <p className="text-[14px] text-muted-foreground">{cooldownHelper as string}</p>;
            }
            @if (resendNote === RESEND_RATE_LIMITED) {
              <p className="text-[14px] text-muted-foreground">{RESEND_RATE_LIMITED}</p>;
            }
          </div>

          <p className="text-[14px] text-muted-foreground">
            Need a different account?{" "}
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
