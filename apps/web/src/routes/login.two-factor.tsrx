import { twoFactorStatusQueryOptions } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { AppPage } from "@/components/app-page";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { PasskeySection } from "@/components/settings/passkey-section";
import { TotpEnrollFlow } from "@/components/settings/totp-enroll-flow";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { safeReturnTo } from "@/lib/return-to";
import { authSessionQueryOptions } from "@/lib/session-queries";
import {
  browserSupportsWebAuthn,
  passkeyErrorMessage,
  runPasskeyAssertion,
} from "@/lib/two-factor";

const NETWORK_ERROR = networkErrorMessage();

/** SSO callbacks land here with `?return=`; the login page uses `?returnTo=`. */
function readChallengeReturnTo(): string {
  if (typeof window === "undefined") return "/";
  const params = new URLSearchParams(window.location.search);
  return safeReturnTo(params.get("return") ?? params.get("returnTo"));
}

/** Exported for login.two-factor.integration.test.ts (#130). */
export function TwoFactorLoginPage() @{
  <AppPage>
    <TwoFactorContent />
  </AppPage>;
}

function TwoFactorContent() @{
  const statusQ = useQuery(twoFactorStatusQueryOptions(apiClient));
  const meQ = useQuery(authSessionQueryOptions());
  const status = statusQ.data;
  const returnTo = readChallengeReturnTo();
  const supported = typeof window !== "undefined" && browserSupportsWebAuthn();

  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [emailSent, setEmailSent] = useState(false);
  const [enrollMethod, setEnrollMethod] = useState<"none" | "totp" | "passkey">("none");
  // Explicit challenge id — carried via `?challenge=` for robustness; the
  // HttpOnly `oxidean_2fa` cookie is the primary transport.
  const [challengeId] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    const raw = new URLSearchParams(window.location.search).get("challenge");
    return raw && raw.trim() ? raw.trim() : null;
  });

  // Already signed in (challenge completed elsewhere) → continue.
  useEffect(() => {
    if (meQ.data && status && !status.challenge_pending) {
      window.location.assign(
        meQ.data.must_change_credentials
          ? `/setup/credentials?returnTo=${encodeURIComponent(returnTo)}`
          : returnTo,
      );
    }
  }, [meQ.data, status]);

  function destFor(user: { must_change_credentials?: boolean | null }): string {
    return user.must_change_credentials
      ? `/setup/credentials?returnTo=${encodeURIComponent(returnTo)}`
      : returnTo;
  }

  function fatalChallengeError(codeName: string): boolean {
    return codeName === "auth.two_factor_expired" || codeName === "auth.rate_limited";
  }

  async function submitCode() {
    setPending(true);
    setError("");
    try {
      const res = await apiClient.twoFactor.complete({
        code: code.trim(),
        challenge: challengeId,
      });
      if (!res.ok) {
        if (fatalChallengeError(res.error.code)) {
          setError(`${res.error.message || "Challenge expired"} — sign in again.`);
        } else {
          setError(rpcErrorMessage(res.error));
        }
        setPending(false);
        return;
      }
      window.location.assign(destFor(res.data));
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function usePasskey() {
    setPending(true);
    setError("");
    try {
      const begin = await apiClient.twoFactor.passkeyBegin({ challenge: challengeId });
      if (!begin.ok) {
        setError(rpcErrorMessage(begin.error));
        setPending(false);
        return;
      }
      const assertion = await runPasskeyAssertion(begin.data.publicKey);
      const done = await apiClient.twoFactor.passkeyComplete({
        assertion,
        challenge: challengeId,
      });
      if (!done.ok) {
        setError(rpcErrorMessage(done.error));
        setPending(false);
        return;
      }
      window.location.assign(destFor(done.data));
    } catch (e) {
      setError(passkeyErrorMessage(e));
      setPending(false);
    }
  }

  async function sendEmailCode() {
    setPending(true);
    setError("");
    try {
      const res = await apiClient.twoFactor.emailSend({ challenge: challengeId });
      if (!res.ok) {
        setError(rpcErrorMessage(res.error));
        setPending(false);
        return;
      }
      setEmailSent(true);
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const methods = status?.challenge_methods ?? [];
  const has = (m: string) => methods.includes(m);
  const challenge = challengeId;
  const expired = status && !status.challenge_pending && !meQ.data;

  <AuthShell
    title="Two-factor authentication"
    support={
      status?.challenge_kind === "enroll"
        ? "This instance requires two-factor authentication — enroll a method to finish signing in."
        : "Enter a verification code or use a passkey to finish signing in."
    }
  >
    @if (statusQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading…</p>;
    } @else {
      @if (expired || statusQ.isError) {
        <>
          <p className="text-[16px] text-muted-foreground">
            This sign-in challenge is no longer active.
          </p>
          <AppLink
            href={`/login?returnTo=${encodeURIComponent(returnTo)}` as string}
            className="text-primary underline-offset-2 hover:underline"
          >
            Back to sign in
          </AppLink>
        </>;
      } @else {
        @if (status?.challenge_kind === "enroll") {
          <>
            @if (enrollMethod === "none") {
              <div className="flex flex-col gap-3">
                <p className="text-[14px] text-muted-foreground">
                  Choose a method to enroll. You can add the other later in Settings →
                  Security.
                </p>
                @if (has("totp")) {
                  <Button
                    type="button"
                    variant="outline"
                    className="justify-start"
                    onClick={() => setEnrollMethod("totp")}
                  >
                    Authenticator app (TOTP)
                  </Button>;
                }
                @if (has("passkey")) {
                  <Button
                    type="button"
                    variant="outline"
                    className="justify-start"
                    disabled={!supported}
                    onClick={() => setEnrollMethod("passkey")}
                  >
                    Passkey — Face ID, Touch ID, Windows Hello
                  </Button>;
                }
                @if (
                  !supported &&
                  has("passkey") &&
                  !has("totp")
                ) {
                  <p className="text-[14px] text-muted-foreground">
                    This browser does not support passkeys — use a device that
                    does to finish signing in.
                  </p>;
                }
              </div>;
            } @else {
              @if (enrollMethod === "totp") {
                <TotpEnrollFlow
                  challenge={challenge}
                  onFinished={() => window.location.assign(returnTo)}
                  onCancel={() => setEnrollMethod("none")}
                />;
              } @else {
                <PasskeySection
                  passkeys={[]}
                  challenge={challenge}
                  onEnrolled={() => window.location.assign(returnTo)}
                />;
              }
            }
          </>;
        } @else {
          <>
            <form
              className="flex flex-col gap-4"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void submitCode();
              }}
            >
              <div className="flex flex-col gap-2">
                <Label htmlFor="two-factor-code">Verification code</Label>
                <Input
                  id="two-factor-code"
                  name="two-factor-code"
                  inputMode="text"
                  autoComplete="one-time-code"
                  placeholder="6-digit code"
                  value={code}
                  onInput={(ev) => setCode((ev.currentTarget as HTMLInputElement).value)}
                  required
                />
                @if (has("recovery")) {
                  <p className="text-[13px] text-muted-foreground">
                    App code, 8-digit email code, or a recovery code (xxxxx-xxxxx) all
                    work here.
                  </p>;
                }
              </div>
              @if (error) {
                <AuthErrorBanner message={error} />;
              }
              <Button
                type="submit"
                className="w-full"
                disabled={pending || code.trim() === ""}
              >
                {(pending ? "Verifying…" : "Verify") as string}
              </Button>
            </form>

            <div className="flex flex-col gap-2 border-t border-border pt-4">
              @if (has("passkey") && supported) {
                <Button
                  type="button"
                  variant="ghost"
                  className="justify-start"
                  disabled={pending}
                  onClick={() => void usePasskey()}
                >
                  Use a passkey
                </Button>;
              }
              @if (has("email")) {
                <Button
                  type="button"
                  variant="ghost"
                  className="justify-start"
                  disabled={pending || emailSent}
                  onClick={() => void sendEmailCode()}
                >
                  {
                    (emailSent
                      ? "Sign-in code sent — check your email"
                      : "Email me a sign-in code") as string
                  }
                </Button>;
              }
              <p className="text-[13px] text-muted-foreground">
                <AppLink
                  href={`/login?returnTo=${encodeURIComponent(returnTo)}` as string}
                  className="text-foreground underline-offset-4 hover:underline"
                >
                  Use a different account
                </AppLink>
              </p>
            </div>
          </>;
        }
      }
    }
  </AuthShell>;
}
