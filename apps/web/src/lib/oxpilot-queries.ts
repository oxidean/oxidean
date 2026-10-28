import { queryOptions, type QueryClient } from "@octanejs/tanstack-query";
import type {
  OxpilotDiffApplyResponse,
  OxpilotDiffRefRequest,
  OxpilotGrantListRequest,
  OxpilotGrantRespondRequest,
  OxpilotGrantRespondResponse,
  OxpilotGrantView,
  OxpilotMessageQueueRequest,
  OxpilotOkResponse,
  OxpilotPairingCreateRequest,
  OxpilotPairingView,
  OxpilotProposedDiff,
  OxpilotQueueDeleteRequest,
  OxpilotQueueDeleteResponse,
  OxpilotQueuedMessage,
  OxpilotQueuedReceipt,
  OxpilotSession,
  OxpilotSessionCreateRequest,
  OxpilotSessionRefRequest,
  OxpilotStatusResponse,
  OxpilotTargetRefRequest,
  OxpilotTargetView,
  OxpilotTurnCancelResponse,
  OxpilotUsageResponse,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

const oxpilotStatusQueryKey = ["oxpilot", "status"] as const;
const oxpilotSessionListQueryKey = ["oxpilot", "sessionList"] as const;
const oxpilotSessionGetQueryKey = (sessionId: string) =>
  ["oxpilot", "sessionGet", sessionId] as const;
const oxpilotTargetListQueryKey = ["oxpilot", "targetList"] as const;
/** `grant_list` filter — the generated `target_id` + the flagged `session_id` amendment. */
type OxpilotGrantListFilter = OxpilotGrantListRequest & { session_id?: string };
const oxpilotGrantListQueryKey = (filter: OxpilotGrantListFilter) =>
  ["oxpilot", "grantList", filter.target_id ?? null, filter.session_id ?? null] as const;
const oxpilotGrantGetQueryKey = (grantId: string) => ["oxpilot", "grantGet", grantId] as const;
const oxpilotQueueListQueryKey = (sessionId: string) =>
  ["oxpilot", "queueList", sessionId] as const;
const oxpilotDiffListQueryKey = (sessionId: string) => ["oxpilot", "diffList", sessionId] as const;
const oxpilotDiffGetQueryKey = (proposalId: string) => ["oxpilot", "diffGet", proposalId] as const;
const oxpilotUsageGetQueryKey = ["oxpilot", "usageGet"] as const;

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

/** `oxpilot.session_get` — one session's metadata (workspace header, context drawer). */
export function oxpilotSessionGetQuery(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: oxpilotSessionGetQueryKey(sessionId),
    queryFn: async (): Promise<OxpilotSession> => {
      const res = await apiClient.oxpilot.sessionGet({ session_id: sessionId });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    enabled: enabled && Boolean(sessionId),
    retry: false,
    staleTime: 10_000,
  });
}

