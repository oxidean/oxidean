/**
 * TemplatePicker: selecting a card must not throw Octane insertBefore races when
 * the parent also updates sibling fields (stack → default .gitignore on /new).
 */
import { cleanup, fireEvent, screen, waitFor, within } from "@octanejs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { TemplatePickerSiblingHarness } from "@/components/repo/template-picker.harness";
import { trackDomErrors } from "@/test/dom-errors";
import { renderWithQueryClient } from "@/test/render-with-query";

afterEach(cleanup);

function openStackPicker() {
  const trigger = screen.getByLabelText("Stack / template");
  const details = trigger.closest("details");
  if (!details) throw new Error("expected stack picker <details>");
  // happy-dom does not always toggle <details> from summary click alone.
  details.open = true;
  fireEvent(details, new Event("toggle", { bubbles: true }));
}

async function chooseFromOpenDialog(name: RegExp | string) {
  const overlay = await waitFor(() => screen.getByTestId("repo-stack-overlay"));
  const dialog = within(overlay).getByRole("dialog", { hidden: true });
  fireEvent.click(within(dialog).getByRole("button", { name, hidden: true }));
  await waitFor(() => {
    const details = screen.getByLabelText("Stack / template").closest("details");
    expect(details?.open).toBe(false);
  });
}

describe("TemplatePicker DOM races (issue #18)", () => {
  // Issue #143: the overlay + its card list must not mount while the picker is
  // closed (they're ~hundreds of nodes inside a closed <details>).
  it("mounts the overlay and cards lazily on first open", async () => {
    renderWithQueryClient(TemplatePickerSiblingHarness);

    expect(screen.queryByTestId("repo-stack-overlay")).not.toBeInTheDocument();
    expect(document.querySelector("[data-template-id]")).toBeNull();

    openStackPicker();

    await waitFor(() => {
      expect(screen.getByTestId("repo-stack-overlay")).toBeInTheDocument();
    });
    expect(document.querySelector("[data-template-id]")).toBeTruthy();
  }, 20_000);

  it("selecting a stack with sibling gitignore update does not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      renderWithQueryClient(TemplatePickerSiblingHarness);

      openStackPicker();
      await chooseFromOpenDialog(/^Next\.js/);

      await waitFor(() => {
        expect(screen.getByTestId("stack-value").textContent).toBe("nextjs");
        expect(screen.getByTestId("gitignore-value").textContent).toBe("Node");
      });

      expect(tracker.domRaceErrors()).toEqual([]);
      expect(screen.getByLabelText("Stack / template")).toHaveTextContent(/Next\.js/);
      expect(screen.getByLabelText(".gitignore")).toHaveTextContent(/Node/);
    } finally {
      tracker.dispose();
    }
  }, 20_000);

  it("switching templates repeatedly stays free of hierarchy errors", async () => {
    const tracker = trackDomErrors();
    try {
      renderWithQueryClient(TemplatePickerSiblingHarness);

      for (const name of [/^Rust/, /^Go/, /^Acme Node/, /^starter/] as const) {
        openStackPicker();
        await chooseFromOpenDialog(name);
      }

      expect(tracker.domRaceErrors()).toEqual([]);
      expect(screen.getByTestId("stack-value").textContent).toBe("repo:r1");
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
