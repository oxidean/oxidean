import { queryOptions, type QueryClient } from "@octanejs/tanstack-query";
import type {
  OxpilotSession,
  OxpilotSessionCreateRequest,
  OxpilotStatusResponse,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

const oxpilotStatusQueryKey = ["oxpilot", "status"] as const;
const oxpilotSessionListQueryKey = ["oxpilot", "sessionList"] as const;

/**
 * `oxpilot.status` — the gate-exempt enablement probe the flag-off state
 * renders. Unauthenticated callers get `null` so the page can redirect
 * without throwing through the query layer.
 */
export function oxpilotStatusQueryOptions() {
  return queryOptions({
    queryKey: oxpilotStatusQueryKey,
    queryFn: async (): Promise<OxpilotStatusResponse | null> => {
      const res = await apiClient.oxpilot.status();
      if (!res.ok) {
        if (res.error.code === "auth.unauthenticated" || res.error.code === "auth.setup_required") {
          return null;
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    retry: false,
    staleTime: 15_000,
  });
}

/** `/oxpilot` session list — gated on the flag being on. */
export function oxpilotSessionListQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: oxpilotSessionListQueryKey,
    queryFn: async (): Promise<OxpilotSession[]> => {
      const res = await apiClient.oxpilot.sessionList();
      if (!res.ok) {
        if (res.error.code === "oxpilot.disabled") return [];
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.sessions;
    },
    enabled,
    retry: false,
    staleTime: 10_000,
  });
}

export async function oxpilotSessionCreate(
  input: OxpilotSessionCreateRequest,
): Promise<OxpilotSession> {
  const res = await apiClient.oxpilot.sessionCreate(input);
  if (!res.ok) {
    throw new Error(`${res.error.code}: ${res.error.message}`);
  }
  return res.data;
}

export function invalidateOxpilotQueries(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: ["oxpilot"] });
}
