export type ThemePreference = "system" | "light" | "dark";

export const THEME_STORAGE_KEY = "oxidean-theme";
/** Cookie mirrors localStorage so SSR can pick oxidean-light vs oxidean-dark. */
export const THEME_COOKIE_KEY = "oxidean-theme";
/**
 * Resolved light/dark after system preference — set by the FOUC boot script so
 * SSR highlighting matches `html.dark` on the next request (and hydrate).
 */
export const THEME_RESOLVED_COOKIE_KEY = "oxidean-color-scheme";

export function readThemePreference(): ThemePreference {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(THEME_STORAGE_KEY);
  if (v === "light" || v === "dark" || v === "system") return v;
  return "system";
}

export function resolveTheme(pref: ThemePreference): "light" | "dark" {
  if (pref === "light" || pref === "dark") return pref;
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Parse `oxidean-theme` from a Cookie header (SSR). */
export function themePreferenceFromCookieHeader(
  cookieHeader: string | undefined | null,
): ThemePreference | null {
  if (!cookieHeader) return null;
  const match = /(?:^|;\s*)oxidean-theme=(light|dark|system)(?:;|$)/.exec(cookieHeader);
  return match ? (match[1] as ThemePreference) : null;
}

/** Parse resolved `oxidean-color-scheme` cookie (SSR highlight alignment). */
export function resolvedColorSchemeFromCookieHeader(
  cookieHeader: string | undefined | null,
): "light" | "dark" | null {
  if (!cookieHeader) return null;
  const match = /(?:^|;\s*)oxidean-color-scheme=(light|dark)(?:;|$)/.exec(cookieHeader);
  return match ? (match[1] as "light" | "dark") : null;
}

/**
 * Resolve light/dark for SSR highlighting.
 * Order: explicit preference cookie → resolved scheme cookie (from boot) →
 * Sec-CH-Prefers-Color-Scheme → light.
 */
export function resolveThemeForSsr(
  pref: ThemePreference | null,
  prefersColorScheme?: string | null,
  resolvedFromBoot?: "light" | "dark" | null,
): "light" | "dark" {
  if (pref === "light" || pref === "dark") return pref;
  if (resolvedFromBoot === "light" || resolvedFromBoot === "dark") return resolvedFromBoot;
  const ch = prefersColorScheme?.trim().toLowerCase();
  if (ch === "dark") return "dark";
  if (ch === "light") return "light";
  return "light";
}

function persistThemeCookie(pref: ThemePreference) {
  if (typeof document === "undefined") return;
  document.cookie = `${THEME_COOKIE_KEY}=${pref}; path=/; max-age=31536000; SameSite=Lax`;
  const resolved = resolveTheme(pref);
  document.cookie = `${THEME_RESOLVED_COOKIE_KEY}=${resolved}; path=/; max-age=31536000; SameSite=Lax`;
}

export function applyTheme(pref: ThemePreference) {
  const resolved = resolveTheme(pref);
  document.documentElement.classList.toggle("dark", resolved === "dark");
  localStorage.setItem(THEME_STORAGE_KEY, pref);
  persistThemeCookie(pref);
}

export const THEME_OPTIONS: readonly ThemePreference[] = ["system", "light", "dark"];

// Runs in <head> before first paint (D-12). Static literal — never interpolate
// request data or storage values into this string (T-03-04).
// Mirrors preference + resolved scheme into cookies so the next SSR request
// highlights with the same theme hydrate will use (`html.dark`).
export const THEME_BOOT_SCRIPT =
  '(function(){try{var v=localStorage.getItem("oxidean-theme");' +
  'var p=(v==="light"||v==="dark"||v==="system")?v:"system";' +
  'var d=p==="dark"||(p==="system"&&window.matchMedia("(prefers-color-scheme: dark)").matches);' +
  'document.documentElement.classList.toggle("dark",d);' +
  'document.cookie="oxidean-theme="+p+";path=/;max-age=31536000;SameSite=Lax";' +
  'document.cookie="oxidean-color-scheme="+(d?"dark":"light")+";path=/;max-age=31536000;SameSite=Lax";' +
  "}catch(e){}})();";

/**
 * Warm webfonts into `document.fonts` so View Transitions / soft navigations
 * do not paint a long stretch of metric fallbacks after CSS re-applies.
 * Static literal — no interpolation (same threat model as theme boot).
 */
export const FONT_WARM_SCRIPT =
  "(function(){try{if(!document.fonts||!document.fonts.load)return;" +
  "document.fonts.load('600 24px \"Sora Variable\"');" +
  "document.fonts.load('400 16px \"Source Sans 3 Variable\"');" +
  "}catch(e){}})();";
