import { createElement } from "octane";
import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * Phase 11 Issues UI — create/list/detail lifecycle greened through 11-09
 * (Linked PRs stubs + manual link). Factory wipe remains later as needed.
 */

const getMock = vi.fn();
const listMock = vi.fn();
const issueGetMock = vi.fn();
const createMock = vi.fn();
const historyMock = vi.fn();
const updateMock = vi.fn();
const closeMock = vi.fn();
const reopenMock = vi.fn();
const deleteMock = vi.fn();
const commentsListMock = vi.fn();
const commentsCreateMock = vi.fn();
const commentsUpdateMock = vi.fn();
const commentsDeleteMock = vi.fn();
const commentsHistoryMock = vi.fn();
const labelListForRepoMock = vi.fn();
const labelsSetMock = vi.fn();
const assigneeCandidatesMock = vi.fn();
const assigneesSetMock = vi.fn();
const reactionsToggleMock = vi.fn();
const linksListMock = vi.fn();
const linksAddMock = vi.fn();
const linksRemoveMock = vi.fn();
const userLookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => getMock(...args),
    },
    issue: {
      list: (...args: unknown[]) => listMock(...args),
      get: (...args: unknown[]) => issueGetMock(...args),
      create: (...args: unknown[]) => createMock(...args),
      history: (...args: unknown[]) => historyMock(...args),
      update: (...args: unknown[]) => updateMock(...args),
      close: (...args: unknown[]) => closeMock(...args),
      reopen: (...args: unknown[]) => reopenMock(...args),
      delete: (...args: unknown[]) => deleteMock(...args),
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: (...args: unknown[]) => commentsCreateMock(...args),
        update: (...args: unknown[]) => commentsUpdateMock(...args),
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
        history: (...args: unknown[]) => commentsHistoryMock(...args),
      },
      labels: {
        set: (...args: unknown[]) => labelsSetMock(...args),
      },
      assignees: {
        set: (...args: unknown[]) => assigneesSetMock(...args),
      },
      assigneeCandidates: (...args: unknown[]) => assigneeCandidatesMock(...args),
      reactions: {
        toggle: (...args: unknown[]) => reactionsToggleMock(...args),
      },
      links: {
        list: (...args: unknown[]) => linksListMock(...args),
        add: (...args: unknown[]) => linksAddMock(...args),
        remove: (...args: unknown[]) => linksRemoveMock(...args),
      },
    },
    label: {
      listForRepo: (...args: unknown[]) => labelListForRepoMock(...args),
    },
    user: {
      lookup: (...args: unknown[]) => userLookupMock(...args),
    },
  },
}));

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: {
      id: "u1",
      email: "ada@example.com",
      username: "ada",
      display_name: "Ada",
      bio: "",
      role: "user",
      profile_incomplete: false,
      email_verified: true,
      must_change_credentials: false,
    },
    needsSetup: false,
    allowSignup: true,
  }),
  resolveAllowSignup: () => true,
}));

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  function MockLink(props: {
    to?: string;
    href?: string;
    children?: unknown;
    className?: string;
    preload?: string;
  }) {
    return createElement(
      "a",
      {
        href: (props.href ?? props.to ?? "#") as string,
        className: props.className,
      } as never,
      props.children as never,
    );
  }
  return {
    ...actual,
    useParams: () => ({ owner: "ada", repo: "hello", n: "1" }),
    useLoaderData: () => undefined,
    useNavigate: () => vi.fn(),
    Link: MockLink,
  };
});

const readableRepo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-09-14T00:00:00Z",
  can_admin: true,
  can_write: true,
};

const sampleIssue = {
  id: "i1",
  repo_id: "r1",
  number: 1,
  title: "Tracer issue",
  body: "Hello **world**",
  state: "open" as const,
  author_id: "u1",
  author_username: "ada",
  created_at: "2026-09-14T00:00:00Z",
  updated_at: "2026-09-14T00:00:00Z",
  labels: [],
  assignees: [],
  reactions: [],
};

