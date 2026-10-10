import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CloneBox } from "./clone-box";

/**
 * GIT-02: CloneBox HTTPS PAT how-to panel (D-13 / UI-SPEC).
 *
 * Greens with PatHowTo embed in CloneBox (08-12).
 */

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

describe("CloneBox PAT how-to (GIT-02 / D-13)", () => {
  it("shows Authenticate with a personal access token how-to with username aliases", async () => {
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

    expect(screen.getByText("Authenticate with a personal access token")).toBeInTheDocument();
    expect(
      screen.getByText(/Username: your Oxidean username, or `git`, `token`, or `oauth2`\./),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Password: a personal access token — not your account password\./),
    ).toBeInTheDocument();
  }, 20000);

  it("Create a personal access token CTA points to /settings/tokens", async () => {
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

    const cta = screen.getByRole("link", {
      name: "Create a personal access token",
    });
    expect(cta).toHaveAttribute("href", "/settings/tokens");
  }, 20000);

  it("long HTTPS clone URL uses overflow-x-auto + break-all so it wraps/scrolls", async () => {
    const longOrigin = "http://127.0.0.1:3000/" + "very-long-owner-name".repeat(4);
    render(CloneBox, {
      props: {
        owner: "ada-with-a-quite-long-username",
        repo: "hello-world-repository-with-a-long-name",
        refName: "main",
        empty: false,
        publicOrigin: longOrigin,
      },
    });

    await openCloneMenu();

    const howTo = document.querySelector('[data-slot="pat-how-to"]');
    expect(howTo).toBeTruthy();
    const code = howTo!.querySelector("code");
    expect(code).toBeTruthy();
    const cls = code!.className;
    expect(cls).toMatch(/overflow-x-auto/);
    expect(cls).toMatch(/break-all/);
    expect(cls).toMatch(/whitespace-pre-wrap/);
    expect(code!.textContent).toMatch(/git clone http/);
  }, 20000);
});
