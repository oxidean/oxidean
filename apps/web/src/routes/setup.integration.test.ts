import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      bootstrapStatus: vi.fn(async () => ({
        ok: true,
        data: { needs_setup: true },
      })),
      bootstrapSetup: vi.fn(),
    },
  },
}));

afterEach(cleanup);

beforeEach(() => {
  window.history.pushState({}, "", "/setup");
});

describe("/setup Wave 0 (AUTH-07 UI-SPEC)", () => {
  it("exports SetupPage with Allow open signup Switch + Create system admin CTA", async () => {
    const mod = await import("./setup.index");
    expect(mod, "SetupPage must be exported for integration tests (06-06)").toHaveProperty(
      "SetupPage",
    );

    const { SetupPage } = mod as { SetupPage: unknown };
    render(SetupPage as never);

    await waitFor(() => {
      expect(screen.getByText("Allow open signup")).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "Create system admin" })).toBeInTheDocument();
    expect(screen.getByText(/When off, new accounts can’t self-register/i)).toBeInTheDocument();
  }, 15_000);
});
