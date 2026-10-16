import { describe, expect, it, vi } from "vitest";
import { QueryClient } from "@octanejs/tanstack-query";

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      bootstrapStatus: vi.fn(),
    },
  },
}));

import { apiClient } from "@/lib/api-client";
import {
  authBootstrapQueryKey,
  authBootstrapQueryOptions,
  markSetupComplete,
} from "./session-queries";

describe("data-oxidean-setup stamp", () => {
  it("resolves the bootstrap query from a stamped 1 without the RPC", async () => {
    document.documentElement.setAttribute("data-oxidean-setup", "1");
    try {
      const data = await authBootstrapQueryOptions().queryFn();
      expect(data).toEqual({ needs_setup: true });
      expect(apiClient.auth.bootstrapStatus).not.toHaveBeenCalled();
    } finally {
      document.documentElement.removeAttribute("data-oxidean-setup");
    }
  });

  it("treats a stamped 0 as resolved-false", async () => {
    document.documentElement.setAttribute("data-oxidean-setup", "0");
    try {
      const data = await authBootstrapQueryOptions().queryFn();
      expect(data).toEqual({ needs_setup: false });
      expect(apiClient.auth.bootstrapStatus).not.toHaveBeenCalled();
    } finally {
      document.documentElement.removeAttribute("data-oxidean-setup");
    }
  });

  it("mounts already-resolved when stamped — no fetch, no pending wave", async () => {
    document.documentElement.setAttribute("data-oxidean-setup", "1");
    try {
      const qc = new QueryClient();
      const data = await qc.fetchQuery(authBootstrapQueryOptions());
      // Resolved without bootstrapStatus being called → the stamp seeded
      // initialData, so hydration mounts this query already-resolved.
      expect(data).toEqual({ needs_setup: true });
      expect(apiClient.auth.bootstrapStatus).not.toHaveBeenCalled();
      expect(qc.getQueryData(authBootstrapQueryKey)).toEqual({ needs_setup: true });
    } finally {
      document.documentElement.removeAttribute("data-oxidean-setup");
    }
  });

  it("markSetupComplete clears the stamp and settles the shared cache", () => {
    document.documentElement.setAttribute("data-oxidean-setup", "1");
    const qc = new QueryClient();
    markSetupComplete(qc);
    expect(document.documentElement.hasAttribute("data-oxidean-setup")).toBe(false);
    expect(qc.getQueryData(authBootstrapQueryKey)).toEqual({ needs_setup: false });
  });
});
