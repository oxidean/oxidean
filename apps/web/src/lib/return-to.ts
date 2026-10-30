/**
 * D-15 / T-04-23: only same-origin relative paths starting with `/` and not `//`.
 * Empty / invalid → `/` (signed-in home). Legacy `/dashboard` → `/`.
 * Mirrors server-side `sanitize_return_to` (crates/oxidean-api-core/src/auth/external.rs).
 */
export function safeReturnTo(raw: string | null | undefined): string {
  if (raw == null || raw === "") return "/";
  const value = raw.trim();
  if (!value.startsWith("/")) return "/";
  if (value.startsWith("//")) return "/";
  // Browsers normalize `\` to `/` on document navigations — `/\host` is a
  // protocol-relative escape. Control chars can't appear in a navigable path.
  if (value.includes("\\")) return "/";
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code <= 0x1f || (code >= 0x7f && code <= 0x9f)) return "/";
  }
  const lower = value.toLowerCase();
  if (
    lower.startsWith("http:") ||
    lower.startsWith("https:") ||
    lower.startsWith("javascript:") ||
    lower.startsWith("data:")
  ) {
    return "/";
  }
  if (value === "/dashboard" || value.startsWith("/dashboard?")) {
    return "/";
  }
  return value;
}

export function readReturnToFromLocation(): string {
  if (typeof window === "undefined") return "/";
  const params = new URLSearchParams(window.location.search);
  return safeReturnTo(params.get("returnTo"));
}
