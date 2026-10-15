import { useState } from "octane";
import { AppPage } from "@/components/app-page";
import { TwoFactorChallenge } from "@/components/two-factor-challenge";
import { safeReturnTo } from "@/lib/return-to";

/** SSO callbacks land here with `?return=`; the login page uses `?returnTo=`. */
function readChallengeReturnTo(): string {
  if (typeof window === "undefined") return "/";
  const params = new URLSearchParams(window.location.search);
  return safeReturnTo(params.get("return") ?? params.get("returnTo"));
}

/**
 * Standalone challenge page (#130): SSO-callback and signup/reset challenges
 * redirect here. The local identifier-first sign-in runs its challenge steps
 * inline on `/login` instead.
 *
 * Exported for login.two-factor.integration.test.ts (#130).
 */
export function TwoFactorLoginPage() @{
  const returnTo = readChallengeReturnTo();
  // Explicit challenge id — carried via `?challenge=` for robustness; the
  // HttpOnly `oxidean_2fa` cookie is the primary transport.
  const [challengeId] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    const raw = new URLSearchParams(window.location.search).get("challenge");
    return raw && raw.trim() ? raw.trim() : null;
  });

  <AppPage>
    <TwoFactorChallenge
      challengeId={challengeId}
      returnTo={returnTo}
      rememberMe={false}
      onDone={(user) => {
        window.location.assign(
          user?.must_change_credentials === true
            ? `/setup/credentials?returnTo=${encodeURIComponent(returnTo)}`
            : returnTo,
        );
      }}
      onRestart={() => {
        window.location.assign(`/login?returnTo=${encodeURIComponent(returnTo)}`);
      }}
    />
  </AppPage>;
}
