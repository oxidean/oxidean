import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * GIT-11 /settings/tokens list / create / revoke UI
 * (D-14, D-15, D-17, D-24 / T-08-01 / T-08-03).
 *
 * List + nav greened in 08-09-T1; revoke dialog in 08-09-T2; classic create/reveal in 08-10.
 */

const listMock = vi.fn();
const revokeMock = vi.fn();
const createClassicMock = vi.fn();
const createFineGrainedMock = vi.fn();
const listMineMock = vi.fn();
const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    pat: {
      list: (...args: unknown[]) => listMock(...args),
      revoke: (...args: unknown[]) => revokeMock(...args),
      createClassic: (...args: unknown[]) => createClassicMock(...args),
      createFineGrained: (...args: unknown[]) => createFineGrainedMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => listMineMock(...args),
    },
  },
}));

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      user: {
        id: string;
        email: string;
        username: string;
        display_name: string;
        bio: string;
        avatar_url: null;
        role: string;
        profile_incomplete: boolean;
        email_verified: boolean;
        must_change_credentials: boolean;
      };
      tokens?: unknown[];
    };

let loaderData: LoaderShape;

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

const verifiedUser = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

beforeEach(() => {
  listMock.mockReset();
  revokeMock.mockReset();
  createClassicMock.mockReset();
  createFineGrainedMock.mockReset();
  listMineMock.mockReset();
  meMock.mockReset();
  loaderData = { kind: "ready", user: verifiedUser, tokens: [] };
  listMock.mockResolvedValue({ ok: true, data: [] });
  listMineMock.mockResolvedValue({
    ok: true,
    data: {
      repos: [
        {
          id: "repo-1",
          owner_id: "u1",
          owner_type: "user",
          owner_username: "ada",
          name: "demo",
          description: "",
          visibility: "private",
          default_branch: "main",
          updated_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
  meMock.mockResolvedValue({ ok: true, data: verifiedUser });
});

afterEach(cleanup);

/** Load tokens page; @vite-ignore keeps the suite collectable before ./tokens exists. */
async function loadTokensModule(): Promise<Record<string, unknown>> {
  const rel = "./tokens.index";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `Wave 0: /settings/tokens route missing — implement in 08-09 (GIT-11 / D-14). Expected list title Personal access tokens. ${(err as Error).message}`,
    );
  }
}

describe("/settings/tokens (GIT-11 / D-14 list)", () => {
  it("list title Personal access tokens + empty hero No personal access tokens + Generate new token", async () => {
    const mod = await loadTokensModule();
    const TokensPage = (mod.TokensPage ?? mod.default) as unknown;
    const { container } = renderWithQueryClient(TokensPage);

    await waitFor(() => {
      expect(container.querySelector("h1")?.textContent).toBe("Personal access tokens");
    });

    await waitFor(() => {
      expect(screen.getByText("No personal access tokens")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Create a token to clone, fetch, and push over HTTPS."),
    ).toBeInTheDocument();
    const generate = screen.getAllByRole("button", {
      name: /Generate new token/i,
    })[0]!;
    expect(generate).toBeInTheDocument();
    expect(generate).not.toBeDisabled();
    generate.click();
    await waitFor(() => {
      expect(screen.getByText("Classic token")).toBeInTheDocument();
    });
    expect(screen.getByText("Fine-grained token")).toBeInTheDocument();
    // T-08-01: no plaintext secrets on list
    expect(container.textContent).not.toMatch(
      /oxidean_pat_[a-f0-9]{16,}|oxidean_fg_[a-f0-9]{16,}/i,
    );
  }, 15_000);

  it("unverified: list visible with Generate disabled + Verify your email to create a token.", async () => {
    loaderData = {
      kind: "ready",
      user: { ...verifiedUser, email_verified: false },
      tokens: [],
    };
    meMock.mockResolvedValue({
      ok: true,
      data: { ...verifiedUser, email_verified: false },
    });

    const mod = await loadTokensModule();
    const TokensPage = (mod.TokensPage ?? mod.default) as unknown;
    const { container } = renderWithQueryClient(TokensPage);

    await waitFor(() => {
      expect(container.querySelector("h1")?.textContent).toBe("Personal access tokens");
    });

    const generate = screen.getAllByRole("button", {
      name: /Generate new token/i,
    })[0]!;
    expect(generate).toBeDisabled();
    expect(
      screen.getAllByText("Verify your email to create a token.").length,
    ).toBeGreaterThanOrEqual(1);
    await waitFor(() => {
      expect(screen.getAllByText("No personal access tokens").length).toBeGreaterThanOrEqual(1);
    });
  }, 15_000);

  it("settings secondary nav General | Account | Personal access tokens", async () => {
    const mod = await loadTokensModule();
    const TokensPage = (mod.TokensPage ?? mod.default) as unknown;
    const { container } = renderWithQueryClient(TokensPage);

    await waitFor(() => {
      expect(container.querySelector('nav[aria-label="Account settings"]')).toBeTruthy();
    });

    const nav = container.querySelector('nav[aria-label="Account settings"]')!;
    expect(nav.querySelector("p")?.textContent).toBe("Settings");
    const general = nav.querySelector('a[href="/settings/general"]');
    const account = nav.querySelector('a[href="/settings/profile"]');
    const tokens = nav.querySelector('a[href="/settings/tokens"]');
    expect(general?.textContent).toBe("General");
    expect(account?.textContent).toBe("Account");
    expect(tokens?.textContent).toBe("Personal access tokens");
    expect(tokens?.getAttribute("aria-current")).toBe("page");
    expect(nav.querySelector('a[href="/settings/emails"]')).toBeNull();
  }, 15_000);
});

describe("/settings/tokens (GIT-11 / D-17 revoke)", () => {
  it("revoke AlertDialog copy Revoke token? / Keep token", async () => {
    listMock.mockResolvedValue({
      ok: true,
      data: [
        {
          id: "pat-1",
          kind: "classic",
          name: "laptop",
          token_prefix: "oxidean_pat_abcd",
          scopes: ["repo"],
          expires_at: null,
          last_used_at: null,
          last_used_ip: null,
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    });

    const mod = await loadTokensModule();
    const TokensPage = (mod.TokensPage ?? mod.default) as unknown;
    renderWithQueryClient(TokensPage);

    await waitFor(() => {
      expect(screen.getByText("laptop")).toBeInTheDocument();
    });

    screen.getAllByRole("button", { name: "Revoke token" })[0]!.click();

    await waitFor(() => {
      expect(screen.getByText("Revoke token?")).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "Keep token" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Revoke token$/ }).length).toBeGreaterThanOrEqual(
      1,
    );
    expect(screen.getByText(/Revokes .*laptop/i)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/\bCancel\b/);
  }, 15_000);

  it("revoke confirm calls pat.revoke and keeps dialog open on error", async () => {
    listMock.mockResolvedValue({
      ok: true,
      data: [
        {
          id: "pat-1",
          kind: "classic",
          name: "ci-bot",
          token_prefix: "oxidean_pat_ef01",
          scopes: ["repo"],
          expires_at: null,
          last_used_at: null,
          last_used_ip: null,
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    });
    revokeMock.mockResolvedValue({
      ok: false,
      error: { code: "internal", message: "fail" },
    });

    const mod = await loadTokensModule();
    const TokensPage = (mod.TokensPage ?? mod.default) as unknown;
    renderWithQueryClient(TokensPage);

    await waitFor(() => {
      expect(screen.getByText("ci-bot")).toBeInTheDocument();
    });

    screen.getAllByRole("button", { name: "Revoke token" })[0]!.click();
    await waitFor(() => {
      expect(screen.getByText("Revoke token?")).toBeInTheDocument();
    });

    const confirmBtns = screen.getAllByRole("button", {
      name: /^Revoke token$/,
    });
    // Dialog confirm is the last Revoke token button (row trigger already clicked)
    confirmBtns[confirmBtns.length - 1]!.click();

    await waitFor(() => {
      expect(revokeMock).toHaveBeenCalledWith({ id: "pat-1" });
    });
    await waitFor(() => {
      expect(
        screen.getByText("Couldn't revoke token. Check your connection and try again."),
      ).toBeInTheDocument();
    });
    expect(screen.getByText("Revoke token?")).toBeInTheDocument();
  }, 15_000);
});

/** Load classic create page; @vite-ignore keeps suite collectable before route exists. */
async function loadTokensNewModule(): Promise<Record<string, unknown>> {
  const rel = "./tokens.new.index";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `08-10: /settings/tokens/new route missing — implement classic create + reveal (GIT-11 / D-05 / D-15). ${(err as Error).message}`,
    );
  }
}

function tokensNewPage(mod: Record<string, unknown>): unknown {
  const page = mod.TokensNewPage ?? mod.ClassicCreatePage ?? mod.default;
  expect(
    page,
    "Wave 0: TokensNewPage (or ClassicCreatePage) must be exported from tokens.new.index",
  ).toBeTruthy();
  return page;
}

describe("/settings/tokens/new (GIT-11 / D-05 classic create)", () => {
  it("title New classic token + Note + Full control checkbox + Expiration preset + Generate token", async () => {
    const mod = await loadTokensNewModule();
    const { container } = renderWithQueryClient(tokensNewPage(mod));

    await waitFor(() => {
      expect(container.querySelector("h1")?.textContent).toBe("New classic token");
    });
    expect(screen.getByLabelText(/^Note$/i)).toBeInTheDocument();
    expect(screen.getByTestId("scope-repo")).toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: /Full control of private repositories/i }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("pat-expiry-preset")).toBeInTheDocument();
    expect(screen.getByText("Expiration")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Generate token" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Personal access tokens/i })).toHaveAttribute(
      "href",
      "/settings/tokens",
    );
  }, 15_000);

  it("empty Note submit shows Note is required.", async () => {
    const mod = await loadTokensNewModule();
    renderWithQueryClient(tokensNewPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Generate token" })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Generate token" }));

    await waitFor(() => {
      expect(screen.getByText("Note is required.")).toBeInTheDocument();
    });
    expect(createClassicMock).not.toHaveBeenCalled();
  }, 15_000);

  it("unverified shows Verify your email AuthShell — not the create form", async () => {
    loaderData = {
      kind: "ready",
      user: { ...verifiedUser, email_verified: false },
    };
    meMock.mockResolvedValue({
      ok: true,
      data: { ...verifiedUser, email_verified: false },
    });

    const mod = await loadTokensNewModule();
    renderWithQueryClient(tokensNewPage(mod));

    await waitFor(() => {
      expect(screen.getByText("Verify your email")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Verify your email before creating a personal access token."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Verify email" })).toHaveAttribute("href", "/verify");
    expect(screen.queryByRole("button", { name: "Generate token" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/^Note$/i)).not.toBeInTheDocument();
  }, 15_000);
});

describe("/settings/tokens/new (GIT-11 / D-15 one-time reveal)", () => {
  it("success shows Make sure to copy… + Copy token + Back to tokens", async () => {
    createClassicMock.mockResolvedValue({
      ok: true,
      data: {
        token: "oxidean_pat_abcdef0123456789deadbeef",
        item: {
          id: "pat-new",
          kind: "classic",
          name: "laptop",
          token_prefix: "oxidean_pat_abcd",
          scopes: ["repo"],
          expires_at: null,
          last_used_at: null,
          last_used_ip: null,
          created_at: "2026-01-01T00:00:00Z",
        },
      },
    });

    const mod = await loadTokensNewModule();
    renderWithQueryClient(tokensNewPage(mod));

    await waitFor(() => {
      expect(screen.getByLabelText(/^Note$/i)).toBeInTheDocument();
    });

    fireEvent.input(screen.getByLabelText(/^Note$/i), {
      target: { value: "laptop" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Generate token" }));

    await waitFor(() => {
      expect(createClassicMock).toHaveBeenCalled();
    });
    expect(createClassicMock).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "laptop",
        scopes: ["repo"],
      }),
    );

    await waitFor(() => {
      expect(
        screen.getByText("Make sure to copy your personal access token now"),
      ).toBeInTheDocument();
    });
    expect(screen.getByText("You won’t be able to see it again.")).toBeInTheDocument();
    expect(screen.getByDisplayValue("oxidean_pat_abcdef0123456789deadbeef")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy token" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to tokens" })).toHaveAttribute(
      "href",
      "/settings/tokens",
    );
    expect(screen.queryByRole("button", { name: "Generate token" })).not.toBeInTheDocument();
  }, 15_000);
});

/** Load fine-grained create page; @vite-ignore keeps suite collectable before route exists. */
async function loadTokensNewFgModule(): Promise<Record<string, unknown>> {
  const rel = "./tokens.new.fine-grained";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `08-11: /settings/tokens/new/fine-grained route missing — implement FG create + reveal (GIT-11 / D-05 / D-06). ${(err as Error).message}`,
    );
  }
}

