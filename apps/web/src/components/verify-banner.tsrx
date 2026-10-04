import { useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { toastError, toastSuccess } from "@/lib/toast";
import { AppLink } from "@/components/ui/app-link";

const BANNER_MESSAGE =
  "Verify your email to unlock creating repositories and other privileged actions.";
const RESEND_SUCCESS = "Verification email sent.";
const RESEND_RATE_LIMITED = "Please wait before requesting another email.";
const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const RESEND_COOLDOWN_SECS = 60;

/**
 * Persistent verify callout for signed-in unverified sessions (D-11, D-13).
 * Fixed bottom-right overlay — does not insert into document flow (no layout shift).
 * Resend feedback uses Base UI toasts so the callout stays calm.
 */
export function VerifyBanner() {
  const me = useQuery(authSessionQueryOptions());
  const [resendPending, setResendPending] = useState(false);
  const [cooldownUntil, setCooldownUntil] = useState(0);
  const [cooldownLeft, setCooldownLeft] = useState(0);

  const ready = !me.isPending;
  const visible = Boolean(me.isSuccess && me.data && me.data.email_verified === false);

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
    try {
      const res = await apiClient.auth.resendVerify();
      if (!res.ok) {
        if (res.error.code === "auth.rate_limited") {
          toastError(RESEND_RATE_LIMITED);
          setCooldownUntil(Date.now() + RESEND_COOLDOWN_SECS * 1000);
        } else {
          toastError(res.error.message || NETWORK_ERROR);
        }
        setResendPending(false);
        return;
      }
      toastSuccess(RESEND_SUCCESS, "Check your inbox for a link and 8-digit code.");
      setCooldownUntil(Date.now() + RESEND_COOLDOWN_SECS * 1000);
      setResendPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setResendPending(false);
    }
  }

  if (!ready || !visible) return null;

  const resendLabel = resendPending
    ? "Working…"
    : cooldownLeft > 0
      ? `Resend in ${cooldownLeft}s`
      : "Resend email";

  return (
    <aside
      role="status"
      aria-label="Email verification"
      className="oct-verify-callout fixed bottom-4 right-4 z-50 w-[min(100%-2rem,22rem)] rounded-lg border border-border bg-card/95 p-4 shadow-lg backdrop-blur-sm"
    >
      <div className="flex gap-3">
        <span
          aria-hidden="true"
          className="mt-0.5 w-1 shrink-0 self-stretch rounded-full bg-primary"
        />
        <div className="min-w-0 flex-1">
          <p className="text-[14px] font-medium leading-snug text-foreground">Verify your email</p>
          <p className="mt-1 text-[13px] font-normal leading-snug text-muted-foreground break-words">
            {BANNER_MESSAGE}
          </p>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="secondary"
              size="default"
              className="h-10 px-3 text-[13px]"
              disabled={resendPending || cooldownLeft > 0}
              onClick={() => {
                void resend();
              }}
            >
              {resendLabel}
            </Button>
            <AppLink
              href="/verify"
              className="inline-flex h-10 items-center rounded-md px-3 text-[13px] font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Enter code
            </AppLink>
          </div>
        </div>
      </div>
    </aside>
  );
}
