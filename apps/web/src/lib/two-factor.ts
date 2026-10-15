/**
 * Browser-side two-factor helpers (#130): WebAuthn ceremony wrappers +
 * passkey-UX error mapping. No hooks — safe to import from `.ts` or `.tsrx`.
 */
import {
  browserSupportsWebAuthn,
  startAuthentication,
  startRegistration,
  type AuthenticationResponseJSON,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
  type RegistrationResponseJSON,
} from "@simplewebauthn/browser";

export { browserSupportsWebAuthn };

/** Run a WebAuthn assertion ceremony from `auth.two_factor.passkey_begin` options. */
export async function runPasskeyAssertion(publicKey: unknown): Promise<AuthenticationResponseJSON> {
  return startAuthentication({
    optionsJSON: publicKey as PublicKeyCredentialRequestOptionsJSON,
  });
}

/** Run a WebAuthn attestation ceremony from `passkey_register_begin` options. */
export async function runPasskeyRegistration(
  publicKey: unknown,
): Promise<RegistrationResponseJSON> {
  return startRegistration({
    optionsJSON: publicKey as PublicKeyCredentialCreationOptionsJSON,
  });
}

/**
 * User-facing message for a failed WebAuthn ceremony. `NotAllowedError` is the
 * user cancelling (or a timeout) — not a hard failure.
 */
export function passkeyErrorMessage(err: unknown): string {
  if (err && typeof err === "object" && "name" in err) {
    const name = String((err as { name: unknown }).name);
    if (name === "NotAllowedError") {
      return "Passkey prompt was dismissed — no changes were made.";
    }
    if (name === "InvalidStateError") {
      return "That passkey is already registered on this account.";
    }
    if (name === "SecurityError") {
      return "Passkeys are not available on this connection — use HTTPS or localhost.";
    }
  }
  return "Passkey operation failed — try again.";
}
