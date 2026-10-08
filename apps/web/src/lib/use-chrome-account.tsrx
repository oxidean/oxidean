import { useQuery } from "@octanejs/tanstack-query";
import type { UserPublic } from "@oxidean/api-client";
import {
  authBootstrapQueryOptions,
  authProviderConfigQueryOptions,
  authSessionQueryOptions,
} from "@/lib/session-queries";

/** Fail-closed: only `true` shows Sign up (D-06 / UI Considerations). */
export function resolveAllowSignup(
  cfg:
    | {
        allow_signup?: boolean;
      }
    | null
    | undefined,
): boolean {
  return cfg?.allow_signup === true;
}

/**
 * Shared chrome session slice — one `auth.me` / bootstrap / providerConfig
 * fetch for header + mobile nav (+ verify banner uses session alone).
 *
 * Session/config queryFns soft-handle `auth.setup_required` so TanStack Query
 * never enters a throw→remount refetch loop on the marketing home.
 */
export function useChromeAccountState(): {
  pending: boolean;
  user: UserPublic | null | undefined;
  needsSetup: boolean | null;
  allowSignup: boolean;
} {
  const me = useQuery(authSessionQueryOptions());
  const boot = useQuery(authBootstrapQueryOptions());
  const cfg = useQuery(authProviderConfigQueryOptions());

  const pending = me.isPending || boot.isPending || cfg.isPending;
  const user: UserPublic | null | undefined = me.isPending
    ? undefined
    : me.isError
      ? null
      : (me.data ?? null);
  const needsSetup: boolean | null = boot.isPending
    ? null
    : boot.isSuccess && boot.data.needs_setup === true;
  const allowSignup = resolveAllowSignup(cfg.data);

  return { pending, user, needsSetup, allowSignup };
}
