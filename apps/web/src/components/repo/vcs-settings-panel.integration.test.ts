/**
 * VcsSettingsPanel — the per-repo "Source control protocols" flag surface
 * (FR1.1/AC1.1.1). Status read rides `vcs.status`; its `vcs.disabled` envelope
 * is the never-enabled/disabled wire shape (AC1.2.3) and renders as an
 * unchecked default — every other failure is an alert.
 *
 * Base UI Checkbox renders a `span[role=checkbox]` plus a visually hidden
 * `<input type="checkbox">`: happy-dom toggles only via the input's click
 * (the span ignores synthetic clicks there), and `checked`/`disabled` land on
 * the input.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VcsSettingsPanel } from "@/components/repo/vcs-settings-panel";
import { renderWithQueryClient } from "@/test/render-with-query";

const statusMock = vi.fn();
const enableMock = vi.fn();
const toastSuccessMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    vcs: {
      status: (...args: unknown[]) => statusMock(...args),
      repoEnable: (...args: unknown[]) => enableMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: (...args: unknown[]) => toastSuccessMock(...args),
  toastError: vi.fn(),
}));

const STATUS_OK = {
  enabled: true,
  heads: [],
  changeset_count: 3,
  object_count: 9,
  storage_bytes: 1024,
};

const PROPS = { owner: "ada", name: "hello", can_admin: true, archived: false };

beforeEach(() => {
  statusMock.mockReset();
  enableMock.mockReset();
  toastSuccessMock.mockReset();
  statusMock.mockResolvedValue({ ok: true, data: STATUS_OK });
  enableMock.mockResolvedValue({ ok: true, data: { enabled: false } });
});

afterEach(cleanup);

function checkboxRoot(): HTMLElement {
  const el = document.querySelector('[data-slot="checkbox"]');
  if (!el) throw new Error("vcs checkbox not mounted");
  return el as HTMLElement;
}

function checkboxInput(): HTMLInputElement {
  const el = document.querySelector('input[type="checkbox"]');
  if (!el) throw new Error("vcs hidden input not mounted");
  return el as HTMLInputElement;
}

function saveButton(): HTMLButtonElement {
  return screen.getByText(/^Save protocol setting$|^Saving…$/) as HTMLButtonElement;
}

describe("VcsSettingsPanel", () => {
  it("renders checked when vcs.status reports enabled", async () => {
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    await waitFor(() => {
      expect(checkboxRoot().getAttribute("aria-checked")).toBe("true");
    });
  });

  it("vcs.disabled status renders as unchecked default, not an error", async () => {
    statusMock.mockResolvedValue({
      ok: false,
      error: { code: "vcs.disabled", message: "VCS is not enabled for this repository." },
    });
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    await waitFor(() => {
      expect(checkboxRoot().getAttribute("aria-checked")).toBe("false");
    });
    expect(document.querySelector('[role="alert"]')).toBeNull();
    // The control must stay saveable — enable is how the flag row appears.
    expect(checkboxInput().disabled).toBe(false);
  });

  it("other status failures render the alert state", async () => {
    statusMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "Repository not found." },
    });
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("Repository not found.");
    });
    expect(document.querySelector('[data-slot="checkbox"]')).toBeNull();
  });

  it("save calls vcs.repoEnable with the draft value and toasts", async () => {
    statusMock.mockResolvedValue({ ok: true, data: { ...STATUS_OK, enabled: false } });
    enableMock.mockResolvedValue({ ok: true, data: { enabled: true } });
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    await waitFor(() => {
      expect(checkboxRoot().getAttribute("aria-checked")).toBe("false");
    });
    fireEvent.click(checkboxInput());
    await waitFor(() => {
      expect(checkboxRoot().getAttribute("aria-checked")).toBe("true");
      expect(saveButton().disabled).toBe(false);
    });
    fireEvent.submit(document.querySelector("form")!);
    await waitFor(() => {
      expect(enableMock).toHaveBeenCalledWith({ owner: "ada", name: "hello", enabled: true });
    });
    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith("VCS protocol enabled.");
    });
  });

  it("save failure keeps the draft and shows an inline alert", async () => {
    statusMock.mockResolvedValue({ ok: true, data: { ...STATUS_OK, enabled: false } });
    enableMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.archived", message: "Repository is archived." },
    });
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    await waitFor(() => {
      expect(checkboxRoot().getAttribute("aria-checked")).toBe("false");
    });
    fireEvent.click(checkboxInput());
    await waitFor(() => {
      expect(saveButton().disabled).toBe(false);
    });
    fireEvent.submit(document.querySelector("form")!);
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("Repository is archived.");
    });
    expect(toastSuccessMock).not.toHaveBeenCalled();
  });

  it("archived disables the control and Save with the read-only hint", async () => {
    renderWithQueryClient(VcsSettingsPanel, { props: { ...PROPS, archived: true } });
    await waitFor(() => {
      expect(checkboxInput().disabled).toBe(true);
    });
    expect(document.body.textContent).toContain("read-only");
    expect(saveButton().disabled).toBe(true);
    fireEvent.submit(document.querySelector("form")!);
    expect(enableMock).not.toHaveBeenCalled();
  });

  it("non-admin renders the dead-state prong (page gates admins first)", async () => {
    renderWithQueryClient(VcsSettingsPanel, { props: { ...PROPS, can_admin: false } });
    await waitFor(() => {
      expect(checkboxInput().disabled).toBe(true);
    });
    expect(document.body.textContent).toContain("Only repository admins can change VCS settings.");
  });

  it("pending query shows the loading copy", () => {
    statusMock.mockReturnValue(new Promise(() => {}));
    renderWithQueryClient(VcsSettingsPanel, { props: PROPS });
    expect(document.body.textContent).toContain("Loading VCS status…");
  });
});
