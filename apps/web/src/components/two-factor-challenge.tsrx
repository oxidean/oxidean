import { twoFactorStatusQueryOptions, type UserPublic } from "@oxidean/api-client";
import { useQuery } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { PasskeySection } from "@/components/settings/passkey-section";
import { TotpEnrollFlow } from "@/components/settings/totp-enroll-flow";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { authSessionQueryOptions } from "@/lib/session-queries";
import {
  browserSupportsWebAuthn,
  passkeyErrorMessage,
  runPasskeyAssertion,
} from "@/lib/two-factor";

const NETWORK_ERROR = networkErrorMessage();

type TwoFactorChallengeProps = {
  /** Explicit challenge id; null falls back to the `oxidean_2fa` cookie. */
  challengeId: string | null;
  returnTo: string;
  /** Carried into the password leg's `remember_me`. */
  rememberMe: boolean;
  /** Session minted — `UserPublic` when the completing RPC returned one. */
  onDone: (user: UserPublic | null) => void;
  /** Abandon the challenge — "use a different account" / expired restart. */
  onRestart: () => void;
};

/**
 * The post-identifier legs of sign-in (#130), driven by the pending
 * challenge's `challenge_next`: `"factor"` (TOTP / email OTP / recovery /
 * passkey), `"password"` (the `auth.login` password leg — the challenge
 * binds the account so no identifier is re-sent), or `"enroll"`.
 * Rendered inside its own `AuthShell`; the host supplies `onDone`/`onRestart`.
 */
export function TwoFactorChallenge(props: TwoFactorChallengeProps) @{
  const statusQ = useQuery(twoFactorStatusQueryOptions(apiClient));
  const meQ = useQuery(authSessionQueryOptions());
  const status = statusQ.data;
  const supported = typeof window !== "undefined" && browserSupportsWebAuthn();

  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [emailSent, setEmailSent] = useState(false);
  const [enrollMethod, setEnrollMethod] = useState<"none" | "totp" | "passkey">("none");
  // `two_factor.complete` returning `next: "password"` flips the step
  // locally; a resumed challenge reports it via `challenge_next` instead.
  const [passwordStep, setPasswordStep] = useState(false);

  const challenge = props.challengeId;

  function fatalChallengeError(codeName: string): boolean {
    return codeName === "auth.two_factor_expired" || codeName === "auth.rate_limited";
  }

  async function submitCode() {
    setPending(true);
    setError("");
    try {
      const res = await apiClient.twoFactor.complete({
        code: code.trim(),
        challenge,
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
      if (res.data.next === "password") {
        setPassword("");
        setPasswordStep(true);
        setPending(false);
        return;
      }
      props.onDone(res.data.user ?? null);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function submitPassword() {
    setPending(true);
    setError("");
    try {
      const res = await apiClient.auth.login({
        password,
        remember_me: props.rememberMe,
        challenge,
      });
      if (!res.ok) {
        // Factor/enroll debt remains — refresh the challenge view.
        if (res.error.code === "auth.two_factor_required") {
          setPasswordStep(false);
          setPassword("");
          await statusQ.refetch();
          setPending(false);
          return;
        }
        if (fatalChallengeError(res.error.code)) {
          setError(`${res.error.message || "Challenge expired"} — sign in again.`);
        } else {
          setError(rpcErrorMessage(res.error));
        }
        setPending(false);
        return;
      }
      props.onDone(res.data);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function usePasskey() {
    setPending(true);
    setError("");
    try {
      const begin = await apiClient.twoFactor.passkeyBegin({ challenge });
      if (!begin.ok) {
        setError(rpcErrorMessage(begin.error));
        setPending(false);
        return;
      }
      const assertion = await runPasskeyAssertion(begin.data.publicKey);
      const done = await apiClient.twoFactor.passkeyComplete({
        assertion,
        challenge,
      });
      if (!done.ok) {
        setError(rpcErrorMessage(done.error));
        setPending(false);
        return;
      }
      props.onDone(done.data);
    } catch (e) {
      setError(passkeyErrorMessage(e));
      setPending(false);
    }
  }

  async function sendEmailCode() {
    setPending(true);
    setError("");
    try {
      const res = await apiClient.twoFactor.emailSend({ challenge });
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
  const expired = status && !status.challenge_pending && !meQ.data;
  const step =
    passwordStep || status?.challenge_next === "password"
      ? "password"
      : status?.challenge_next === "enroll"
        ? "enroll"
        : "factor";

  const shellTitle = step === "password" ? "Enter your password" : "Two-factor authentication";
  const shellSupport =
    step === "password"
      ? "Almost there — enter the account password to finish signing in."
      : status?.challenge_kind === "enroll"
        ? "This instance requires two-factor authentication — enroll a method to finish signing in."
        : "Enter a verification code or use a passkey to continue.";

  <AuthShell title={shellTitle as string} support={shellSupport as string}>
    @if (statusQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading…</p>;
    } @else {
      @if (expired || statusQ.isError) {
        <>
          <p className="text-[16px] text-muted-foreground">
            This sign-in challenge is no longer active.
          </p>
          <Button
            type="button"
            variant="link"
            className="justify-start px-0 text-primary"
            onClick={() => props.onRestart()}
          >
            Back to sign in
          </Button>
        </>;
      } @else {
        @if (step === "password") {
          <form
            className="flex flex-col gap-4"
            method="post"
            action="#"
            onSubmit={(e) => {
              e.preventDefault();
              void submitPassword();
            }}
          >
            <div className="flex flex-col gap-2">
              <Label htmlFor="two-factor-password">Password</Label>
              <Input
                id="two-factor-password"
                name="password"
                type="password"
                autoComplete="current-password"
                placeholder="Password"
                value={password}
                onInput={(ev) => setPassword((ev.currentTarget as HTMLInputElement).value)}
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
            @if (error) {
              <AuthErrorBanner message={error} />;
            }
            <Button type="submit" className="w-full" disabled={pending || password === ""}>
              {(pending ? "Verifying…" : "Sign in") as string}
            </Button>
          </form>;
        } @else {
          @if (step === "enroll") {
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
                    onFinished={() => props.onDone(null)}
                    onCancel={() => setEnrollMethod("none")}
                  />;
                } @else {
                  <PasskeySection
                    passkeys={[]}
                    challenge={challenge}
                    onEnrolled={() => props.onDone(null)}
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
                  <button
                    type="button"
                    className="text-foreground underline-offset-4 hover:underline"
                    onClick={() => props.onRestart()}
                  >
                    Use a different account
                  </button>
                </p>
              </div>
            </>;
          }
        }
      }
    }
  </AuthShell>;
}
