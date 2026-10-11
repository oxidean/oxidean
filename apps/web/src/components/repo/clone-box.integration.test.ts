import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CloneBox } from "./clone-box";

afterEach(cleanup);

beforeEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    value: {
      origin: "http://127.0.0.1:3000",
      href: "http://127.0.0.1:3000/",
      assign: vi.fn(),
    },
  });
});

async function openCloneMenu() {
  fireEvent.click(screen.getByRole("button", { name: "Clone or download" }));
  await waitFor(() => {
    expect(screen.getByRole("tab", { name: "HTTPS" })).toBeInTheDocument();
    expect(screen.getByLabelText("HTTPS clone URL")).toBeInTheDocument();
  });
}

async function selectSshTab() {
  fireEvent.click(screen.getByRole("tab", { name: "SSH" }));
  await waitFor(() => {
    expect(screen.getByLabelText(/SSH clone URL/i)).toBeInTheDocument();
  });
}

describe("CloneBox (E12 / D-22 / D-29)", () => {
  it("shows HTTPS clone URL, SSH placeholder, and enabled archive items when not empty", async () => {
    render(CloneBox, {
      props: {
        owner: "ada",
        repo: "hello",
        refName: "main",
        empty: false,
        publicOrigin: "http://127.0.0.1:3000",
      },
    });

    await openCloneMenu();

    const httpsUrl = "http://127.0.0.1:3000/ada/hello.git";
    const urlField = screen.getByRole("textbox", { name: "HTTPS clone URL" });
    expect(urlField).toHaveValue(httpsUrl);
    expect(urlField).toHaveAttribute("readonly");
    expect(urlField.className).not.toMatch(/truncate/);

    const copyBtn = screen.getByRole("button", { name: "Copy HTTPS URL" });
    expect(copyBtn).toBeInTheDocument();
    expect(copyBtn.querySelector("svg")).not.toBeNull();

    // SSH section lives behind the SSH tab.
    await selectSshTab();
    expect(screen.getByLabelText(/SSH clone URL/i)).toBeInTheDocument();

    const zip = screen.getByRole("menuitem", { name: "Download ZIP" });
    const tar = screen.getByRole("menuitem", { name: "Download tar.gz" });
    expect(zip).not.toHaveAttribute("aria-disabled", "true");
    expect(tar).not.toHaveAttribute("aria-disabled", "true");
    expect(zip).not.toBeDisabled();
    expect(tar).not.toBeDisabled();
  });

  it("disables archive downloads when the repo is empty", async () => {
    render(CloneBox, {
      props: {
        owner: "ada",
        repo: "empty",
        refName: "main",
        empty: true,
        publicOrigin: "http://127.0.0.1:3000",
      },
    });

    await openCloneMenu();

    expect(screen.getByRole("textbox", { name: "HTTPS clone URL" })).toHaveValue(
      "http://127.0.0.1:3000/ada/empty.git",
    );
    expect(screen.getByRole("tab", { name: "SSH" })).toBeInTheDocument();

    const zip = screen.getByRole("menuitem", { name: "Download ZIP" });
    const tar = screen.getByRole("menuitem", { name: "Download tar.gz" });
    expect(zip).toHaveAttribute("aria-disabled", "true");
    expect(tar).toHaveAttribute("aria-disabled", "true");
  });

  it("archive downloads are real anchors for the current ref", async () => {
    render(CloneBox, {
      props: {
        owner: "ada",
        repo: "hello",
        refName: "feature/x",
        empty: false,
        publicOrigin: "http://127.0.0.1:3000",
      },
    });

    await openCloneMenu();

    // LinkItem anchors carry the archive URL on href — no onClick navigation.
    const zip = screen.getByRole("menuitem", { name: "Download ZIP" });
    const tar = screen.getByRole("menuitem", { name: "Download tar.gz" });
    expect(zip.tagName).toBe("A");
    expect(zip).toHaveAttribute("href", "/api/repos/ada/hello/archive/feature%2Fx.zip");
    expect(zip).toHaveAttribute("data-astro-reload");
    expect(tar).toHaveAttribute("href", "/api/repos/ada/hello/archive/feature%2Fx.tar.gz");
    expect(tar).toHaveAttribute("data-astro-reload");
  });
});
