import { describe, expect, it, vi } from "vitest";
import { QueryClient } from "@octanejs/tanstack-query";

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(),
      bootstrapStatus: vi.fn(),
      providerConfig: vi.fn(),
    },
    admin: {
      auth: {
        getSettings: vi.fn(),
      },
    },
  },
}));

import { apiClient } from "@/lib/api-client";
import {
  adminAuthSettingsQueryKey,
  adminAuthSettingsQueryOptions,
  authBootstrapQueryOptions,
  authMeQueryKey,
  authProviderConfigQueryKey,
  authProviderConfigQueryOptions,
  authSessionQueryOptions,
  clearSessionQueries,
  setAdminAuthSettingsCache,
  setAuthMeCache,
} from "./session-queries";

const sampleUser = {
  id: "1",
  email: "a@b.c",
  username: "a",
  display_name: "A",
  bio: "",
  role: "user" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

const sampleSettings = {
  provider_mode: "local" as const,
  email_provider: "log" as const,
  from_address: null,
  workos_client_id: null,
  oidc_issuer: null,
  oidc_client_id: null,
  smtp_configured: false,
  resend_configured: false,
  workos_api_key_configured: false,
  oidc_client_secret_configured: false,
  allow_signup: true,
};

describe("authSessionQueryOptions", () => {
  it("maps auth.unauthenticated to null", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValueOnce({
      ok: false,
      error: { code: "auth.unauthenticated", message: "n" },
    } as never);

    const data = await authSessionQueryOptions().queryFn();
    expect(data).toBeNull();
  });

  it("returns UserPublic when authenticated", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValueOnce({
      ok: true,
      data: sampleUser,
    } as never);

    const data = await authSessionQueryOptions().queryFn();
    expect(data).toEqual(sampleUser);
  });

  it("maps auth.setup_required to null (pre-setup lock)", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValueOnce({
      ok: false,
      error: { code: "auth.setup_required", message: "setup" },
    } as never);

    const data = await authSessionQueryOptions().queryFn();
    expect(data).toBeNull();
  });

  it("throws on non-auth failures", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValueOnce({
      ok: false,
      error: { code: "rpc.internal", message: "boom" },
    } as never);

    await expect(authSessionQueryOptions().queryFn()).rejects.toThrow(/rpc.internal/);
  });
});

describe("authBootstrapQueryOptions", () => {
  // Node env has no document → stamped verdict is null; the query must fall
  // back to auth.bootstrapStatus (DOM-stamped cases live in the happy-dom
  // integration suite).
  it("falls back to auth.bootstrapStatus when no stamp is present", async () => {
    vi.mocked(apiClient.auth.bootstrapStatus).mockResolvedValueOnce({
      ok: true,
      data: { needs_setup: false },
    } as never);

    const data = await authBootstrapQueryOptions().queryFn();
    expect(data).toEqual({ needs_setup: false });
    expect(apiClient.auth.bootstrapStatus).toHaveBeenCalledTimes(1);
  });
});

describe("authProviderConfigQueryOptions", () => {
  it("maps auth.setup_required to locked allow_signup false", async () => {
    vi.mocked(apiClient.auth.providerConfig).mockResolvedValueOnce({
      ok: false,
      error: { code: "auth.setup_required", message: "setup" },
    } as never);

    const data = await authProviderConfigQueryOptions().queryFn();
    expect(data).toEqual({ mode: "local", allow_signup: false });
  });
});

describe("adminAuthSettingsQueryOptions", () => {
  it("returns settings when ok", async () => {
    vi.mocked(apiClient.admin.auth.getSettings).mockResolvedValueOnce({
      ok: true,
      data: sampleSettings,
    } as never);

    const data = await adminAuthSettingsQueryOptions().queryFn();
    expect(data).toEqual(sampleSettings);
  });

  it("attaches error code for forbidden", async () => {
    vi.mocked(apiClient.admin.auth.getSettings).mockResolvedValueOnce({
      ok: false,
      error: { code: "admin.forbidden", message: "no" },
    } as never);

    await expect(adminAuthSettingsQueryOptions().queryFn()).rejects.toMatchObject({
      code: "admin.forbidden",
    });
  });
});

describe("session cache helpers", () => {
  it("clearSessionQueries nulls auth.me", () => {
    const qc = new QueryClient();
    setAuthMeCache(qc, sampleUser);
    expect(qc.getQueryData(authMeQueryKey)).toBeTruthy();
    clearSessionQueries(qc);
    expect(qc.getQueryData(authMeQueryKey)).toBeNull();
  });

  it("setAdminAuthSettingsCache writes settings and marks provider config stale", () => {
    const qc = new QueryClient();
    qc.setQueryData(authProviderConfigQueryKey, {
      mode: "local",
      allow_signup: false,
    });
    setAdminAuthSettingsCache(qc, sampleSettings);
    expect(qc.getQueryData(adminAuthSettingsQueryKey)).toEqual(sampleSettings);
    const state = qc.getQueryState(authProviderConfigQueryKey);
    expect(state?.isInvalidated).toBe(true);
  });
});
