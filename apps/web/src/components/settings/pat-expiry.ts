import type { PatPolicyPublic } from "@oxidean/api-client";

/** Shared PAT mint expiry presets (classic + fine-grained), driven by `pat.policy`. */

/** Numeric day count, `custom`, or `none` (the `"never"` wire sentinel). */
export type PatExpiryPreset = string;

export type PatExpiryOption = { value: PatExpiryPreset; label: string };

/** Presets offered when no policy is available yet (pre-load fallback). */
const FALLBACK_DAYS = [7, 30, 60, 90];
const FALLBACK_DEFAULT = "30";

/**
 * Expiration options for the instance's effective PAT policy (`pat.policy`).
 * `null` policy keeps the permissive pre-policy shape — the server is still
 * authoritative and rejects what the policy forbids (`pat.expiry_forbidden`).
 */
export function expiryOptionsForPolicy(
  policy: PatPolicyPublic | null | undefined,
): PatExpiryOption[] {
  if (!policy) {
    return [
      ...FALLBACK_DAYS.map((d) => ({ value: String(d), label: `${d} days` })),
      { value: "custom", label: "Custom date" },
      { value: "none", label: "No expiration" },
    ];
  }
  const days = new Set<number>([...FALLBACK_DAYS, policy.default_ttl_days]);
  const opts: PatExpiryOption[] = [...days]
    .sort((a, b) => a - b)
    .filter((d) => d > 0 && d <= policy.max_ttl_days)
    .map((d) => ({
      value: String(d),
      label: d === policy.default_ttl_days ? `${d} days (default)` : `${d} days`,
    }));
  opts.push({ value: "custom", label: "Custom date" });
  if (policy.allow_never_expire) {
    opts.push({ value: "none", label: "No expiration" });
  }
  return opts;
}

/** The preset a fresh form should select — the policy default TTL. */
export function defaultPresetForPolicy(
  policy: PatPolicyPublic | null | undefined,
): PatExpiryPreset {
  return policy ? String(policy.default_ttl_days) : FALLBACK_DEFAULT;
}

/** `max` attribute for the custom date input — the policy TTL cap. */
export function maxCustomDate(policy: PatPolicyPublic | null | undefined): string | undefined {
  return policy ? datePlusDays(policy.max_ttl_days) : undefined;
}

export function tomorrowDateValue(): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + 1);
  return d.toISOString().slice(0, 10);
}

export function datePlusDays(days: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

export function expiryToIso(dateValue: string): string {
  return `${dateValue}T23:59:59Z`;
}

/**
 * Resolve create RPC `expires_at` from preset + optional custom YYYY-MM-DD.
 * `"none"` sends the `"never"` sentinel — an omitted/`null` `expires_at`
 * means "apply the default TTL" on policy-enforcing servers, not "no expiry".
 */
export function resolveExpiresAt(preset: PatExpiryPreset, customDate: string): string | null {
  if (preset === "none") return "never";
  if (preset === "custom") {
    return customDate ? expiryToIso(customDate) : null;
  }
  const days = Number(preset);
  if (!Number.isFinite(days) || days <= 0) return null;
  return expiryToIso(datePlusDays(days));
}

export function expiryPresetLabel(options: PatExpiryOption[], preset: PatExpiryPreset): string {
  return options.find((p) => p.value === preset)?.label ?? preset;
}
