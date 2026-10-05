/**
 * Account /settings/cli — ox install, login, and self-update commands.
 */
import { createElement } from "octane";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

let loaderData: { kind: "unauthenticated" } | { kind: "ready"; origin: string } | undefined;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
    Link: (props: {
      to?: string;
      children?: unknown;
      className?: string;
      "aria-current"?: string;
    }) =>
      createElement(
        "a",
        {
          href: props.to ?? "#",
          className: props.className,
          "aria-current": props["aria-current"],
        },
        props.children as never,
      ),
  };
});

import { CliSettingsPage, Route } from "./cli";

afterEach(cleanup);

describe("/settings/cli", () => {
  it("exports a file route for /settings/cli", () => {
    expect(Route.options).toBeTruthy();
    expect(Route.options.loader).toBeTypeOf("function");
  });

  it("happy: renders origin-baked install and login commands", async () => {
    loaderData = { kind: "ready", origin: "https://forge.example" };
    renderWithQueryClient(CliSettingsPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Command line" })).toBeInTheDocument();
    });
    expect(screen.getByTestId("settings-cli-page")).toBeInTheDocument();

    const install = screen.getByRole("textbox", {
      name: "Copy install command",
    });
    expect(install).toHaveValue("curl -fsSL https://forge.example/cli/install.sh | sh");
    const login = screen.getByRole("textbox", { name: "Copy login command" });
    expect(login).toHaveValue("ox auth login --instance https://forge.example --token <pat>");
    expect(screen.getByRole("button", { name: "Copy install command" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy login command" })).toBeInTheDocument();

    const nav = screen.getByRole("navigation", { name: "Account settings" });
    expect(nav.querySelector('a[href="/settings/cli"]')).toBeTruthy();
  });

  it("unhappy: unauthenticated sessions redirect to login", async () => {
    const assign = vi.fn();
    const originalLocation = window.location;
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        href: "http://localhost/settings/cli",
        search: "",
        pathname: "/settings/cli",
        assign,
        replace: vi.fn(),
        reload: vi.fn(),
      },
    });
    try {
      loaderData = { kind: "unauthenticated" };
      renderWithQueryClient(CliSettingsPage);

      await waitFor(() => {
        expect(assign).toHaveBeenCalledWith("/login?returnTo=/settings/cli");
      });
    } finally {
      Object.defineProperty(window, "location", {
        configurable: true,
        value: originalLocation,
      });
    }
  });
});
