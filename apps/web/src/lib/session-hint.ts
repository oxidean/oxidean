/** Readable companion to HttpOnly `oxidean_session` — value is always `1`. */
const SESSION_PRESENCE_COOKIE = "oxidean_signed_in";
/** Readable companion to HttpOnly `oxidean_session` — value is always `1`. */

/** Sync hint for choosing signed-in home skeleton before `auth.me` resolves. */
export function hasSessionPresenceHint(): boolean {
  if (typeof document === "undefined") return false;
  return new RegExp(`(?:^|;\\s*)${SESSION_PRESENCE_COOKIE}=1(?:;|$)`).test(document.cookie);
}

/** Heal / clear the presence cookie after `auth.me` (covers sessions minted before the hint existed). */
export function syncSessionPresenceHint(signedIn: boolean): void {
  if (typeof document === "undefined") return;
  if (signedIn) {
    document.cookie = `${SESSION_PRESENCE_COOKIE}=1; Path=/; SameSite=Lax; Max-Age=${30 * 24 * 3600}`;
  } else {
    document.cookie = `${SESSION_PRESENCE_COOKIE}=; Path=/; SameSite=Lax; Max-Age=0`;
  }
}
