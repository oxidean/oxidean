/** Readable companion to HttpOnly `oxidean_session` — value is always `1`. */
const SESSION_PRESENCE_COOKIE = "oxidean_signed_in";
/**
 * Readable companion carrying `allow_signup` (`1`/`0`) — the web tier stamps
 * `data-oxidean-signup` from it so the anon header skeleton can pre-select
 * the one- vs two-button shape before `auth.provider_config` resolves.
 */
const ALLOW_SIGNUP_COOKIE = "oxidean_allow_signup";

/**
 * `Secure` only on HTTPS origins — a literal `Secure` attr is ignored over
 * `http://localhost` dev (and would split the jar from the non-Secure copy).
 */
function secureAttr(): string {
  return typeof location !== "undefined" && location.protocol === "https:" ? "; Secure" : "";
}

/** Sync hint for choosing signed-in home skeleton before `auth.me` resolves. */
export function hasSessionPresenceHint(): boolean {
  if (typeof document === "undefined") return false;
  return new RegExp(`(?:^|;\\s*)${SESSION_PRESENCE_COOKIE}=1(?:;|$)`).test(document.cookie);
}

/** Heal / clear the presence cookie after `auth.me` (covers sessions minted before the hint existed). */
export function syncSessionPresenceHint(signedIn: boolean): void {
  if (typeof document === "undefined") return;
  const secure = secureAttr();
  if (signedIn) {
    document.cookie = `${SESSION_PRESENCE_COOKIE}=1; Path=/; SameSite=Lax; Max-Age=${30 * 24 * 3600}${secure}`;
  } else {
    document.cookie = `${SESSION_PRESENCE_COOKIE}=; Path=/; SameSite=Lax; Max-Age=0${secure}`;
  }
}

/** Persist `allow_signup` (`0` rather than cleared, so "closed" differs from "never resolved"). */
export function syncAllowSignupHint(allowed: boolean): void {
  if (typeof document === "undefined") return;
  const next = allowed ? "1" : "0";
  if (new RegExp(`(?:^|;\\s*)${ALLOW_SIGNUP_COOKIE}=${next}(?:;|$)`).test(document.cookie)) return;
  document.cookie = `${ALLOW_SIGNUP_COOKIE}=${next}; Path=/; SameSite=Lax; Max-Age=${30 * 24 * 3600}${secureAttr()}`;
}
