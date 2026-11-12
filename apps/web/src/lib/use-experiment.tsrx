import { useQuery } from "@octanejs/tanstack-query";
import { authSessionQueryOptions } from "@/lib/session-queries";

/**
 * Whether the signed-in user has experiment `key` enabled (WI-0001).
 *
 * Reads the shared `auth.me` cache — `UserPublic.experiments` carries the
 * enabled keys, so flag checks add no request. Fails closed (`false`) for
 * anonymous sessions, pending loads, errors, and unknown keys — a flag is
 * only ever "on" when the server explicitly enrolled this user.
 *
 * Lives in `.tsrx`: Octane hook call sites get per-site slots, so hook-calling
 * helpers can't live in plain `.ts` (see the octane skill).
 */
export function useExperiment(key: string): boolean {
  const me = useQuery(authSessionQueryOptions()).data;
  return me?.experiments?.includes(key) ?? false;
}
