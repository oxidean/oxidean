/** Shared PAT mint expiry presets (classic + fine-grained). */

export type PatExpiryPreset = "7" | "30" | "60" | "90" | "custom" | "none";

export const PAT_EXPIRY_PRESETS: { value: PatExpiryPreset; label: string }[] = [
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "60", label: "60 days" },
  { value: "90", label: "90 days" },
  { value: "custom", label: "Custom date" },
  { value: "none", label: "No expiration" },
];

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

/** Resolve create RPC `expires_at` from preset + optional custom YYYY-MM-DD. */
export function resolveExpiresAt(preset: PatExpiryPreset, customDate: string): string | null {
  if (preset === "none") return null;
  if (preset === "custom") {
    return customDate ? expiryToIso(customDate) : null;
  }
  const days = Number(preset);
  if (!Number.isFinite(days) || days <= 0) return null;
  return expiryToIso(datePlusDays(days));
}

export function expiryPresetLabel(preset: PatExpiryPreset): string {
  return PAT_EXPIRY_PRESETS.find((p) => p.value === preset)?.label ?? preset;
}
