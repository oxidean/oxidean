import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CloneBox } from "./clone-box";

/**
 * GIT-03: CloneBox SSH URL + add-key CTA (D-SSH-02 / D-SSH-06 / UI-SPEC).
 *
 * Wave 0 RED stubs — greened by 09-08 when live SSH panel replaces the
 * "SSH cloning arrives in a later phase." placeholder.
 */

afterEach(cleanup);

beforeEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    value: {
      origin: "http://127.0.0.1:3000",
      href: "http://127.0.0.1:3000/",
      hostname: "127.0.0.1",
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

/** SSH content lives behind the SSH tab — select it like a user. */
async function selectSshTab() {
  fireEvent.click(screen.getByRole("tab", { name: "SSH" }));
  await waitFor(() => {
    expect(screen.getByLabelText(/SSH clone URL/i)).toBeInTheDocument();
  });
}

describe("CloneBox SSH (GIT-03 / D-SSH-02 / D-SSH-06)", () => {
  it("primary clone string is scp-style git@host:owner/repo.git (not ssh://)", async () => {
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
    await selectSshTab();

    const sshInput = screen.getByLabelText(/SSH clone URL/i) as HTMLInputElement;
    expect(sshInput.value).toBe("git@127.0.0.1:ada/hello.git");
    expect(sshInput.value).not.toMatch(/^ssh:\/\//);
    expect(document.body.textContent).not.toMatch(/SSH cloning arrives in a later phase/);
  }, 20_000);

  it("shows Port hint when advertised SSH port is 2222", async () => {
    render(CloneBox, {
      props: {
        owner: "ada",
        repo: "hello",
        refName: "main",
        empty: false,
        publicOrigin: "http://127.0.0.1:3000",
        sshPort: 2222,
      },
    });

    await openCloneMenu();
    await selectSshTab();

    await waitFor(() => {
      expect(screen.getByText(/Port 2222/)).toBeInTheDocument();
    });
    expect(screen.getByText(/Host/)).toBeInTheDocument();
  }, 20_000);

  it("hides Port hint when advertised SSH port is 22", async () => {
    render(CloneBox, {
      props: {
        owner: "ada",
        repo: "hello",
        refName: "main",
        empty: false,
        publicOrigin: "https://app.oxidean.dev",
        sshHost: "app.oxidean.dev",
        sshPort: 22,
      },
    });

    await openCloneMenu();
    await selectSshTab();

    expect(document.body.textContent).not.toMatch(/Port 22/);
    expect(document.body.textContent).not.toMatch(/~\/\.ssh\/config/);
  }, 20_000);

  it("compact Add an SSH key CTA points to /settings/ssh-keys", async () => {
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
    await selectSshTab();

    const cta = await waitFor(() => screen.getByRole("link", { name: "Add an SSH key" }));
    expect(cta).toHaveAttribute("href", "/settings/ssh-keys");
  }, 20_000);

  it("documents SSH login user git", async () => {
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
    await selectSshTab();

    await waitFor(() => {
      expect(
        screen.getByText(/login user is `?git`?|user `git`|as user `git`/i),
      ).toBeInTheDocument();
    });
  }, 20_000);
});
