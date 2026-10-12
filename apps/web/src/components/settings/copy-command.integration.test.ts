/**
 * CopyCommand — shared read-only command field + copy button used by the
 * /settings/cli and /settings/ai pages. Clicking must write to the clipboard
 * and flip the label to "Copied"; a rejected clipboard write leaves "Copy".
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CopyCommand } from "@/components/settings/copy-command";
import { renderWithQueryClient } from "@/test/render-with-query";

function stubClipboard(writeText: ReturnType<typeof vi.fn>) {
  Object.defineProperty(window.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

afterEach(cleanup);

describe("CopyCommand", () => {
  it("happy: click writes the command to the clipboard and shows Copied", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    renderWithQueryClient(CopyCommand, {
      props: { command: "ox skill install --upgrade", ariaLabel: "Copy upgrade command" },
    });

    const input = screen.getByRole("textbox", { name: "Copy upgrade command" });
    expect(input).toHaveValue("ox skill install --upgrade");

    screen.getByRole("button", { name: "Copy upgrade command" }).click();

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("ox skill install --upgrade");
      expect(screen.getByRole("button", { name: "Copy upgrade command" })).toHaveTextContent(
        "Copied",
      );
    });
  });

  it("unhappy: a rejected clipboard write leaves the Copy label", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    stubClipboard(writeText);
    renderWithQueryClient(CopyCommand, {
      props: { command: "ox skill install", ariaLabel: "Copy install command" },
    });

    const button = screen.getByRole("button", { name: "Copy install command" });
    button.click();

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("ox skill install");
    });
    expect(button).toHaveTextContent("Copy");
  });
});