/** `oxpilot.target_list` — the index "Computer use" section + rail target chips. */
export function oxpilotTargetListQuery(enabled = true) {
  return queryOptions({
    queryKey: oxpilotTargetListQueryKey,
    queryFn: async (): Promise<OxpilotTargetView[]> => {
      const res = await apiClient.oxpilot.targetList();
      if (!res.ok) {
        if (res.error.code === "oxpilot.disabled") return [];
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.targets;
    },
    enabled,
    retry: false,
    staleTime: 10_000,
  });
}

/**
 * `oxpilot.grant_list` — pending connect grants by `target_id` (Manage
 * expander / connect dialog), pending consent grants by `session_id`
 * (mount card recovery — flagged C1 amendment).
 */
export function oxpilotGrantListQuery(filter: OxpilotGrantListFilter = {}, enabled = true) {
  return queryOptions({
    queryKey: oxpilotGrantListQueryKey(filter),
    queryFn: async (): Promise<OxpilotGrantView[]> => {
      const res = await apiClient.oxpilot.grantList(filter);
      if (!res.ok) {
        if (res.error.code === "oxpilot.disabled") return [];
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.grants;
    },
    enabled: enabled && Boolean(filter.target_id ?? filter.session_id),
    retry: false,
    staleTime: 10_000,
  });
}

/** `oxpilot.grant_get` — resolves a `card{consent}` ref for `OxpilotConsentCard`. */
export function oxpilotGrantGetQuery(grantId: string, enabled = true) {
  return queryOptions({
    queryKey: oxpilotGrantGetQueryKey(grantId),
    queryFn: async (): Promise<OxpilotGrantView> => {
      const res = await apiClient.oxpilot.grantGet({ grant_id: grantId });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    enabled: enabled && Boolean(grantId),
    retry: false,
    staleTime: 10_000,
  });
}

/** `oxpilot.queue_list` — resolves `card{queued}` rows; non-consuming read. */
export function oxpilotQueueListQuery(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: oxpilotQueueListQueryKey(sessionId),
    queryFn: async (): Promise<OxpilotQueuedMessage[]> => {
      const res = await apiClient.oxpilot.queueList({ session_id: sessionId });
      if (!res.ok) {
        if (res.error.code === "oxpilot.disabled") return [];
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.queued;
    },
    enabled: enabled && Boolean(sessionId),
    retry: false,
    staleTime: 10_000,
  });
}

/** `oxpilot.diff_list` — session proposals (mount card recovery + drawer pane). */
export function oxpilotDiffListQuery(sessionId: string, enabled = true) {
  return queryOptions({
    queryKey: oxpilotDiffListQueryKey(sessionId),
    queryFn: async (): Promise<OxpilotProposedDiff[]> => {
      const res = await apiClient.oxpilot.diffList({ session_id: sessionId });
      if (!res.ok) {
        if (res.error.code === "oxpilot.disabled") return [];
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.diffs;
    },
    enabled: enabled && Boolean(sessionId),
    retry: false,
    staleTime: 10_000,
  });
}

/** `oxpilot.diff_get` — resolves a `card{diff}` ref for `OxpilotDiffCard`. */
export function oxpilotDiffGetQuery(proposalId: string, enabled = true) {
  return queryOptions({
    queryKey: oxpilotDiffGetQueryKey(proposalId),
    queryFn: async (): Promise<OxpilotProposedDiff> => {
      const res = await apiClient.oxpilot.diffGet({ proposal_id: proposalId });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    enabled: enabled && Boolean(proposalId),
    retry: false,
    staleTime: 10_000,
  });
}

/**
 * `oxpilot.usage_get` — the UTC-day rollup against budget caps (generated seam, unread in v1).
 * @public
 */
export function oxpilotUsageGetQuery(enabled = true) {
  return queryOptions({
    queryKey: oxpilotUsageGetQueryKey,
    queryFn: async (): Promise<OxpilotUsageResponse> => {
      const res = await apiClient.oxpilot.usageGet();
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    enabled,
    retry: false,
    staleTime: 15_000,
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

/** `oxpilot.session_delete` — the M5 confirm ride; the row vanishes on invalidate. */
export function oxpilotSessionDeleteMutation() {
  return {
    mutationKey: ["oxpilot", "sessionDelete"] as const,
    mutationFn: async (input: OxpilotSessionRefRequest): Promise<{ deleted: boolean }> => {
      const res = await apiClient.oxpilot.sessionDelete(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.pairing_create` — mints the one-shot pairing code (ConnectDialog step 1). */
export function oxpilotPairingCreateMutation() {
  return {
    mutationKey: ["oxpilot", "pairingCreate"] as const,
    mutationFn: async (input: OxpilotPairingCreateRequest): Promise<OxpilotPairingView> => {
      const res = await apiClient.oxpilot.pairingCreate(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.target_disconnect` — offline transition; pending grants cancel. */
export function oxpilotTargetDisconnectMutation() {
  return {
    mutationKey: ["oxpilot", "targetDisconnect"] as const,
    mutationFn: async (input: OxpilotTargetRefRequest): Promise<OxpilotOkResponse> => {
      const res = await apiClient.oxpilot.targetDisconnect(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.target_revoke` — ends the target AND every pending grant incl. in-flight (BR3.1). */
export function oxpilotTargetRevokeMutation() {
  return {
    mutationKey: ["oxpilot", "targetRevoke"] as const,
    mutationFn: async (input: OxpilotTargetRefRequest): Promise<OxpilotOkResponse> => {
      const res = await apiClient.oxpilot.targetRevoke(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/**
 * `oxpilot.grant_respond` — consent card `[Allow once]`/`[Allow for this
 * session]` (`{allow:true, scope:"once"|"session"}`) and `[Deny]`
 * (`{allow:false}`); pending connect grants respond `{allow:true}`.
 */
export function oxpilotGrantRespondMutation() {
  return {
    mutationKey: ["oxpilot", "grantRespond"] as const,
    mutationFn: async (input: OxpilotGrantRespondRequest): Promise<OxpilotGrantRespondResponse> => {
      const res = await apiClient.oxpilot.grantRespond(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.diff_apply` — the ONLY materialization path; always behind the explicit confirm. */
export function oxpilotDiffApplyMutation() {
  return {
    mutationKey: ["oxpilot", "diffApply"] as const,
    mutationFn: async (input: OxpilotDiffRefRequest): Promise<OxpilotDiffApplyResponse> => {
      const res = await apiClient.oxpilot.diffApply(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/**
 * `oxpilot.message_queue` — live-turn send; a `{null, null}` receipt is a
 * transcript-merge resend (no queued row). `session_active_turn_absent`
 * and `turn_active` surface as thrown typed errors — never auto-enqueue.
 */
export function oxpilotMessageQueueMutation() {
  return {
    mutationKey: ["oxpilot", "messageQueue"] as const,
    mutationFn: async (input: OxpilotMessageQueueRequest): Promise<OxpilotQueuedReceipt> => {
      const res = await apiClient.oxpilot.messageQueue(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.queue_delete` — the inline queued-message cancel (U8's `×` affordance). */
export function oxpilotQueueDeleteMutation() {
  return {
    mutationKey: ["oxpilot", "queueDelete"] as const,
    mutationFn: async (input: OxpilotQueueDeleteRequest): Promise<OxpilotQueueDeleteResponse> => {
      const res = await apiClient.oxpilot.queueDelete(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

/** `oxpilot.turn_cancel` — the optional `[Stop]` affordance; `{aborted:false}` is a quiet no-op. */
export function oxpilotTurnCancelMutation() {
  return {
    mutationKey: ["oxpilot", "turnCancel"] as const,
    mutationFn: async (input: OxpilotSessionRefRequest): Promise<OxpilotTurnCancelResponse> => {
      const res = await apiClient.oxpilot.turnCancel(input);
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
  };
}

export function invalidateOxpilotQueries(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: ["oxpilot"] });
}