beforeEach(() => {
  getMock.mockReset();
  listMock.mockReset();
  issueGetMock.mockReset();
  createMock.mockReset();
  historyMock.mockReset();
  updateMock.mockReset();
  closeMock.mockReset();
  reopenMock.mockReset();
  deleteMock.mockReset();
  commentsListMock.mockReset();
  commentsCreateMock.mockReset();
  commentsUpdateMock.mockReset();
  commentsDeleteMock.mockReset();
  commentsHistoryMock.mockReset();
  labelListForRepoMock.mockReset();
  labelsSetMock.mockReset();
  assigneeCandidatesMock.mockReset();
  assigneesSetMock.mockReset();
  reactionsToggleMock.mockReset();
  linksListMock.mockReset();
  linksAddMock.mockReset();
  linksRemoveMock.mockReset();
  userLookupMock.mockReset();
  getMock.mockResolvedValue({ ok: true, data: readableRepo });
  listMock.mockResolvedValue({
    ok: true,
    data: { issues: [], total: 0 },
  });
  issueGetMock.mockResolvedValue({ ok: true, data: sampleIssue });
  createMock.mockResolvedValue({ ok: true, data: sampleIssue });
  historyMock.mockResolvedValue({ ok: true, data: { revisions: [] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsHistoryMock.mockResolvedValue({ ok: true, data: { revisions: [] } });
  labelListForRepoMock.mockResolvedValue({ ok: true, data: { labels: [] } });
  labelsSetMock.mockResolvedValue({ ok: true, data: sampleIssue });
  assigneeCandidatesMock.mockResolvedValue({
    ok: true,
    data: {
      users: [
        { user_id: "u1", username: "ada", display_name: "Ada" },
        { user_id: "u2", username: "grace", display_name: "Grace" },
      ],
    },
  });
  assigneesSetMock.mockResolvedValue({ ok: true, data: sampleIssue });
  reactionsToggleMock.mockResolvedValue({
    ok: true,
    data: {
      reactions: [{ content: "+1", count: 1, viewerHasReacted: true }],
      reacted: true,
    },
  });
  linksListMock.mockResolvedValue({ ok: true, data: { links: [] } });
  linksAddMock.mockResolvedValue({
    ok: true,
    data: {
      id: "lnk1",
      kind: "pr_stub",
      target_number: 42,
      title: "Stub PR",
      created_at: "2026-09-14T00:00:00Z",
    },
  });
  linksRemoveMock.mockResolvedValue({ ok: true, data: { ok: true } });
  userLookupMock.mockResolvedValue({
    ok: true,
    data: {
      users: [{ username: "ada", display_name: "Ada", avatar_url: null }],
    },
  });
});

afterEach(cleanup);

async function loadIssuesListModule(): Promise<Record<string, unknown>> {
  const rel = "./$owner.$repo.issues.index";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `Wave 0: /{owner}/{repo}/issues missing — implement in 11-03 (D-ISS-16 / D-ISS-19). ${(err as Error).message}`,
    );
  }
}

async function loadIssuesNewModule(): Promise<Record<string, unknown>> {
  const rel = "./$owner.$repo.issues.new";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `Wave 0: /{owner}/{repo}/issues/new missing — implement in 11-03 (D-ISS-19 / D-ISS-10). ${(err as Error).message}`,
    );
  }
}

async function loadIssueDetailModule(): Promise<Record<string, unknown>> {
  const rel = "./$owner.$repo.issues.$n";
  try {
    return (await import(/* @vite-ignore */ rel)) as Record<string, unknown>;
  } catch (err) {
    throw new Error(
      `Wave 0: /{owner}/{repo}/issues/{n} missing — implement in 11-03/11-04 (ISS-01..04). ${(err as Error).message}`,
    );
  }
}

function issuesListPage(mod: Record<string, unknown>): unknown {
  const page = mod.IssuesListPage ?? mod.IssuesPage ?? mod.default;
  expect(
    page,
    "Wave 0: IssuesListPage (or IssuesPage) must be exported from $owner.$repo.issues.index",
  ).toBeTruthy();
  return page;
}

function issuesNewPage(mod: Record<string, unknown>): unknown {
  const page = mod.IssuesNewPage ?? mod.NewIssuePage ?? mod.default;
  expect(page, "Wave 0: IssuesNewPage must be exported from $owner.$repo.issues.new").toBeTruthy();
  return page;
}

