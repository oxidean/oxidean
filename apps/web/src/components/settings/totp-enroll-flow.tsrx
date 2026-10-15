import { useEffect, useState } from "octane";
import QRCode from "qrcode";
import type { TotpEnrollmentPublic } from "@oxidean/api-client";
import { AuthErrorBanner } from "@/components/auth-shell";
import { RecoveryCodesView } from "@/components/settings/recovery-codes-view";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";

type TotpEnrollFlowProps = {
  /**
   * Enroll-kind challenge id — when set, confirming the code also completes
   * the gated login (the API mints the session).
   */
  challenge?: string | null;
  /** Called once the recovery-code reveal is acknowledged. */
  onFinished: () => void;
  onCancel?: () => void;
};

function groupSecret(secret: string): string {
  return secret
    .replace(/\s+/g, "")
    .replace(/(.{4})/g, "$1 ")
    .trim();
}

/**
 * Authenticator-app enrollment (#130): `totp_begin` → QR + manual key →
 * `totp_confirm` → one-time recovery-code reveal. Used from Security settings
 * (session actor) and the post-login enroll challenge (challenge actor).
 */
export function TotpEnrollFlow(props: TotpEnrollFlowProps) @{
  const [enroll, setEnroll] = useState<TotpEnrollmentPublic | null>(null);
  const [qrUrl, setQrUrl] = useState("");
  const [code, setCode] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [pending, setPending] = useState(false);

  async function begin() {
    setLoadError("");
    setEnroll(null);
    setQrUrl("");
    try {
      const res = await apiClient.twoFactor.totpBegin();
      if (!res.ok) {
        setLoadError(rpcErrorMessage(res.error));
        return;
      }
      setEnroll(res.data);
      try {
        setQrUrl(await QRCode.toDataURL(res.data.otpauth_uri, { margin: 1, width: 176 }));
      } catch {
        // QR rendering is best-effort — the manual key below always works.
        setQrUrl("");
      }
    } catch {
      setLoadError(networkErrorMessage());
    }
  }

  useEffect(() => {
    void begin();
  }, []);

  async function confirm() {
    if (!enroll) return;
    setPending(true);
    setError("");
    try {
      const res = await apiClient.twoFactor.totpConfirm({
        code: code.trim(),
        challenge: props.challenge ?? null,
      });
      if (!res.ok) {
        setError(rpcErrorMessage(res.error));
        setPending(false);
        return;
      }
      setCodes(res.data.recovery_codes);
      setPending(false);
    } catch {
      setError(networkErrorMessage());
      setPending(false);
    }
  }

  <div className="flex flex-col gap-4">
    @if (loadError) {
      <>
        <AuthErrorBanner message={loadError} />
        <div className="flex flex-wrap gap-3">
          <Button type="button" variant="outline" onClick={() => void begin()}>
            Try again
          </Button>
          @if (props.onCancel) {
            <Button type="button" variant="ghost" onClick={() => props.onCancel?.()}>
              Cancel
            </Button>;
          }
        </div>
      </>;
    } @else {
      @if (codes) {
        <RecoveryCodesView codes={codes} onDone={() => props.onFinished()} />;
      } @else {
        @if (!enroll) {
          <p className="text-[14px] text-muted-foreground">Starting authenticator setup…</p>;
        } @else {
          <>
            <p className="text-[14px] leading-[1.5] text-muted-foreground">
              Scan the QR code with an authenticator app (1Password, Bitwarden, Authy, Google
              Authenticator…), then enter the 6-digit code it shows.
            </p>
            <div className="flex flex-wrap items-start gap-6">
              @if (qrUrl) {
                <div className="shrink-0 rounded-md bg-white p-3">
                  <img
                    src={qrUrl}
                    alt="Authenticator setup QR code"
                    width={176}
                    height={176}
                  />
                </div>;
              }
              <div className="flex min-w-52 flex-col gap-2">
                <p className="text-[13px] font-semibold text-foreground">
                  Can’t scan? Enter this setup key
                </p>
                <p
                  className="break-all font-mono text-[13px] leading-[1.6] text-foreground"
                  data-testid="totp-manual-key"
                >
                  {groupSecret(enroll.secret) as string}
                </p>
              </div>
            </div>
            <form
              className="flex flex-col gap-3"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void confirm();
              }}
            >
              <div className="flex max-w-xs flex-col gap-2">
                <Label htmlFor="totp-enroll-code">Verification code</Label>
                <Input
                  id="totp-enroll-code"
                  name="totp-code"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  placeholder="6-digit code"
                  value={code}
                  onInput={(ev) => setCode((ev.currentTarget as HTMLInputElement).value)}
                  required
                />
              </div>
              @if (error) {
                <AuthErrorBanner message={error} />;
              }
              <div className="flex flex-wrap gap-3">
                <Button type="submit" disabled={pending || code.trim() === ""}>
                  {(pending ? "Verifying…" : "Verify and enable") as string}
                </Button>
                @if (props.onCancel) {
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={pending}
                    onClick={() => props.onCancel?.()}
                  >
                    Cancel
                  </Button>;
                }
              </div>
            </form>
          </>;
        }
      }
    }
  </div>;
}
