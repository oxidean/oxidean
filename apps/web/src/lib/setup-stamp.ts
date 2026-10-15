/**
 * `data-oxidean-setup` verdict the web tier stamps on `<html>` per response.
 * `"1"` and `"0"` are both authoritative; an absent attribute means the verdict
 * is unknown (older web tier or a failed setup-check probe) and callers must
 * fall back to `auth.bootstrapStatus`.
 */
export function stampedSetupVerdict(): boolean | null {
  if (typeof document === "undefined") return null;
  const v = document.documentElement.getAttribute("data-oxidean-setup");
  if (v === "1") return true;
  if (v === "0") return false;
  return null;
}

/**
 * Drop the stamp once first-run setup completes so views mounted before the
 * next document swap re-resolve instead of replaying a stale `"1"`.
 */
export function clearSetupStamp(): void {
  if (typeof document === "undefined") return;
  document.documentElement.removeAttribute("data-oxidean-setup");
}