function issueDetailPage(mod: Record<string, unknown>): unknown {
  const page = mod.IssueDetailPage ?? mod.IssuesDetailPage ?? mod.default;
  expect(page, "Wave 0: IssueDetailPage must be exported from $owner.$repo.issues.$n").toBeTruthy();
  return page;
}

describe("repo chrome Issues tab Wave 0 (D-ISS-19)", () => {
  it("Issues tab present for readable repos with href /{owner}/{repo}/issues", async () => {
    const { RepoChrome } = await import("../components/repo/repo-chrome");
    render(
      createElement(RepoChrome as never, {
        repo: readableRepo,
        active: "code",
      }) as never,
    );

    await waitFor(
      () => {
        expect(screen.getByRole("link", { name: "Issues" })).toBeInTheDocument();
      },
      { timeout: 20_000 },
    );
    expect(screen.getByRole("link", { name: "Issues" })).toHaveAttribute(
      "href",
      "/ada/hello/issues",
    );
  }, 30_000);

  it("issues leaves do not remount RepoChrome (D-QH-01)", async () => {
    const sources = await Promise.all([
      import("./$owner.$repo.issues.index.tsrx?raw"),
      import("./$owner.$repo.issues.new.tsrx?raw"),
      import("./$owner.$repo.issues.$n.tsrx?raw"),
      import("./$owner.$repo.issues.labels.tsrx?raw"),
    ]);
    const names = ["issues.index", "issues.new", "issues.$n", "issues.labels"];
    for (let i = 0; i < sources.length; i++) {
      const src = String((sources[i] as { default: string }).default);
      expect(src, `${names[i]} must not remount RepoChrome`).not.toMatch(/RepoChrome/);
    }
  }, 30_000);
});

describe("/{owner}/{repo}/issues list Wave 0 (D-ISS-16 / D-ISS-19)", () => {
  it("list defaults Open with Closed and All controls", async () => {
    const mod = await loadIssuesListModule();
    const { container } = renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(
        screen.getByRole("tab", { name: /^Open$/i }) ??
          screen.getByRole("link", { name: /^Open$/i }) ??
          screen.getByText(/^Open$/i),
      ).toBeTruthy();
    });
    expect(screen.getByText(/^Closed$/i)).toBeInTheDocument();
    expect(screen.getByText(/^All$/i)).toBeInTheDocument();
    // Prefer tablist selection — repo chrome may also set aria-current on Issues
    const openControl =
      container.querySelector('[role="tablist"] [aria-selected="true"]') ??
      container.querySelector('[aria-selected="true"]') ??
      container.querySelector('[data-state="active"]');
    expect(openControl?.textContent).toMatch(/Open/i);
  }, 15_000);

  it("author/label/assignee/text filters and Apply (D-ISS-17)", async () => {
    labelListForRepoMock.mockResolvedValue({
      ok: true,
      data: {
        labels: [
          {
            id: "lab-bug",
            name: "bug",
            color: "d73a4a",
            description: "",
            scope: "repo",
          },
        ],
      },
    });
    const mod = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("form", { name: /Issue filters/i })).toBeInTheDocument();
    });
    expect(screen.getByLabelText(/^Author$/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/^Label$/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/^Assignee$/i)).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/Search title or body/i)).toBeInTheDocument();

    fireEvent.input(screen.getByLabelText(/^Author$/i), {
      target: { value: "ada" },
    });
    fireEvent.input(screen.getByPlaceholderText(/Search title or body/i), {
      target: { value: "uniquephrase" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Apply$/i }));

    await waitFor(() => {
      expect(listMock).toHaveBeenCalled();
      const calls = listMock.mock.calls.map((c) => c[0] as Record<string, unknown>);
      expect(
        calls.some(
          (c) =>
            c.author === "ada" && c.q === "uniquephrase" && (c.state === "open" || c.state == null),
        ),
      ).toBe(true);
    });
  }, 15_000);

  it("offset Previous/Next pagination controls (D-ISS-18)", async () => {
    listMock.mockResolvedValue({
      ok: true,
      data: {
        issues: Array.from({ length: 25 }, (_, i) => ({
          ...sampleIssue,
          id: `iss-${i + 1}`,
          number: i + 1,
          title: `Issue ${i + 1}`,
        })),
        total: 40,
      },
    });
    const mod = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(
        screen.getByRole("navigation", { name: /Issue list pagination/i }),
      ).toBeInTheDocument();
    });
    const prev = screen.getByRole("button", { name: /^Previous$/i });
    const next = screen.getByRole("button", { name: /^Next$/i });
    expect(prev).toBeDisabled();
    expect(next).not.toBeDisabled();

    fireEvent.click(next);
    await waitFor(() => {
      const calls = listMock.mock.calls.map((c) => c[0] as { offset?: number });
      expect(calls.some((c) => c.offset === 25)).toBe(true);
    });
  }, 15_000);

  it("New issue from list when can_write (D-ISS-19 / D-ISS-20)", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: { ...readableRepo, can_write: true, can_admin: false },
    });
    const mod = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /New issue/i })).toBeInTheDocument();
    });
    expect(screen.getByRole("link", { name: /New issue/i })).toHaveAttribute(
      "href",
      "/ada/hello/issues/new",
    );
  }, 15_000);

  it("New issue visible for verified viewer without can_write (D-ISS-20)", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: { ...readableRepo, can_write: false, can_admin: false },
    });
    const mod = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(screen.getByText(/No open issues|No issues/i)).toBeInTheDocument();
    });
    expect(screen.getByRole("link", { name: /New issue/i })).toBeInTheDocument();
  }, 15_000);

  it("Admin label settings entry gated by can_admin (D-ISS-07)", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: { ...readableRepo, can_write: true, can_admin: true },
    });
    const mod = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /Labels|Manage labels/i })).toBeInTheDocument();
    });

    getMock.mockResolvedValue({
      ok: true,
      data: { ...readableRepo, can_write: true, can_admin: false },
    });
    cleanup();
    const mod2 = await loadIssuesListModule();
    renderWithQueryClient(issuesListPage(mod2));
    await waitFor(() => {
      expect(screen.getByText(/^Open$/i)).toBeInTheDocument();
    });
    expect(screen.queryByRole("link", { name: /Labels|Manage labels/i })).not.toBeInTheDocument();
  }, 15_000);
});