function tokensNewFgPage(mod: Record<string, unknown>): unknown {
  const page = mod.TokensNewFineGrainedPage ?? mod.FineGrainedCreatePage ?? mod.default;
  expect(
    page,
    "Wave 0: TokensNewFineGrainedPage (or FineGrainedCreatePage) must be exported from tokens.new.fine-grained",
  ).toBeTruthy();
  return page;
}

describe("/settings/tokens/new/fine-grained (GIT-11 / D-05 / D-06 FG create)", () => {
  it("title New fine-grained token + All repositories / Only select repositories + Contents/Packages selects", async () => {
    const mod = await loadTokensNewFgModule();
    const { container } = renderWithQueryClient(tokensNewFgPage(mod));

    await waitFor(() => {
      expect(container.querySelector("h1")?.textContent).toBe("New fine-grained token");
    });
    expect(screen.getByLabelText(/^Note$/i)).toBeInTheDocument();
    expect(screen.getByText("All repositories")).toBeInTheDocument();
    expect(screen.getByText("Only select repositories")).toBeInTheDocument();
    expect(screen.getByText("Repository permissions")).toBeInTheDocument();
    expect(screen.getByTestId("fg-contents-perm")).toBeInTheDocument();
    expect(screen.getByTestId("fg-packages-perm-select")).toBeInTheDocument();
    expect(screen.getByLabelText("Contents permission")).toHaveTextContent("Read-only");
    expect(screen.getByTestId("pat-expiry-preset")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Generate token" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Personal access tokens/i })).toHaveAttribute(
      "href",
      "/settings/tokens",
    );
  }, 15_000);

  it("selected empty submit shows Select at least one repository.", async () => {
    const mod = await loadTokensNewFgModule();
    renderWithQueryClient(tokensNewFgPage(mod));

    await waitFor(() => {
      expect(screen.getByLabelText(/^Note$/i)).toBeInTheDocument();
    });

    fireEvent.input(screen.getByLabelText(/^Note$/i), {
      target: { value: "ci" },
    });
    // Default is Only select repositories with nothing checked
    fireEvent.click(screen.getByRole("button", { name: "Generate token" }));

    await waitFor(() => {
      expect(screen.getByText("Select at least one repository.")).toBeInTheDocument();
    });
    expect(createFineGrainedMock).not.toHaveBeenCalled();
  }, 15_000);

  it("zero owned repos shows You don’t have any repositories yet. + New repository link", async () => {
    listMineMock.mockResolvedValue({ ok: true, data: { repos: [] } });

    const mod = await loadTokensNewFgModule();
    renderWithQueryClient(tokensNewFgPage(mod));

    await waitFor(() => {
      expect(screen.getByText("You don’t have any repositories yet.")).toBeInTheDocument();
    });
    expect(screen.getByRole("link", { name: "New repository" })).toHaveAttribute("href", "/new");
  }, 15_000);

  it("unverified shows Verify your email AuthShell — not the FG create form", async () => {
    loaderData = {
      kind: "ready",
      user: { ...verifiedUser, email_verified: false },
    };
    meMock.mockResolvedValue({
      ok: true,
      data: { ...verifiedUser, email_verified: false },
    });

    const mod = await loadTokensNewFgModule();
    renderWithQueryClient(tokensNewFgPage(mod));

    await waitFor(() => {
      expect(screen.getByText("Verify your email")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Verify your email before creating a personal access token."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Verify email" })).toHaveAttribute("href", "/verify");
    expect(screen.queryByRole("button", { name: "Generate token" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/^Note$/i)).not.toBeInTheDocument();
  }, 15_000);
});

describe("/settings/tokens/new/fine-grained (GIT-11 / D-15 FG reveal)", () => {
  it("success shows Make sure to copy… + Copy token + Back to tokens via createFineGrained", async () => {
    createFineGrainedMock.mockResolvedValue({
      ok: true,
      data: {
        token: "oxidean_fg_abcdef0123456789deadbeef",
        item: {
          id: "pat-fg-1",
          kind: "fine_grained",
          name: "ci",
          token_prefix: "oxidean_fg_abcd",
          contents: "write",
          repo_access: "all",
          expires_at: null,
          last_used_at: null,
          last_used_ip: null,
          created_at: "2026-01-01T00:00:00Z",
        },
      },
    });

    const mod = await loadTokensNewFgModule();
    renderWithQueryClient(tokensNewFgPage(mod));

    await waitFor(() => {
      expect(screen.getByLabelText(/^Note$/i)).toBeInTheDocument();
    });

    fireEvent.input(screen.getByLabelText(/^Note$/i), {
      target: { value: "ci" },
    });
    fireEvent.click(screen.getByTestId("fg-repo-access-all"));
    fireEvent.click(screen.getByRole("button", { name: "Generate token" }));

    await waitFor(() => {
      expect(createFineGrainedMock).toHaveBeenCalled();
    });
    expect(createFineGrainedMock).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "ci",
        repo_access: "all",
        contents: "read",
      }),
    );

    await waitFor(() => {
      expect(
        screen.getByText("Make sure to copy your personal access token now"),
      ).toBeInTheDocument();
    });
    expect(screen.getByText("You won’t be able to see it again.")).toBeInTheDocument();
    expect(screen.getByDisplayValue("oxidean_fg_abcdef0123456789deadbeef")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy token" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to tokens" })).toHaveAttribute(
      "href",
      "/settings/tokens",
    );
    expect(screen.queryByRole("button", { name: "Generate token" })).not.toBeInTheDocument();
  }, 15_000);
});
