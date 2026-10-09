import { QueryClient } from "@octanejs/tanstack-query";

/** Browser QueryClient — shared cache for session/config RPCs. */
function createAppQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: false,
        refetchOnWindowFocus: false,
      },
    },
  });
}
/** Browser QueryClient — shared cache for session/config RPCs. */

let browserClient: QueryClient | undefined;

/** Singleton for the SPA shell (safe to call from client components). */
export function getQueryClient() {
  if (typeof window === "undefined") {
    return createAppQueryClient();
  }
  browserClient ??= createAppQueryClient();
  return browserClient;
}