describe("/{owner}/{repo}/issues/new Wave 0 (D-ISS-10)", () => {
  it("Write|Preview tabs on new issue form", async () => {
    const mod = await loadIssuesNewModule();
    renderWithQueryClient(issuesNewPage(mod));

    await waitFor(() => {
      expect(screen.getByText(/^Write$/i)).toBeInTheDocument();
    });
    expect(screen.getByText(/^Preview$/i)).toBeInTheDocument();
  }, 15_000);
});

describe("/{owner}/{repo}/issues/{n} detail Wave 0 (ISS-01..04 / D-ISS-13)", () => {
  it("detail shows title/body/comments/labels/assignees + Linked PRs panel shell", async () => {
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(screen.getByTestId("issue-title")).toBeInTheDocument();
    });
    expect(screen.getByTestId("issue-title").textContent).toMatch(/Tracer issue/i);
    expect(screen.getByRole("heading", { name: /^Comments$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^Labels$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^Assignees$/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^Linked PRs$/i })).toBeInTheDocument();
    expect(screen.getByTestId("issue-linked-prs")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/No linked pull requests yet/i)).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: /^Link$/i })).toBeInTheDocument();
  }, 15_000);

  it("Linked PRs panel lists stub rows and Link control (ISS-04 / D-ISS-13 / D-ISS-14)", async () => {
    linksListMock.mockResolvedValue({
      ok: true,
      data: {
        links: [
          {
            id: "lnk-stub",
            kind: "pr_stub",
            target_number: 7,
            title: "Panel stub",
            created_at: "2026-09-14T00:00:00Z",
          },
        ],
      },
    });
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(screen.getByTestId("issue-linked-prs-list")).toBeInTheDocument();
    });
    expect(screen.getByText(/PR stub #7: Panel stub/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Link$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Unlink PR stub #7/i })).toBeInTheDocument();
  }, 15_000);

  it("Write|Preview on edit form after Edit (D-ISS-10)", async () => {
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^Edit$/i })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: /^Edit$/i }).click();

    await waitFor(() => {
      const writes = screen.getAllByText(/^Write$/i);
      const previews = screen.getAllByText(/^Preview$/i);
      expect(writes.length).toBeGreaterThanOrEqual(1);
      expect(previews.length).toBeGreaterThanOrEqual(1);
    });
  }, 15_000);

  it("comment thread lists comments + Write|Preview compose (ISS-02 / D-ISS-10)", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          {
            id: "c1",
            issue_id: "i1",
            author_id: "u1",
            author_username: "ada",
            body: "First comment",
            created_at: "2026-09-14T00:00:00Z",
            updated_at: "2026-09-14T00:00:00Z",
          },
        ],
      },
    });
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(screen.getByTestId("issue-comments")).toBeInTheDocument();
      expect(screen.getByTestId("issue-comment")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(screen.getByTestId("issue-comment").textContent).toMatch(/First comment/i);
    });
    expect(screen.getByTestId("issue-comment-compose")).toBeInTheDocument();
    expect(screen.getByText(/^Show edit history$/i)).toBeInTheDocument();
    const writes = screen.getAllByText(/^Write$/i);
    const previews = screen.getAllByText(/^Preview$/i);
    expect(writes.length).toBeGreaterThanOrEqual(1);
    expect(previews.length).toBeGreaterThanOrEqual(1);
  }, 15_000);

  it("lifecycle affordances edit/close/reopen + history panel + Admin delete confirm (D-ISS-02 / D-ISS-04)", async () => {
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(
        screen.getAllByRole("button", { name: /Edit|Close issue|Reopen/i }).length,
      ).toBeGreaterThanOrEqual(1);
    });
    expect(screen.getByRole("heading", { name: /^Edit history$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Delete issue$/i })).toBeInTheDocument();
    screen.getByRole("button", { name: /^Delete issue$/i }).click();
    await waitFor(() => {
      expect(screen.getByText(/type.*(issue )?number|confirm/i)).toBeInTheDocument();
    });
  }, 15_000);

  it("Write+ multi-assignee picker from assigneeCandidates (ISS-03 / D-ISS-06 / D-ISS-08)", async () => {
    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^Save assignees$/i })).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(assigneeCandidatesMock).toHaveBeenCalled();
      expect(screen.getByRole("checkbox", { name: /grace/i })).toBeInTheDocument();
    });

    screen.getByRole("checkbox", { name: /grace/i }).click();
    screen.getByRole("button", { name: /^Save assignees$/i }).click();

    await waitFor(() => {
      expect(assigneesSetMock).toHaveBeenCalled();
    });
    const arg = assigneesSetMock.mock.calls[0]?.[0] as {
      userIds?: string[];
    };
    expect(arg.userIds).toContain("u2");
  }, 15_000);

  it("reaction bar visibility + Write+ toggle on issue and comments (D-ISS-11)", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          {
            id: "c1",
            issue_id: "i1",
            author_id: "u1",
            author_username: "ada",
            body: "Nice work",
            created_at: "2026-09-14T00:00:00Z",
            updated_at: "2026-09-14T00:00:00Z",
            reactions: [],
          },
        ],
      },
    });

    const mod = await loadIssueDetailModule();
    renderWithQueryClient(issueDetailPage(mod));

    await waitFor(() => {
      expect(
        screen.getAllByRole("toolbar", { name: /^Reactions$/i }).length,
      ).toBeGreaterThanOrEqual(2);
    });
    expect(document.body.textContent).toMatch(/\+1|👍|react/i);

    const plusOne = await screen.findAllByRole("button", {
      name: /React \+1/i,
    });
    expect(plusOne.length).toBeGreaterThanOrEqual(1);
    plusOne[0]!.click();

    await waitFor(() => {
      expect(reactionsToggleMock).toHaveBeenCalled();
    });
    const arg = reactionsToggleMock.mock.calls[0]?.[0] as {
      target?: string;
      content?: string;
    };
    expect(arg.target).toBe("issue");
    expect(arg.content).toBe("+1");
  }, 15_000);
});
