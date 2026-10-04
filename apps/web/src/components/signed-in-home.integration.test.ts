import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { createElement } from "octane";
import { cleanup, fireEvent, render, screen } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@octanejs/tanstack-router", () => ({
  // No RouterProvider in this harness — AppLink must see "no router" and
  // render its plain <a> fallback.
  useRouter: () => undefined,
  Link: (props: { to?: string; children?: unknown; className?: string }) =>
    createElement(
      "a",
      { href: props.to ?? "#", className: props.className },
      props.children as never,
    ),
}));

import { SignedInHome } from "./signed-in-home";

afterEach(cleanup);

function user(overrides: Partial<UserPublic> = {}): UserPublic {
  return {
    id: "u1",
    email: "ada@example.com",
    username: "ada",
    display_name: "Ada",
    bio: "",
    avatar_url: null,
    role: "user",
    profile_incomplete: false,
    email_verified: false,
    must_change_credentials: false,
    default_branch: "main",
    ...overrides,
  };
}

function repo(overrides: Partial<RepoPublic> = {}): RepoPublic {
  return {
    id: "r1",
    owner_id: "u1",
    owner_type: "user",
    owner_username: "ada",
    name: "hello",
    description: "A demo repo",
    visibility: "public",
    default_branch: "main",
    updated_at: "2026-09-12T12:00:00Z",
    ...overrides,
  };
}

describe("SignedInHome New repository CTA", () => {
  it("unverified: disabled New CTA + verify-email hint (unhappy)", () => {
    render(SignedInHome, {
      props: { user: user({ email_verified: false }), repos: [] },
    });

    expect(screen.getByRole("heading", { name: "Home" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Top repositories" })).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Create your first repository" }),
    ).toBeInTheDocument();

    const ctas = screen.getAllByRole("button", { name: /New( repository)?/ });
    expect(ctas.length).toBeGreaterThanOrEqual(1);
    for (const cta of ctas) {
      expect(cta).toBeDisabled();
    }
    expect(screen.getByText("Verify your email to create a repository.")).toBeInTheDocument();
  });

  it("verified: enabled New navigates to /new (happy)", () => {
    render(SignedInHome, {
      props: { user: user({ email_verified: true }), repos: [] },
    });

    const news = screen.getAllByRole("link", { name: /^New( repository)?$/ });
    expect(news.length).toBeGreaterThanOrEqual(1);
    for (const cta of news) {
      expect(cta).toHaveAttribute("href", "/new");
    }
    expect(screen.queryByText("Verify your email to create a repository.")).not.toBeInTheDocument();
  });
});

describe("SignedInHome dashboard IA (classic)", () => {
  it("empty list shows hero + feed placeholder + shortcuts", () => {
    render(SignedInHome, {
      props: { user: user({ email_verified: true }), repos: [] },
    });

    expect(screen.getByTestId("signed-in-home")).toBeInTheDocument();
    expect(screen.getByTestId("home-top-repos")).toBeInTheDocument();
    expect(screen.getByTestId("home-feed")).toBeInTheDocument();
    expect(screen.queryByTestId("home-aside")).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Create your first repository" }),
    ).toBeInTheDocument();
    expect(screen.getByText("No repositories yet.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Shortcuts" })).not.toBeInTheDocument();
  });

  it("populated list shows top repos and recent in feed", () => {
    render(SignedInHome, {
      props: {
        user: user({ email_verified: true }),
        repos: [
          repo({ name: "hello", visibility: "public" }),
          repo({
            id: "r2",
            name: "secrets",
            visibility: "private",
            updated_at: "2026-09-11T12:00:00Z",
          }),
        ],
      },
    });

    expect(screen.getAllByText("ada/hello").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("ada/secrets").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("heading", { name: "Your repositories" })).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Create your first repository" }),
    ).not.toBeInTheDocument();
  });

  it("filters top repositories (edge)", () => {
    render(SignedInHome, {
      props: {
        user: user({ email_verified: true }),
        repos: [repo({ name: "hello" }), repo({ id: "r2", name: "secrets" })],
      },
    });

    const search = screen.getByRole("searchbox", { name: "Find a repository" });
    fireEvent.input(search, { target: { value: "sec" } });
    const top = screen.getByTestId("home-top-repos");
    expect(top.textContent).toContain("ada/secrets");
    expect(top.textContent).not.toContain("ada/hello");
  });

  it("shows Show more when more than 7 repos (edge)", () => {
    const repos = Array.from({ length: 9 }, (_, i) => repo({ id: `r${i}`, name: `repo${i}` }));
    render(SignedInHome, {
      props: { user: user({ email_verified: true }), repos },
    });

    expect(screen.getByRole("button", { name: "Show more" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show more" }));
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
    expect(screen.getByText("ada/repo8")).toBeInTheDocument();
  });

  it("keeps incomplete-profile banner above the dashboard", () => {
    render(SignedInHome, {
      props: {
        user: user({ profile_incomplete: true, email_verified: true }),
        repos: [],
      },
    });

    expect(screen.getByRole("status")).toHaveTextContent("Choose a username to finish setup.");
    expect(screen.getByRole("heading", { name: "Home" })).toBeInTheDocument();
  });

  it("shows reposError without empty hero (unhappy)", () => {
    render(SignedInHome, {
      props: {
        user: user({ email_verified: true }),
        repos: [],
        reposError: "Could not load repositories.",
      },
    });

    expect(screen.getByRole("alert")).toHaveTextContent("Could not load repositories.");
    expect(
      screen.queryByRole("heading", { name: "Create your first repository" }),
    ).not.toBeInTheDocument();
  });
});
