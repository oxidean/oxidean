/**
 * `/oxpilot` — index mount, flag-off explanatory state, and the session
 * workspace composer append path over a stubbed SSE POST.
 */
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const statusMock = vi.fn();
const sessionListMock = vi.fn();
const sessionCreateMock = vi.fn();
const sessionGetMock = vi.fn();
const sessionDeleteMock = vi.fn();
const targetListMock = vi.fn();
const grantListMock = vi.fn();
const grantGetMock = vi.fn();
const queueListMock = vi.fn();
const diffListMock = vi.fn();
const diffGetMock = vi.fn();
const messageQueueMock = vi.fn();
const turnCancelMock = vi.fn();
const diffApplyMock = vi.fn();
const grantRespondMock = vi.fn();
const queueDeleteMock = vi.fn();
const targetDisconnectMock = vi.fn();
const targetRevokeMock = vi.fn();
const pairingCreateMock = vi.fn();
const repoListMineMock = vi.fn();
const repoRefsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    oxpilot: {
      status: (...args: unknown[]) => statusMock(...args),
      sessionList: (...args: unknown[]) => sessionListMock(...args),
      sessionCreate: (...args: unknown[]) => sessionCreateMock(...args),
      sessionGet: (...args: unknown[]) => sessionGetMock(...args),
      sessionDelete: (...args: unknown[]) => sessionDeleteMock(...args),
      targetList: (...args: unknown[]) => targetListMock(...args),
      grantList: (...args: unknown[]) => grantListMock(...args),
      grantGet: (...args: unknown[]) => grantGetMock(...args),
      queueList: (...args: unknown[]) => queueListMock(...args),
      diffList: (...args: unknown[]) => diffListMock(...args),
      diffGet: (...args: unknown[]) => diffGetMock(...args),
      messageQueue: (...args: unknown[]) => messageQueueMock(...args),
      turnCancel: (...args: unknown[]) => turnCancelMock(...args),
      diffApply: (...args: unknown[]) => diffApplyMock(...args),
      grantRespond: (...args: unknown[]) => grantRespondMock(...args),
      queueDelete: (...args: unknown[]) => queueDeleteMock(...args),
      targetDisconnect: (...args: unknown[]) => targetDisconnectMock(...args),
      targetRevoke: (...args: unknown[]) => targetRevokeMock(...args),
      pairingCreate: (...args: unknown[]) => pairingCreateMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => repoListMineMock(...args),
      refs: (...args: unknown[]) => repoRefsMock(...args),
    },
    org: {
      listMine: () => Promise.resolve({ ok: true, data: { orgs: [] } }),
    },
  },
}));

import { OxpilotPage } from "./oxpilot.tsrx";
import { OxpilotSessionPage } from "./oxpilot.$session.tsrx";

const user = {
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

const repo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user",
  owner_username: "ada",
  name: "proj",
  description: "",
  visibility: "public",
  default_branch: "main",
  updated_at: "2026-01-01T00:00:00Z",
};

const session = {
  id: "s-1",
  user_id: "u1",
  repo_id: "r1",
  repo_owner: "ada",
  repo_name: "proj",
  branch: "main",
  goal: null,
  title: "tidy the tests",
  status: "active",
  created_at: "2026-01-01T00:00:00Z",
  last_activity_at: "2026-01-01T00:00:00Z",
  computer_use_target_id: null as string | null,
};

function makeTarget(over: Record<string, unknown> = {}) {
  return {
    id: "t-1",
    label: "desk-box",
    status: "ok",
    lifecycle: "connected",
    pairing_id: "pair-1",
    granted_caps: ["exec_shell"],
    ...over,
  };
}

function makeGrant(over: Record<string, unknown> = {}) {
  return {
    id: "g-1",
    kind: "action",
    capability: "exec_shell",
    args_summary: "npm test",
    session_id: "s-1",
    state: "pending",
    expires_at: "2027-01-01T00:00:00Z",
    ...over,
  };
}

function makeDiff(over: Record<string, unknown> = {}) {
  return {
    id: "d-1",
    session_id: "s-1",
    title: "tidy tests",
    base: "main",
    status: "pending",
    pr_ref: null as string | null,
    files: [{ path: "src/a.ts", patch: "@@ -1 +1 @@\n-old\n+new" }],
    created_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function enabledStatus() {
  statusMock.mockResolvedValue({
    ok: true,
    data: {
      enabled: true,
      provider: { id: "stub", model: "stub-1" },
      reason: null,
    },
  });
  // The reseed's read set + rail/target surfaces resolve to empty
  // collections unless a test overrides them.
  sessionGetMock.mockResolvedValue({ ok: true, data: { session } });
  targetListMock.mockResolvedValue({ ok: true, data: { targets: [] } });
  grantListMock.mockResolvedValue({ ok: true, data: { grants: [] } });
  queueListMock.mockResolvedValue({ ok: true, data: { queued: [] } });
  diffListMock.mockResolvedValue({ ok: true, data: { diffs: [] } });
  repoRefsMock.mockResolvedValue({ ok: true, data: { refs: [] } });
  for (const m of [
    sessionDeleteMock,
    messageQueueMock,
    turnCancelMock,
    diffApplyMock,
    grantRespondMock,
    queueDeleteMock,
    targetDisconnectMock,
    targetRevokeMock,
  ]) {
    m.mockResolvedValue({ ok: true, data: {} });
  }
}

beforeEach(() => {
  for (const m of [
    meMock,
    statusMock,
    sessionListMock,
    sessionCreateMock,
    sessionGetMock,
    sessionDeleteMock,
    targetListMock,
    grantListMock,
    grantGetMock,
    queueListMock,
    diffListMock,
    diffGetMock,
    messageQueueMock,
    turnCancelMock,
    diffApplyMock,
    grantRespondMock,
    queueDeleteMock,
    targetDisconnectMock,
    targetRevokeMock,
    pairingCreateMock,
    repoListMineMock,
    repoRefsMock,
  ]) {
    m.mockReset();
  }
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("/oxpilot index", () => {
  it("flag-off: renders the explanatory state naming the envs", async () => {
    window.history.pushState({}, "", "/oxpilot");
    meMock.mockResolvedValue({ ok: true, data: user });
    statusMock.mockResolvedValue({
      ok: true,
      data: { enabled: false, provider: null, reason: "flag_off" },
    });
    repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [] } });
    renderWithQueryClient(OxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-page")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(screen.getByText(/Oxpilot is not enabled/)).toBeInTheDocument();
    });
    expect(screen.getByText(/OXIDEAN_OXPILOT_ENABLED/)).toBeInTheDocument();
    expect(screen.getByText(/OXIDEAN_OXPILOT_PROVIDER/)).toBeInTheDocument();
    expect(screen.getByText(/docs\/CONFIGURATION\.md/)).toBeInTheDocument();
    expect(screen.queryByTestId("oxpilot-new-session")).not.toBeInTheDocument();
  });

  it("restricted (stub): renders the route as absent — no OxPilot surface", async () => {
    window.history.pushState({}, "", "/oxpilot");
    meMock.mockResolvedValue({ ok: true, data: user });
    statusMock.mockResolvedValue({
      ok: true,
      data: { enabled: false, provider: null, reason: "restricted" },
    });
    repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [] } });
    renderWithQueryClient(OxpilotPage);

    await waitFor(() => {
      expect(screen.getByText("Page not found")).toBeInTheDocument();
    });
    // BR2.6 — no header, no flag-off explainer, no controls.
    expect(screen.queryByText(/Oxpilot is not enabled/)).not.toBeInTheDocument();
    expect(screen.queryByText("OxPilot")).not.toBeInTheDocument();
    expect(screen.queryByText(/OXIDEAN_OXPILOT/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("oxpilot-new-session-open")).not.toBeInTheDocument();
    expect(sessionListMock).not.toHaveBeenCalled();
    expect(targetListMock).not.toHaveBeenCalled();
  });

  it("enabled: renders the session list + new-session form", async () => {
    window.history.pushState({}, "", "/oxpilot");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    sessionListMock.mockResolvedValue({
      ok: true,
      data: { sessions: [session] },
    });
    repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [repo] } });
    renderWithQueryClient(OxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-session-list")).toBeInTheDocument();
    });
    expect(screen.getByText("tidy the tests")).toBeInTheDocument();
    expect(screen.getByText(/ada\/proj · main/)).toBeInTheDocument();
    expect(screen.getByTestId("oxpilot-new-session")).toBeInTheDocument();
    expect(screen.getByTestId("oxpilot-repo-trigger")).toBeInTheDocument();
    const link = screen.getByText("tidy the tests").closest("a");
    expect(link?.getAttribute("href")).toBe("/oxpilot/s-1");
  });
});

describe("/oxpilot/{session} workspace", () => {
  /**
   * Live attach fakes — the constructor replays the post-attach `snapshot`
   * (skipped by the workspace's `attached` arm); tests then `emit` frames
   * or drive `onerror` with a forced `readyState` for the REL-3 arms.
   */
  const liveSources: {
    url: string;
    readyState: number;
    emit(frame: object): void;
    error(): void;
    close(): void;
  }[] = [];

  function lastSource() {
    const src = liveSources[liveSources.length - 1];
    if (!src) throw new Error("no live EventSource");
    return src;
  }

  function emitFrame(frame: object) {
    lastSource().emit(frame);
  }

  function stubEventSource() {
    // Snapshot replay over the GET stream — an empty transcript attach.
    liveSources.length = 0;
    class FakeES {
      static CONNECTING = 0;
      static OPEN = 1;
      static CLOSED = 2;
      onmessage: ((ev: { data: string }) => void) | null = null;
      onerror: (() => void) | null = null;
      readyState = 1;
      constructor(public url: string) {
        liveSources.push(this);
        queueMicrotask(() => {
          this.onmessage?.({
            data: JSON.stringify({
              type: "snapshot",
              session_id: "s-1",
              messages: [
                {
                  id: "m-user",
                  seq: 1,
                  role: "user",
                  content: "seed message",
                  status: "complete",
                },
              ],
              active_turn: null,
              pending_queue: [],
            }),
          });
        });
      }
      emit(frame: object) {
        this.onmessage?.({ data: JSON.stringify(frame) });
      }
      error() {
        this.onerror?.();
      }
      close() {
        this.readyState = 2;
      }
    }
    vi.stubGlobal("EventSource", FakeES as unknown as typeof EventSource);
  }

  function sseResponse(frames: object[]): Response {
    const text = frames.map((f) => `data: ${JSON.stringify(f)}\n\n`).join("");
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(text));
        controller.close();
      },
    });
    return new Response(stream, { status: 200 });
  }

  const seedMessages = [
    {
      id: "m-user",
      seq: 1,
      role: "user",
      content: "seed message",
      status: "complete",
    },
  ];

  function snapshotFrame(over: Record<string, unknown> = {}) {
    return {
      type: "snapshot",
      session_id: "s-1",
      messages: seedMessages,
      active_turn: null,
      pending_queue: [],
      ...over,
    };
  }

  function fetchFor(snapshot: object[], post: object[]) {
    return vi.fn().mockImplementation((url: string) => {
      if (url.endsWith("/chat/stream")) {
        return Promise.resolve(sseResponse(snapshot));
      }
      return Promise.resolve(sseResponse(post));
    });
  }

  it("mounts the transcript and streams the composer reply", async () => {
    window.history.pushState({}, "", "/oxpilot/s-1");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    stubEventSource();
    const fetchMock = fetchFor(
      [snapshotFrame()],
      [
        {
          type: "append",
          message_id: "m-a1",
          seq: 1,
          role: "assistant",
          delta: "Hello ",
        },
        {
          type: "append",
          message_id: "m-a1",
          seq: 2,
          role: "assistant",
          delta: "there",
        },
        { type: "terminal", message_id: "m-a1", result: "done" },
      ],
    );
    vi.stubGlobal("fetch", fetchMock);

    renderWithQueryClient(OxpilotSessionPage);

    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });

    const composer = screen.getByTestId("oxpilot-composer");
    const textarea = composer.querySelector("textarea");
    expect(textarea).toBeTruthy();
    fireEvent.input(textarea as Element, { target: { value: "hi ox" } });
    fireEvent.submit(composer);

    await waitFor(() => {
      expect(screen.getByText("Hello there")).toBeInTheDocument();
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/oxpilot/sessions/s-1/chat",
      expect.objectContaining({ method: "POST" }),
    );
    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith("/chat") && !String(c[0]).endsWith("/chat/stream"),
    );
    const body = JSON.parse((postCall![1] as { body: string }).body) as {
      content: string;
      client_msg_id?: string;
    };
    expect(body.content).toBe("hi ox");
    expect(body.client_msg_id).toBeTruthy();
  });

  it("composer denial surfaces the error banner (fallback)", async () => {
    window.history.pushState({}, "", "/oxpilot/s-1");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    stubEventSource();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((url: string) => {
        if (url.endsWith("/chat/stream")) {
          return Promise.resolve(sseResponse([snapshotFrame()]));
        }
        return Promise.resolve(
          new Response(
            JSON.stringify({
              error: { code: "turn_active", message: "a turn is live" },
            }),
            { status: 409 },
          ),
        );
      }),
    );

    renderWithQueryClient(OxpilotSessionPage);
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "again" },
    });
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(screen.getByText(/turn_active/)).toBeInTheDocument();
    });
    // Draft survives the denial — the composer never clears a failed send.
    expect((composer.querySelector("textarea") as HTMLTextAreaElement).value).toBe("again");
    // …and a 409'd idle POST never silently auto-enqueues (C2's forbidden pair).
    expect(messageQueueMock).not.toHaveBeenCalled();
  });

  function mountWorkspace(
    fetchImpl: (url: string, init?: RequestInit) => Promise<Response>,
    configure?: () => void,
  ) {
    window.history.pushState({}, "", "/oxpilot/s-1");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    // Per-test mock overrides land after the enabledStatus defaults and
    // before render so the mount's 4-read set sees them.
    configure?.();
    stubEventSource();
    const fetchMock = vi.fn().mockImplementation(fetchImpl);
    vi.stubGlobal("fetch", fetchMock);
    renderWithQueryClient(OxpilotSessionPage);
    return fetchMock;
  }

  const defaultPost: object[] = [
    { type: "append", message_id: "m-a1", seq: 1, role: "assistant", delta: "ok" },
    { type: "terminal", message_id: "m-a1", result: "done" },
  ];

  function mountWorkspaceOk(snapshot: object[] = [snapshotFrame()], configure?: () => void) {
    return mountWorkspace(
      (url) =>
        Promise.resolve(
          url.endsWith("/chat/stream") ? sseResponse(snapshot) : sseResponse(defaultPost),
        ),
      configure,
    );
  }

  it("mounts with the pinned 4-read set (snapshot + session + grants + diffs)", async () => {
    const fetchMock = mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    // GET snapshot read — the one-shot reseed stream, credentials-included.
    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(
          (c) =>
            String(c[0]) === "/api/oxpilot/sessions/s-1/chat/stream" &&
            (c[1] as RequestInit | undefined)?.method !== "POST",
        ),
      ).toBe(true);
    });
    expect(sessionGetMock).toHaveBeenCalledWith({ session_id: "s-1" });
    expect(grantListMock).toHaveBeenCalledWith({ session_id: "s-1" });
    expect(diffListMock).toHaveBeenCalledWith({ session_id: "s-1" });
    // The transcript's boundary-only live region is present (BR10.5).
    const live = document.querySelector('[aria-live="polite"]');
    expect(live).not.toBeNull();
    expect(live?.getAttribute("aria-atomic")).toBe("false");
  });

  it("seed pending_queue renders queued rows; a LATER snapshot reseeds wholesale", async () => {
    let streamReads = 0;
    mountWorkspace((url) => {
      if (url.endsWith("/chat/stream")) {
        streamReads += 1;
        return Promise.resolve(
          sseResponse(
            streamReads === 1
              ? [
                  snapshotFrame({
                    pending_queue: [
                      {
                        id: "q-seed",
                        queue_seq: 1,
                        content: "queued earlier",
                        client_msg_id: null,
                        created_at: "2026-01-01T00:00:00Z",
                      },
                    ],
                  }),
                ]
              : [snapshotFrame()],
          ),
        );
      }
      return Promise.resolve(sseResponse(defaultPost));
    });
    await waitFor(() => {
      expect(queuedRowEl("queued earlier")).toBeTruthy();
    });
    // A post-attach snapshot is a reconnect replay — the seed re-reads
    // (a second /chat/stream GET) and the remount drops the stale row.
    emitFrame(snapshotFrame());
    await waitFor(() => {
      expect(streamReads).toBe(2);
    });
    await waitFor(() => {
      expect(queuedRowEl("queued earlier")).toBeNull();
    });
  });

  it("pending grant/proposal rows reconstruct consent + diff cards on mount", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      grantListMock.mockResolvedValue({ ok: true, data: { grants: [makeGrant()] } });
      diffListMock.mockResolvedValue({ ok: true, data: { diffs: [makeDiff()] } });
      grantGetMock.mockResolvedValue({ ok: true, data: makeGrant() });
      diffGetMock.mockResolvedValue({ ok: true, data: makeDiff() });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-consent")).toBeInTheDocument();
    });
    expect(grantGetMock).toHaveBeenCalledWith({ grant_id: "g-1" });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-diff-card")).toBeInTheDocument();
    });
    expect(diffGetMock).toHaveBeenCalledWith({ proposal_id: "d-1" });
  });

  it("grantList{session_id} refused degrades to the 3-read set (BR7.4)", async () => {
    // An unamended server refuses the flagged session_id filter — the
    // mount still seeds; pending consent never fabricates a card.
    mountWorkspaceOk([snapshotFrame()], () => {
      grantListMock.mockResolvedValue({
        ok: false,
        error: { code: "forbidden", message: "session_id unsupported" },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    expect(grantListMock).toHaveBeenCalledWith({ session_id: "s-1" });
    expect(grantGetMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("oxpilot-consent")).not.toBeInTheDocument();
  });

  it("not_found seed renders SessionNotFound + the back link", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      sessionGetMock.mockResolvedValue({
        ok: false,
        error: { code: "not_found", message: "no such session" },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("Session not found")).toBeInTheDocument();
    });
    // The page's own back-nav also reads "All sessions" — the EmptyState
    // action is the second anchor.
    const backs = screen.getAllByText("All sessions");
    expect(backs.length).toBeGreaterThanOrEqual(2);
    const back = backs[backs.length - 1]?.closest("a");
    expect(back?.getAttribute("href")).toBe("/oxpilot");
    expect(screen.queryByTestId("oxpilot-composer")).not.toBeInTheDocument();
  });

  it("flag-off on the session route swaps to the explanatory state", async () => {
    statusMock.mockResolvedValue({
      ok: true,
      data: { enabled: false, provider: null, reason: "flag_off" },
    });
    window.history.pushState({}, "", "/oxpilot/s-1");
    meMock.mockResolvedValue({ ok: true, data: user });
    stubEventSource();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((_url: string) => Promise.resolve(sseResponse([snapshotFrame()]))),
    );
    renderWithQueryClient(OxpilotSessionPage);
    await waitFor(() => {
      expect(screen.getByText(/Oxpilot is not enabled/)).toBeInTheDocument();
    });
    expect(screen.getByText(/OXIDEAN_OXPILOT_ENABLED/)).toBeInTheDocument();
    // Zero gated controls — the swap is total (SEC-9/AC5.1.1).
    expect(screen.queryByTestId("oxpilot-composer")).not.toBeInTheDocument();
    expect(screen.queryByTestId("oxpilot-context-toggle")).not.toBeInTheDocument();
    expect(screen.getByText("All sessions").closest("a")?.getAttribute("href")).toBe("/oxpilot");
  });

  it("restricted on the session route renders the page as absent", async () => {
    statusMock.mockResolvedValue({
      ok: true,
      data: { enabled: false, provider: null, reason: "restricted" },
    });
    window.history.pushState({}, "", "/oxpilot/s-1");
    meMock.mockResolvedValue({ ok: true, data: user });
    renderWithQueryClient(OxpilotSessionPage);

    await waitFor(() => {
      expect(screen.getByText("Page not found")).toBeInTheDocument();
    });
    // BR2.6 — even the "All sessions" back link is part of the hidden
    // surface; the gated read set must not fire either.
    expect(screen.queryByText("All sessions")).not.toBeInTheDocument();
    expect(screen.queryByText(/Oxpilot is not enabled/)).not.toBeInTheDocument();
    expect(sessionGetMock).not.toHaveBeenCalled();
  });

  it("tool_call frames update the keyed card running → done", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({
      type: "tool_call",
      call_id: "c-1",
      call_index: 0,
      tool: "exec_shell",
      status: "running",
      args_summary: "ls -la",
    });
    await waitFor(() => {
      expect(screen.getByText("exec_shell")).toBeInTheDocument();
      expect(screen.getByText("running")).toBeInTheDocument();
    });
    emitFrame({
      type: "tool_call",
      call_id: "c-1",
      call_index: 0,
      tool: "exec_shell",
      status: "done",
      args_summary: "ls -la",
      output_summary: "README.md",
    });
    await waitFor(() => {
      expect(screen.getByText("done")).toBeInTheDocument();
      expect(screen.getByText("README.md")).toBeInTheDocument();
    });
    // One keyed row — no duplicate card for the same call_id.
    expect(screen.getAllByText("exec_shell")).toHaveLength(1);
  });

  it("running exec/fs tool card flips the bound target's rail chip to connected·busy", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      sessionGetMock.mockResolvedValue({
        ok: true,
        data: { ...session, computer_use_target_id: "t-1" },
      });
      targetListMock.mockResolvedValue({
        ok: true,
        data: { targets: [makeTarget()] },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("connected·idle")).toBeInTheDocument();
    });
    emitFrame({
      type: "tool_call",
      call_id: "c-busy",
      call_index: 0,
      tool: "fs_read",
      status: "running",
      args_summary: "README.md",
    });
    await waitFor(() => {
      expect(screen.getByText("connected·busy")).toBeInTheDocument();
    });
    emitFrame({
      type: "tool_call",
      call_id: "c-busy",
      call_index: 0,
      tool: "fs_read",
      status: "done",
      output_summary: "contents",
    });
    await waitFor(() => {
      expect(screen.getByText("connected·idle")).toBeInTheDocument();
    });
  });

  it("card{consent} resolves via grant_get and the three buttons respond verbatim", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      grantGetMock.mockResolvedValue({ ok: true, data: makeGrant() });
      grantRespondMock.mockResolvedValue({ ok: true, data: { state: "allowed" } });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "consent", ref_id: "g-1" });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-consent")).toBeInTheDocument();
    });
    expect(grantGetMock).toHaveBeenCalledWith({ grant_id: "g-1" });
    expect(screen.getByText(/asking to exec_shell/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({
        grant_id: "g-1",
        allow: true,
        scope: "once",
      });
    });
    await waitFor(() => {
      expect(screen.getByText(/Allowed — the target may proceed\./)).toBeInTheDocument();
    });
    // Frozen — the three action buttons are gone after resolve.
    expect(screen.queryByRole("button", { name: "Allow once" })).not.toBeInTheDocument();
  });

  it("consent Deny + session-scope payload; grant_not_pending surfaces the typed alert", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      grantGetMock.mockResolvedValue({ ok: true, data: makeGrant({ id: "g-2" }) });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "consent", ref_id: "g-2" });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-consent")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-2", allow: false });
    });
  });

  it("consent respond denial renders role=alert (grant_not_pending)", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      grantGetMock.mockResolvedValue({ ok: true, data: makeGrant({ id: "g-3" }) });
      grantRespondMock.mockResolvedValue({
        ok: false,
        error: { code: "grant_not_pending", message: "already resolved" },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "consent", ref_id: "g-3" });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-consent")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Allow for this session" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({
        grant_id: "g-3",
        allow: true,
        scope: "session",
      });
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(/grant_not_pending/);
    });
  });

  it("card{diff} resolves via diff_get; Show patch <pre>; Create branch + PR rides the confirm", async () => {
    let applied = false;
    mountWorkspaceOk([snapshotFrame()], () => {
      diffGetMock.mockImplementation(() =>
        Promise.resolve({
          ok: true,
          data: applied ? makeDiff({ status: "applied", pr_ref: "ada/proj#7" }) : makeDiff(),
        }),
      );
      diffApplyMock.mockImplementation(() => {
        applied = true;
        return Promise.resolve({
          ok: true,
          data: { proposal_id: "d-1", pr_ref: "ada/proj#7" },
        });
      });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "diff", ref_id: "d-1" });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-diff-card")).toBeInTheDocument();
    });
    expect(diffGetMock).toHaveBeenCalledWith({ proposal_id: "d-1" });
    expect(screen.getByText("tidy tests")).toBeInTheDocument();
    expect(screen.getByText(/Proposed on main · 1 file/)).toBeInTheDocument();

    // Never auto-applied — diff_apply only runs behind the explicit confirm.
    expect(diffApplyMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Show patch" }));
    await waitFor(() => {
      const pre = screen.getByTestId("oxpilot-diff-card").querySelector("pre");
      expect(pre?.textContent).toContain("-old");
      expect(pre?.textContent).toContain("+new");
    });

    fireEvent.click(screen.getByRole("button", { name: "Create branch + PR" }));
    // The shared confirm carries the verbatim destructive label.
    const confirm = await screen.findByRole("alertdialog");
    expect(confirm).toHaveTextContent("Create branch + PR?");
    fireEvent.click(
      screen
        .getAllByRole("button", { name: "Create branch + PR" })
        .find((b) => confirm.contains(b)) as Element,
    );
    await waitFor(() => {
      expect(diffApplyMock).toHaveBeenCalledWith({ proposal_id: "d-1" });
    });
    // Applied diff shows its pr_ref link after the invalidate refetch.
    await waitFor(() => {
      const link = screen.getByText("ada/proj#7").closest("a");
      expect(link?.getAttribute("href")).toBe("/ada/proj/pull/7");
    });
  });

  it("card{unknown} renders the generic notice, never raw payload", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "mystery_kind", ref_id: "x-1" });
    await waitFor(() => {
      expect(
        screen.getByText(/Unsupported card kind “mystery_kind” — update your client\./),
      ).toBeInTheDocument();
    });
  });

  it("card{queued} refetches queue_list → QueuedRow", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      queueListMock.mockResolvedValue({
        ok: true,
        data: {
          queued: [
            {
              id: "q-1",
              queue_seq: 1,
              content: "follow up after",
              client_msg_id: null,
              created_at: "2026-01-01T00:00:00Z",
            },
          ],
        },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "queued", ref_id: "q-1" });
    await waitFor(() => {
      expect(queuedRowEl("follow up after")).toBeTruthy();
    });
    expect(queueListMock).toHaveBeenCalledWith({ session_id: "s-1" });
  });

  /** Queued rows are split text nodes — match by their role="status" surface. */
  function queuedRowEl(content: string): Element | null {
    return (
      [...document.querySelectorAll('[role="status"]')].find(
        (el) => (el.textContent ?? "").includes(content) && /queued/.test(el.textContent ?? ""),
      ) ?? null
    );
  }

  it("live-turn send goes through message_queue with the verbatim payload", async () => {
    const fetchMock = mountWorkspaceOk(
      [
        snapshotFrame({
          active_turn: { turn_id: "turn-1", message_id: "m-live", started_seq: 2 },
        }),
      ],
      () => {
        messageQueueMock.mockResolvedValue({
          ok: true,
          data: { queued_id: "q-9", position: 1 },
        });
        queueListMock.mockResolvedValue({
          ok: true,
          data: {
            queued: [
              {
                id: "q-9",
                queue_seq: 1,
                content: "while busy",
                client_msg_id: null,
                created_at: "2026-01-01T00:00:00Z",
              },
            ],
          },
        });
      },
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    expect((composer.querySelector("textarea") as HTMLTextAreaElement).placeholder).toMatch(
      /queues/,
    );
    const submitBtn = screen.getByRole("button", { name: /^Queue$/ });
    // The composer is NEVER disabled during a live turn (WF4).
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "while busy" },
    });
    expect(submitBtn).not.toBeDisabled();
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(messageQueueMock).toHaveBeenCalledWith(
        expect.objectContaining({ session_id: "s-1", content: "while busy" }),
      );
    });
    const body = messageQueueMock.mock.calls[0]![0] as { client_msg_id?: string };
    expect(body.client_msg_id).toBeTruthy();
    // The receipt mints a local row; the server row wins after the refresh.
    await waitFor(() => {
      expect(queuedRowEl("while busy")).toBeTruthy();
    });
    expect(queueListMock).toHaveBeenCalledWith({ session_id: "s-1" });
    // The POST send path stays cold — no /chat POST while turnLive.
    expect(
      fetchMock.mock.calls.every(
        (c) =>
          !(String(c[0]).endsWith("/chat") && (c[1] as RequestInit | undefined)?.method === "POST"),
      ),
    ).toBe(true);
    // The replayed snapshot's active_turn:null disagrees with the seeded
    // live turn — the first-attach reconcile reseeds once, then the
    // signature guard suppresses the identical repeat instead of
    // remounting forever (initial attach + one remount = 2 sources).
    expect(liveSources.length).toBeLessThanOrEqual(2);
  });

  it("{null,null} queue receipt renders no fabricated row", async () => {
    mountWorkspaceOk(
      [
        snapshotFrame({
          active_turn: { turn_id: "turn-1", message_id: "m-live", started_seq: 2 },
        }),
      ],
      () => {
        messageQueueMock.mockResolvedValue({
          ok: true,
          data: { queued_id: null, position: null },
        });
      },
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "merged" },
    });
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(messageQueueMock).toHaveBeenCalled();
    });
    // The draft clears on success but NO queued row is ever fabricated.
    await waitFor(() => {
      expect((composer.querySelector("textarea") as HTMLTextAreaElement).value).toBe("");
    });
    expect(queuedRowEl("merged")).toBeNull();
    expect(document.body.textContent ?? "").not.toContain("merged — queued");
  });

  it("queue denial surfaces the typed banner and keeps the draft", async () => {
    mountWorkspaceOk(
      [
        snapshotFrame({
          active_turn: { turn_id: "turn-1", message_id: "m-live", started_seq: 2 },
        }),
      ],
      () => {
        messageQueueMock.mockResolvedValue({
          ok: false,
          error: { code: "session_active_turn_absent", message: "no active turn" },
        });
      },
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "still mine" },
    });
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(screen.getByText(/session_active_turn_absent/)).toBeInTheDocument();
    });
    expect((composer.querySelector("textarea") as HTMLTextAreaElement).value).toBe("still mine");
  });

  it("inline cancel rides queue_delete then the row drops locally", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      queueListMock
        .mockResolvedValueOnce({
          ok: true,
          data: {
            queued: [
              {
                id: "q-1",
                queue_seq: 1,
                content: "drop me",
                client_msg_id: null,
                created_at: "2026-01-01T00:00:00Z",
              },
            ],
          },
        })
        .mockResolvedValue({ ok: true, data: { queued: [] } });
      queueDeleteMock.mockResolvedValue({ ok: true, data: { deleted: true } });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "card", card_kind: "queued", ref_id: "q-1" });
    await waitFor(() => {
      expect(queuedRowEl("drop me")).toBeTruthy();
    });
    fireEvent.click(screen.getByRole("button", { name: "Remove queued message" }));
    await waitFor(() => {
      expect(queueDeleteMock).toHaveBeenCalledWith({
        session_id: "s-1",
        queued_id: "q-1",
      });
    });
    // No server frame announces a delete — the row drops from local
    // state on mutation success (N-01); the invalidate keeps queue_list
    // honest for any later card.
    await waitFor(() => {
      expect(queuedRowEl("drop me")).toBeNull();
    });
  });

  it("foreign terminal{error} raises the notice; Retry reseeds", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
      expect(sessionGetMock).toHaveBeenCalledTimes(1);
    });
    emitFrame({ type: "append", message_id: "m-for", seq: 1, role: "assistant", delta: "hi" });
    emitFrame({ type: "terminal", message_id: "m-for", result: "error" });
    await waitFor(() => {
      expect(
        screen.getByText(/The provider stopped mid-reply — your messages are saved\./),
      ).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(sessionGetMock).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(screen.queryByText(/The provider stopped mid-reply/)).not.toBeInTheDocument();
    });
  });

  it("foreign terminal{aborted} says 'Reply stopped.'", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    emitFrame({ type: "append", message_id: "m-for", seq: 1, role: "assistant", delta: "hi" });
    emitFrame({ type: "terminal", message_id: "m-for", result: "aborted" });
    await waitFor(() => {
      expect(screen.getByText("Reply stopped.")).toBeInTheDocument();
    });
  });

  it("suppresses a stale reseed landing when calls overlap", async () => {
    // Every snapshot GET parks in `gates` — the test controls which
    // reseed's read completes first, so an older call can resolve last.
    const gates: Array<(r: Response) => void> = [];
    mountWorkspace((url) =>
      url.endsWith("/chat/stream")
        ? new Promise<Response>((res) => gates.push(res))
        : Promise.resolve(sseResponse(defaultPost)),
    );
    await waitFor(() => expect(gates.length).toBe(1));
    gates[0](sseResponse([snapshotFrame()]));
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });

    // A later snapshot (arm 2) + foreign terminal (arm 3) fire two
    // reseeds in one synchronous pass — both GETs park in flight.
    emitFrame(snapshotFrame());
    emitFrame({ type: "terminal", message_id: "m-for", result: "done" });
    await waitFor(() => expect(gates.length).toBe(3));

    // The NEWER call (gates[2]) lands first with fresh data...
    gates[2](
      sseResponse([
        snapshotFrame({
          messages: [{ ...seedMessages[0], id: "m-n", content: "fresh seed" }],
        }),
      ]),
    );
    await waitFor(() => {
      expect(screen.getByText("fresh seed")).toBeInTheDocument();
    });

    // ...then the OLDER call (gates[1]) resolves — its landing must be
    // suppressed or the fresh seed remounts back to the stale one.
    gates[1](
      sseResponse([
        snapshotFrame({
          messages: [{ ...seedMessages[0], id: "m-o", content: "stale seed" }],
        }),
      ]),
    );
    // Give the suppressed completion its window to (not) land, then
    // assert the DOM still reflects the newer seed.
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText("stale seed")).not.toBeInTheDocument();
    expect(screen.getByText("fresh seed")).toBeInTheDocument();
  });

  it("budget_exceeded send denial names the envs + docs link", async () => {
    mountWorkspace((url) =>
      Promise.resolve(
        url.endsWith("/chat/stream")
          ? sseResponse([snapshotFrame()])
          : new Response(
              JSON.stringify({
                error: { code: "budget_exceeded", message: "daily cap reached" },
              }),
              { status: 429 },
            ),
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "over budget" },
    });
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(screen.getByText(/Daily budget reached — send paused/)).toBeInTheDocument();
    });
    const banner = screen.getByText(/Daily budget reached — send paused/);
    expect(banner.textContent).toContain("OXIDEAN_OXPILOT_USER_BUDGET");
    expect(banner.textContent).toContain("OXIDEAN_OXPILOT_INSTANCE_BUDGET");
    expect(banner.textContent).toContain("docs/CONFIGURATION.md");
    expect(banner.textContent).toContain("budget_exceeded");
  });

  it("mid-session oxpilot.disabled swaps to the flag-off surface", async () => {
    mountWorkspace((url) =>
      Promise.resolve(
        url.endsWith("/chat/stream")
          ? sseResponse([snapshotFrame()])
          : new Response(
              JSON.stringify({
                error: { code: "oxpilot.disabled", message: "flag off" },
              }),
              { status: 403 },
            ),
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "too late" },
    });
    fireEvent.submit(composer);
    await waitFor(() => {
      expect(screen.getByText(/Oxpilot is not enabled/)).toBeInTheDocument();
    });
    expect(screen.queryByTestId("oxpilot-composer")).not.toBeInTheDocument();
    expect(screen.getByText("All sessions").closest("a")?.getAttribute("href")).toBe("/oxpilot");
  });

  it("[Stop] calls turn_cancel on a live own turn", async () => {
    // A POST stream that opens then never closes — chat stays busy.
    const hanging = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(
          new TextEncoder().encode(
            `data: ${JSON.stringify({ type: "append", message_id: "m-own", seq: 1, role: "assistant", delta: "…" })}\n\n`,
          ),
        );
      },
    });
    mountWorkspace((url) =>
      Promise.resolve(
        url.endsWith("/chat/stream")
          ? sseResponse([snapshotFrame()])
          : new Response(hanging, { status: 200 }),
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const composer = screen.getByTestId("oxpilot-composer");
    fireEvent.input(composer.querySelector("textarea") as Element, {
      target: { value: "long turn" },
    });
    fireEvent.submit(composer);
    const stop = await screen.findByRole("button", { name: "Stop" });
    fireEvent.click(stop);
    await waitFor(() => {
      expect(turnCancelMock).toHaveBeenCalledWith({ session_id: "s-1" });
    });
    // `{aborted:false}`-style quiet path — no typed banner on success.
    expect(screen.queryByText(/turn_cancel/)).not.toBeInTheDocument();
  });

  it("a CLOSED stream probes once: not_found swaps to SessionNotFound", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
      expect(sessionGetMock).toHaveBeenCalledTimes(1);
    });
    sessionGetMock.mockResolvedValue({
      ok: false,
      error: { code: "not_found", message: "gone" },
    });
    const src = lastSource();
    src.readyState = 2; // EventSource.CLOSED — the HTTP-denial class (REL-3).
    src.error();
    await waitFor(() => {
      expect(screen.getByText("Session not found")).toBeInTheDocument();
    });
    // Exactly ONE sessionGet probe names the denial — never a retry loop.
    expect(sessionGetMock).toHaveBeenCalledTimes(2);
  });

  it("a CLOSED stream with an unrelated probe code shows the retryable banner", async () => {
    mountWorkspaceOk();
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const sourcesBefore = liveSources.length;
    const src = lastSource();
    src.readyState = 2;
    src.error();
    await waitFor(() => {
      expect(screen.getByText(/The live stream ended \(stream_error\)/)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    // The reseed remounts the keyed surface — the fatal banner clears and
    // a fresh EventSource attaches (the one-probe spend stays spent).
    await waitFor(() => {
      expect(screen.queryByText(/The live stream ended \(stream_error\)/)).not.toBeInTheDocument();
    });
    await waitFor(() => {
      expect(liveSources.length).toBeGreaterThan(sourcesBefore);
    });
  });

  it("rail collapse toggles aria-expanded; footer chips carry target name + status", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [session] } });
      targetListMock.mockResolvedValue({
        ok: true,
        data: {
          targets: [
            makeTarget(),
            makeTarget({ id: "t-2", label: "nas-box", lifecycle: "pending_consent" }),
          ],
        },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const toggle = screen.getByTestId("oxpilot-rail-toggle");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    // The target chips arrive with the rail's target_list read.
    await waitFor(() => {
      expect(screen.getByText("desk-box")).toBeInTheDocument();
    });
    expect(screen.getByText("nas-box")).toBeInTheDocument();
    expect(screen.getByText("needs consent")).toBeInTheDocument();
    expect(screen.getByText("●2 targets")).toBeInTheDocument();
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(toggle.getAttribute("aria-expanded")).toBe("false");
    });
    expect(screen.queryByText("●2 targets")).not.toBeInTheDocument();
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(toggle.getAttribute("aria-expanded")).toBe("true");
    });
  });

  it("context drawer shows repo, latest diff, and the bound target", async () => {
    mountWorkspaceOk([snapshotFrame()], () => {
      sessionGetMock.mockResolvedValue({
        ok: true,
        data: { ...session, computer_use_target_id: "t-1", goal: "ship it" },
      });
      diffListMock.mockResolvedValue({ ok: true, data: { diffs: [makeDiff()] } });
      diffGetMock.mockResolvedValue({ ok: true, data: makeDiff() });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [makeTarget()] } });
    });
    await waitFor(() => {
      expect(screen.getByText("seed message")).toBeInTheDocument();
    });
    const toggle = screen.getByTestId("oxpilot-context-toggle");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-drawer")).toBeInTheDocument();
    });
    const drawer = screen.getByTestId("oxpilot-drawer");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(drawer).toHaveTextContent("ada/proj · main");
    expect(drawer).toHaveTextContent("ship it");
    expect(drawer).toHaveTextContent(/tidy tests — 1 file on main/);
    expect(drawer).toHaveTextContent("desk-box");
    expect(drawer).toHaveTextContent("connected");
    expect(drawer).toHaveTextContent("Granted: exec_shell");
    // Esc inside closes and hands focus back to the toggle (BR10.2).
    fireEvent.keyDown(drawer, { key: "Escape" });
    await waitFor(() => {
      expect(toggle.getAttribute("aria-expanded")).toBe("false");
    });
  });
});

describe("/oxpilot index — sessions, targets, dialogs", () => {
  function mountIndex(configure?: () => void) {
    window.history.pushState({}, "", "/oxpilot");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    // Per-test overrides land after the enabledStatus defaults, before render.
    configure?.();
    repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [repo] } });
    renderWithQueryClient(OxpilotPage);
  }

  async function openNewSession() {
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-new-session-open")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("oxpilot-new-session-open"));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
  }

  async function openConnect() {
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-connect-open")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("oxpilot-connect-open"));
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-connect")).toBeInTheDocument();
    });
  }

  /**
   * Drive a Base UI checkbox by its root's aria-label. The root span's
   * onClick forwards a synthesized click to the hidden input — but the
   * connect dialog wraps each checkbox in a `<label>`, and happy-dom runs
   * label activation mid-bubble (real browsers wait for dispatch to end
   * and skip it once `preventDefault` lands). Under happy-dom a span click
   * therefore toggles the input twice — a no-op. Clicking the hidden input
   * directly takes the same `onChange` → `onCheckedChange` path exactly once.
   */
  function checkboxInput(ariaLabel: string): Element {
    const root = document.querySelector(`[aria-label="${ariaLabel}"]`);
    const input = root?.parentElement?.querySelector('input[type="checkbox"]');
    if (!input) throw new Error(`no hidden checkbox input for ${ariaLabel}`);
    return input;
  }

  /** Base UI Select pick — pointerDown arms mouse selection before click. */
  async function pickOption(trigger: Element, optionName: string | RegExp) {
    fireEvent.click(trigger);
    const option = await screen.findByRole("option", { name: optionName });
    fireEvent.pointerDown(option, { pointerType: "mouse" });
    fireEvent.click(option);
  }

  it("session rows carry title, repo·branch, status badge, relative time; delete rides the confirm", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({
        ok: true,
        data: {
          sessions: [
            { ...session, id: "s-2", title: "newest first", status: "active" },
            { ...session, id: "s-1", title: "tidy the tests", status: "active" },
          ],
        },
      });
      sessionDeleteMock.mockResolvedValue({ ok: true, data: { deleted: true } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-session-list")).toBeInTheDocument();
    });
    const rows = screen.getAllByTestId("oxpilot-session-row");
    // Rows render server order — newest first (the list is never re-sorted).
    expect(rows[0]).toHaveTextContent("newest first");
    expect(rows[1]).toHaveTextContent("tidy the tests");
    expect(rows[1]).toHaveTextContent("ada/proj · main");
    expect(rows[1]).toHaveTextContent("active");
    // The relative-time span renders text, never a bare timestamp attr.
    expect(rows[1]?.querySelector("span.text-\\[12px\\]")?.textContent ?? "").toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Delete session tidy the tests" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Delete session “tidy the tests”?");
    // Cancel-first DOM order — initial focus lands on it (M5 pin).
    await waitFor(() => {
      expect(document.activeElement?.textContent).toBe("Cancel");
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete session" }));
    await waitFor(() => {
      expect(sessionDeleteMock).toHaveBeenCalledWith({ session_id: "s-1" });
    });
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
  });

  it("session list error renders the section-scoped notice + Retry", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({
        ok: false,
        error: { code: "forbidden", message: "nope" },
      });
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(/forbidden: nope/);
    });
    sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.getByText("No sessions yet")).toBeInTheDocument();
    });
  });

  it("empty targets render the CTA which opens the connect dialog", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
    });
    await waitFor(() => {
      expect(screen.getByText("No systems connected")).toBeInTheDocument();
    });
    const section = screen.getByTestId("oxpilot-targets");
    // Header + EmptyState both carry the same "Connect a system" affordance.
    const ctas = [...section.querySelectorAll("button")].filter(
      (b) => b.textContent?.trim() === "Connect a system",
    );
    expect(ctas.length).toBeGreaterThanOrEqual(2);
    fireEvent.click(ctas[ctas.length - 1] as Element);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    expect(screen.getByText(/Step 1 of 2/)).toBeInTheDocument();
  });

  it("a target-list failure renders the section-scoped notice", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({
        ok: false,
        error: { code: "forbidden", message: "targets denied" },
      });
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(/forbidden: targets denied/);
    });
    // Sessions stay mounted — the error is scoped to the targets section.
    expect(screen.getByText("Sessions")).toBeInTheDocument();
  });

  it("lifecycle → text labels incl. needs consent + caps badges", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({
        ok: true,
        data: {
          targets: [
            makeTarget({ id: "t-c", label: "conn-box", lifecycle: "connected" }),
            makeTarget({ id: "t-p", label: "pend-box", lifecycle: "pending_consent" }),
            makeTarget({ id: "t-d", label: "off-box", lifecycle: "disconnected" }),
            makeTarget({ id: "t-r", label: "dead-box", lifecycle: "revoked" }),
          ],
        },
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    const rows = screen.getAllByTestId("oxpilot-target-row");
    expect(rows).toHaveLength(4);
    const label = (i: number) => rows[i]?.querySelector('[role="status"]')?.textContent;
    expect(label(0)).toBe("connected·idle");
    expect(label(1)).toBe("needs consent");
    expect(label(2)).toBe("offline");
    expect(label(3)).toBe("revoked");
    // Capability badges render as text (BR10.3).
    expect(rows[0]).toHaveTextContent("exec_shell");
    // A revoked row offers no manage/revoke affordances.
    expect(rows[3]?.querySelectorAll("button")).toHaveLength(0);
  });

  it("[Connect] on a needs-consent row opens the expander → grantList{target_id} → Allow/Deny", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      const pending = makeTarget({ id: "t-p", label: "pend-box", lifecycle: "pending_consent" });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [pending] } });
      grantListMock.mockImplementation((input: unknown) => {
        const f = input as { target_id?: string; session_id?: string };
        if (f.target_id === "t-p") {
          return Promise.resolve({
            ok: true,
            data: {
              grants: [
                makeGrant({
                  id: "g-c",
                  kind: "connect",
                  capability: "exec_shell",
                  session_id: null,
                }),
                makeGrant({ id: "g-done", kind: "connect", state: "denied", session_id: null }),
              ],
            },
          });
        }
        return Promise.resolve({ ok: true, data: { grants: [] } });
      });
      grantRespondMock.mockResolvedValue({ ok: true, data: { state: "allowed" } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect pend-box" }));
    await waitFor(() => {
      expect(grantListMock).toHaveBeenCalledWith({ target_id: "t-p" });
    });
    await waitFor(() => {
      expect(screen.getByText(/Consent to connect pend-box \(exec_shell\)/)).toBeInTheDocument();
    });
    // Only the pending connect grant renders an Allow/Deny pair.
    expect(screen.getAllByRole("button", { name: "Allow pend-box to connect" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Allow pend-box to connect" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-c", allow: true });
    });
  });

  it("Deny responds allow:false on the pending connect grant", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      const pending = makeTarget({ id: "t-p", label: "pend-box", lifecycle: "pending_consent" });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [pending] } });
      grantListMock.mockImplementation((input: unknown) => {
        const f = input as { target_id?: string };
        if (f.target_id === "t-p") {
          return Promise.resolve({
            ok: true,
            data: {
              grants: [makeGrant({ id: "g-c", kind: "connect", session_id: null })],
            },
          });
        }
        return Promise.resolve({ ok: true, data: { grants: [] } });
      });
      grantRespondMock.mockResolvedValue({ ok: true, data: { state: "denied" } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Manage pend-box" }));
    await waitFor(() => {
      expect(screen.getByText(/Consent to connect pend-box/)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Deny pend-box connecting" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-c", allow: false });
    });
  });

  it("Manage expander renders the empty pending state", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [makeTarget()] } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    const manage = screen.getByRole("button", { name: "Manage desk-box" });
    expect(manage.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(manage);
    await waitFor(() => {
      expect(screen.getByText("No pending consent requests.")).toBeInTheDocument();
    });
    expect(manage.getAttribute("aria-expanded")).toBe("true");
    expect(grantListMock).toHaveBeenCalledWith({ target_id: "t-1" });
    expect(screen.getByText("Granted capabilities: exec_shell")).toBeInTheDocument();
  });

  it("[Disconnect] rides the shared confirm → target_disconnect", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [makeTarget()] } });
      targetDisconnectMock.mockResolvedValue({ ok: true, data: { ok: true } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Manage desk-box" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Disconnect desk-box" })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Disconnect desk-box" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Disconnect desk-box?");
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => {
      expect(targetDisconnectMock).toHaveBeenCalledWith({ target_id: "t-1" });
    });
    // The row owns the whole confirm+proc ride — post-success the
    // parent's callback refreshes + toasts only; NO second confirm and
    // NO second proc call (N-02).
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(targetDisconnectMock).toHaveBeenCalledTimes(1);
  });

  it("[Revoke] rides the shared confirm → target_revoke", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({ ok: true, data: { targets: [makeTarget()] } });
      targetRevokeMock.mockResolvedValue({ ok: true, data: { ok: true } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-target-list")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Revoke desk-box" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Revoke desk-box?");
    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    await waitFor(() => {
      expect(targetRevokeMock).toHaveBeenCalledWith({ target_id: "t-1" });
    });
    // Same single-ride contract as disconnect — the row owns it end to
    // end; no phantom route-level confirm re-fires the proc (N-02).
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(targetRevokeMock).toHaveBeenCalledTimes(1);
  });

  it("[+ New session] opens; submit refuses empty repo; create rides onCreated", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      sessionCreateMock.mockResolvedValue({
        ok: true,
        data: { ...session, id: "s-new" },
      });
    });
    await waitFor(() => {
      expect(screen.getByText("No sessions yet")).toBeInTheDocument();
    });
    await openNewSession();
    const dialog = screen.getByTestId("oxpilot-new-session");
    // Submit stays disabled until a repo is picked (required Select).
    const submitBtn = screen.getByRole("button", { name: "Start session" });
    expect(submitBtn).toBeDisabled();
    // A bare form submit still refuses empty (defense-in-depth message).
    fireEvent.submit(dialog.querySelector("form") as Element);
    await waitFor(() => {
      expect(screen.getByText("Pick a repository for the session.")).toBeInTheDocument();
    });
    expect(sessionCreateMock).not.toHaveBeenCalled();

    await pickOption(screen.getByTestId("oxpilot-repo-trigger"), "ada/proj");
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-repo-trigger")).toHaveTextContent("ada/proj");
    });
    // repo.refs fires for the picked repo — branch defaults to default_branch.
    await waitFor(() => {
      expect(repoRefsMock).toHaveBeenCalledWith({ owner: "ada", name: "proj" });
    });
    fireEvent.input(dialog.querySelector("#oxpilot-new-goal") as Element, {
      target: { value: "tidy tests" },
    });
    fireEvent.submit(dialog.querySelector("form") as Element);
    await waitFor(() => {
      expect(sessionCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({
          repo_owner: "ada",
          repo_name: "proj",
          branch: "main",
          goal: "tidy tests",
        }),
      );
    });
    const req = sessionCreateMock.mock.calls[0]![0] as Record<string, unknown>;
    expect(req.target_id).toBeUndefined();
    await waitFor(() => {
      expect(window.location.pathname).toBe("/oxpilot/s-new");
    });
  });

  it("target_id is sent only on a connected pick; non-connected rows never render", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      targetListMock.mockResolvedValue({
        ok: true,
        data: {
          targets: [
            makeTarget({ id: "t-conn", label: "conn-box", lifecycle: "connected" }),
            makeTarget({ id: "t-pend", label: "pend-box", lifecycle: "pending_consent" }),
            makeTarget({ id: "t-disc", label: "off-box", lifecycle: "disconnected" }),
          ],
        },
      });
      sessionCreateMock.mockResolvedValue({ ok: true, data: { ...session, id: "s-t" } });
    });
    await waitFor(() => {
      expect(screen.getByTestId("oxpilot-new-session-open")).toBeInTheDocument();
    });
    await openNewSession();
    await pickOption(screen.getByTestId("oxpilot-repo-trigger"), "ada/proj");
    // Only "No target" + the connected lifecycle row are offered.
    const group = screen.getByRole("radiogroup", { name: "Computer-use target" });
    expect(group).toHaveTextContent("No target");
    expect(group).toHaveTextContent("conn-box");
    expect(group).not.toHaveTextContent("pend-box");
    expect(group).not.toHaveTextContent("off-box");

    fireEvent.click(group.querySelectorAll('[role="radio"]')[1] as Element);
    await waitFor(() => {
      expect(group.querySelectorAll('[role="radio"]')[1]?.getAttribute("aria-checked")).toBe(
        "true",
      );
    });
    const dialog = screen.getByTestId("oxpilot-new-session");
    fireEvent.submit(dialog.querySelector("form") as Element);
    await waitFor(() => {
      expect(sessionCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({ target_id: "t-conn" }),
      );
    });
  });

  it("Repository default branch pick omits branch from the payload", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      repoRefsMock.mockResolvedValue({
        ok: true,
        data: {
          refs: [
            { name: "refs/heads/main", oid: "o1" },
            { name: "refs/heads/dev", oid: "o2" },
            { name: "refs/tags/v1", oid: "o3" },
          ],
        },
      });
      sessionCreateMock.mockResolvedValue({ ok: true, data: { ...session, id: "s-b" } });
    });
    await openNewSession();
    await pickOption(screen.getByTestId("oxpilot-repo-trigger"), "ada/proj");
    // The repo pick auto-selects its default_branch. The Select trigger
    // carries role=combobox, not button — query by its input id.
    const branchTrigger = document.getElementById("oxpilot-branch") as Element;
    await waitFor(() => {
      expect(branchTrigger).toHaveTextContent("main");
    });
    await pickOption(branchTrigger, /Repository default \(main\)/);
    const dialog = screen.getByTestId("oxpilot-new-session");
    fireEvent.submit(dialog.querySelector("form") as Element);
    await waitFor(() => {
      expect(sessionCreateMock).toHaveBeenCalled();
    });
    const req = sessionCreateMock.mock.calls[0]![0] as Record<string, unknown>;
    expect(req.branch).toBeUndefined();
    expect(req.repo_owner).toBe("ada");
    expect(req.repo_name).toBe("proj");
  });

  it("an explicit branch pick + a create error renders inline and keeps the draft", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      repoRefsMock.mockResolvedValue({
        ok: true,
        data: {
          refs: [
            { name: "refs/heads/main", oid: "o1" },
            { name: "refs/heads/dev", oid: "o2" },
          ],
        },
      });
      sessionCreateMock.mockResolvedValue({
        ok: false,
        error: { code: "bad_input", message: "branch refused" },
      });
    });
    await openNewSession();
    await pickOption(screen.getByTestId("oxpilot-repo-trigger"), "ada/proj");
    const branchTrigger = document.getElementById("oxpilot-branch") as Element;
    await waitFor(() => {
      expect(branchTrigger).toHaveTextContent("main");
    });
    await pickOption(branchTrigger, "dev");
    const dialog = screen.getByTestId("oxpilot-new-session");
    fireEvent.submit(dialog.querySelector("form") as Element);
    await waitFor(() => {
      expect(sessionCreateMock).toHaveBeenCalledWith(expect.objectContaining({ branch: "dev" }));
    });
    await waitFor(() => {
      expect(screen.getByText(/bad_input: branch refused/)).toBeInTheDocument();
    });
    // Draft preserved — the dialog stays open with the picks intact.
    expect(screen.getByTestId("oxpilot-repo-trigger")).toHaveTextContent("ada/proj");
    expect(branchTrigger).toHaveTextContent("dev");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("connect dialog: step-1 gating → pairing → step-2 command → explicit consent → Done", async () => {
    const expiry = new Date(Date.now() + 60_000).toISOString();
    let connected = false;
    let resolveRespond: ((v: unknown) => void) | undefined;
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      pairingCreateMock.mockResolvedValue({
        ok: true,
        data: { pairing_id: "pair-99", pairing_code: "PAIR-42-XYZ", expires_at: expiry },
      });
      targetListMock.mockImplementation(() =>
        Promise.resolve({
          ok: true,
          data: {
            targets: [
              makeTarget({
                id: "t-99",
                label: "desk-box",
                lifecycle: connected ? "connected" : "pending_consent",
                pairing_id: "pair-99",
                granted_caps: ["exec_shell"],
              }),
            ],
          },
        }),
      );
      grantListMock.mockImplementation((input: unknown) => {
        const f = input as { target_id?: string };
        if (f.target_id === "t-99") {
          return Promise.resolve({
            ok: true,
            data: {
              grants: [makeGrant({ id: "g-c", kind: "connect", session_id: null })],
            },
          });
        }
        return Promise.resolve({ ok: true, data: { grants: [] } });
      });
      grantRespondMock.mockImplementation(
        // Held pending until the test releases it — keeps the "waiting"
        // phase observable instead of racing consent → success.
        () =>
          new Promise((res) => {
            resolveRespond = res as (v: unknown) => void;
          }),
      );
    });
    await openConnect();

    const continueBtn = () => screen.getByRole("button", { name: "Continue" });
    expect(continueBtn()).toBeDisabled();
    // Name alone is not enough.
    fireEvent.input(document.getElementById("oxpilot-target-name") as Element, {
      target: { value: "desk-box" },
    });
    expect(continueBtn()).toBeDisabled();
    // A capability alone is not enough — acknowledgement is required.
    fireEvent.click(checkboxInput("shell.exec"));
    expect(continueBtn()).toBeDisabled();
    fireEvent.click(checkboxInput("I understand the granted capabilities"));
    // Octane flushes the checked-state re-render after the click — the
    // enabled assertion has to ride the wait.
    await waitFor(() => {
      expect(continueBtn()).not.toBeDisabled();
    });

    fireEvent.click(continueBtn());
    await waitFor(() => {
      expect(pairingCreateMock).toHaveBeenCalledWith({
        target_name: "desk-box",
        caps: ["exec_shell"],
      });
    });
    // Step 2 renders ONLY `ox pilot pair <code>` — the code lives in the
    // CopyCommand input's VALUE, never in rendered text (SEC-14). The
    // input and its copy button share the aria-label, so query the input.
    // The granted caps ride the command so the paired connector is
    // scoped to exactly what the dialog showed (SEC-14 follow-through).
    await waitFor(() => {
      const input = document.querySelector(
        'input[aria-label="Pairing command"]',
      ) as HTMLInputElement | null;
      expect(input?.value).toBe("ox pilot pair PAIR-42-XYZ --caps exec_shell");
    });
    expect(screen.getByText(/Waiting for the connector to check in/)).toBeInTheDocument();
    expect(document.body.textContent ?? "").not.toContain("PAIR-42-XYZ");

    // Poll matches pairing_id → pending_consent → the target is surfaced
    // for review with its identity + requested capabilities — pairing
    // alone never answers the grant.
    await waitFor(() => {
      expect(screen.getByText("Review connection request")).toBeInTheDocument();
    });
    expect(screen.getByText(/Target: desk-box/)).toBeInTheDocument();
    expect(screen.getByText(/Requested capabilities: exec_shell/)).toBeInTheDocument();
    expect(grantRespondMock).not.toHaveBeenCalled();
    // Explicit Allow → grantList{target_id} → grantRespond{allow:true}.
    fireEvent.click(screen.getByRole("button", { name: "Allow connection" }));
    await waitFor(() => {
      expect(grantListMock).toHaveBeenCalledWith({ target_id: "t-99" });
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-c", allow: true });
    });
    // Release the held response — the next targetsQ refetch reports the
    // target connected, ending the wait on the success copy.
    connected = true;
    resolveRespond?.({ ok: true, data: { state: "allowed" } });
    await waitFor(() => {
      expect(screen.getByText(/Connected — desk-box is ready/)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("connect dialog: Deny answers the connect grant allow:false — never auto-approved", async () => {
    const expiry = new Date(Date.now() + 60_000).toISOString();
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      pairingCreateMock.mockResolvedValue({
        ok: true,
        data: { pairing_id: "pair-98", pairing_code: "PAIR-98-XYZ", expires_at: expiry },
      });
      targetListMock.mockImplementation(() =>
        Promise.resolve({
          ok: true,
          data: {
            targets: [
              makeTarget({
                id: "t-98",
                label: "lab-box",
                lifecycle: "pending_consent",
                pairing_id: "pair-98",
                granted_caps: ["fs_read"],
              }),
            ],
          },
        }),
      );
      let grantCall = 0;
      grantListMock.mockImplementation((input: unknown) => {
        const f = input as { target_id?: string };
        if (f.target_id === "t-98") {
          // The denied grant resolves server-side; the owner read re-mints
          // a FRESH pending grant for the re-review (BR3.2).
          const id = grantCall === 0 ? "g-d" : "g-d2";
          grantCall += 1;
          return Promise.resolve({
            ok: true,
            data: {
              grants: [makeGrant({ id, kind: "connect", session_id: null })],
            },
          });
        }
        return Promise.resolve({ ok: true, data: { grants: [] } });
      });
      grantRespondMock.mockImplementation((input: unknown) => {
        const f = input as { allow?: boolean };
        return Promise.resolve({ ok: true, data: { state: f.allow ? "allowed" : "denied" } });
      });
    });
    await openConnect();
    fireEvent.input(document.getElementById("oxpilot-target-name") as Element, {
      target: { value: "lab-box" },
    });
    fireEvent.click(checkboxInput("fs.read"));
    fireEvent.click(checkboxInput("I understand the granted capabilities"));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Continue" })).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    // Redemption surfaces the review card; nothing is answered until Deny.
    await waitFor(() => {
      expect(screen.getByText("Review connection request")).toBeInTheDocument();
    });
    expect(grantRespondMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-d", allow: false });
    });
    // A decided Deny swaps the card for an explicit denied state — the
    // check-in wait does NOT masquerade as still-pending.
    await waitFor(() => {
      expect(screen.getByText("Connection denied")).toBeInTheDocument();
    });
    expect(screen.queryByText("Review connection request")).not.toBeInTheDocument();
    expect(screen.queryByText(/Waiting for the connector to check in/)).not.toBeInTheDocument();
    // "Review again" re-opens the card — the owner read re-mints a
    // pending grant (g-d2) — and Allow answers THAT grant, not the
    // already-resolved g-d.
    fireEvent.click(screen.getByRole("button", { name: "Review again" }));
    await waitFor(() => {
      expect(screen.getByText("Review connection request")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Allow connection" }));
    await waitFor(() => {
      expect(grantRespondMock).toHaveBeenCalledWith({ grant_id: "g-d2", allow: true });
    });
    expect(grantRespondMock).not.toHaveBeenCalledWith({ grant_id: "g-d", allow: true });
  });

  it("connect dialog: denied consent outlives the pairing-code expiry", async () => {
    // Connector redeems near the code's TTL — expiry must not erase a live
    // consent review; the target stays pending_consent server-side.
    // expires_at is minted at CALL time and every target-list response is
    // gated on the pairing call starting, so redemption ordering is
    // deterministic — repeat polls can't leave earlier requests pending.
    const targetPayload = {
      ok: true,
      data: {
        targets: [
          makeTarget({
            id: "t-97",
            label: "lab-box",
            lifecycle: "pending_consent",
            pairing_id: "pair-97",
            granted_caps: ["fs_read"],
          }),
        ],
      },
    };
    let markPairingStarted: (() => void) | undefined;
    const pairingStarted = new Promise<void>((resolve) => {
      markPairingStarted = resolve;
    });
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      pairingCreateMock.mockImplementation(() => {
        markPairingStarted?.();
        return Promise.resolve({
          ok: true,
          data: {
            pairing_id: "pair-97",
            pairing_code: "PAIR-97-XYZ",
            expires_at: new Date(Date.now() + 400).toISOString(),
          },
        });
      });
      targetListMock.mockImplementation(async () => {
        await pairingStarted;
        return targetPayload;
      });
      grantListMock.mockImplementation((input: unknown) => {
        const f = input as { target_id?: string };
        if (f.target_id === "t-97") {
          return Promise.resolve({
            ok: true,
            data: {
              grants: [makeGrant({ id: "g-x", kind: "connect", session_id: null })],
            },
          });
        }
        return Promise.resolve({ ok: true, data: { grants: [] } });
      });
      grantRespondMock.mockResolvedValue({ ok: true, data: { state: "denied" } });
    });
    await openConnect();
    fireEvent.input(document.getElementById("oxpilot-target-name") as Element, {
      target: { value: "lab-box" },
    });
    fireEvent.click(checkboxInput("fs.read"));
    fireEvent.click(checkboxInput("I understand the granted capabilities"));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Continue" })).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    // Redeem strictly before the 400ms expiry — the gated list resolves as
    // soon as pairingCreate fires.
    await waitFor(() => {
      expect(screen.getByText("Review connection request")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));
    await waitFor(() => {
      expect(screen.getByText("Connection denied")).toBeInTheDocument();
    });
    // Ride past the code's own expiry — the denied review survives.
    await new Promise((r) => setTimeout(r, 600));
    expect(screen.getByText("Connection denied")).toBeInTheDocument();
    expect(
      screen.queryByText(/The pairing code expired before a connector checked in\./),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Review again" }));
    await waitFor(() => {
      expect(screen.getByText("Review connection request")).toBeInTheDocument();
    });
  });

  it("connect dialog: expired pairing shows the timeout + retry affordance", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      pairingCreateMock.mockResolvedValue({
        ok: true,
        data: {
          pairing_id: "pair-old",
          pairing_code: "PAIR-OLD-1",
          expires_at: new Date(Date.now() - 1000).toISOString(),
        },
      });
    });
    await openConnect();
    fireEvent.input(document.getElementById("oxpilot-target-name") as Element, {
      target: { value: "desk-box" },
    });
    fireEvent.click(checkboxInput("fs.read"));
    fireEvent.click(checkboxInput("I understand the granted capabilities"));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Continue" })).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => {
      expect(
        screen.getByText(/The pairing code expired before a connector checked in\./),
      ).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "Get a new code" })).toBeInTheDocument();
    expect(grantRespondMock).not.toHaveBeenCalled();
  });

  it("connect dialog: Esc abandons the wait and resets to step 1", async () => {
    mountIndex(() => {
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      pairingCreateMock.mockResolvedValue({
        ok: true,
        data: {
          pairing_id: "pair-esc",
          pairing_code: "PAIR-ESC-1",
          expires_at: new Date(Date.now() + 60_000).toISOString(),
        },
      });
    });
    await openConnect();
    fireEvent.input(document.getElementById("oxpilot-target-name") as Element, {
      target: { value: "desk-box" },
    });
    fireEvent.click(checkboxInput("fs.write"));
    fireEvent.click(checkboxInput("I understand the granted capabilities"));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Continue" })).not.toBeDisabled();
    });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => {
      expect(document.querySelector('input[aria-label="Pairing command"]')).toBeTruthy();
    });
    // Esc abandons — the pairing code dies with the dialog.
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(grantRespondMock).not.toHaveBeenCalled();
    // Reopening lands back on step 1 with the pairing artifact gone —
    // close() resets the mutations/pairing but keeps the form draft the
    // user typed (name + caps persist, so Continue is already enabled).
    fireEvent.click(screen.getByTestId("oxpilot-connect-open"));
    await waitFor(() => {
      expect(screen.getByText(/Step 1 of 2/)).toBeInTheDocument();
    });
    expect(document.querySelector('input[aria-label="Pairing command"]')).not.toBeInTheDocument();
    expect((document.getElementById("oxpilot-target-name") as HTMLInputElement).value).toBe(
      "desk-box",
    );
  });
});

describe("oxpilot source pins (SEC-2, SEC-14, zero-telemetry)", () => {
  const SRC = join(dirname(fileURLToPath(import.meta.url)), "..");
  const COMPONENT_DIR = join(SRC, "components", "oxpilot");

  function oxpilotSources(): { file: string; src: string }[] {
    const out: { file: string; src: string }[] = [];
    for (const f of readdirSync(COMPONENT_DIR)) {
      if (f.endsWith(".tsrx") || f.endsWith(".ts")) {
        out.push({
          file: `components/oxpilot/${f}`,
          src: readFileSync(join(COMPONENT_DIR, f), "utf8"),
        });
      }
    }
    for (const r of ["oxpilot.tsrx", "oxpilot.$session.tsrx"]) {
      out.push({ file: `routes/${r}`, src: readFileSync(join(SRC, "routes", r), "utf8") });
    }
    for (const l of [
      "oxpilot-stream.ts",
      "oxpilot-chat.tsrx",
      "oxpilot-queries.ts",
      // The `contentHtml` producer — the escaped pipeline the one
      // allowlisted sink consumes; the pin must scan it too (N-06).
      "highlight-diff.ts",
    ]) {
      out.push({ file: `lib/${l}`, src: readFileSync(join(SRC, "lib", l), "utf8") });
    }
    return out;
  }

  it("SEC-2: dangerouslySetInnerHTML only in diff-card; zero raw sinks", () => {
    const allowed = new Set(["components/oxpilot/diff-card.tsrx"]);
    const rawSink =
      /(?<![\w$])(innerHTML|outerHTML|insertAdjacentHTML)\b|document\.write|createElement\(\s*["'`]script|\beval\s*\(|new Function\s*\(/;
    for (const { file, src } of oxpilotSources()) {
      const stripped = src.replaceAll("dangerouslySetInnerHTML", "");
      if (/dangerouslySetInnerHTML/.test(src)) {
        expect(allowed.has(file), `${file} carries dangerouslySetInnerHTML`).toBe(true);
      }
      expect(rawSink.test(stripped), `${file} carries a raw HTML/eval sink`).toBe(false);
    }
  });

  it("SEC-14: pairing_code/pairingCode never leaves connect-dialog.tsrx", () => {
    for (const { file, src } of oxpilotSources()) {
      const has = /pairing_code|pairingCode/.test(src);
      expect(has, `${file} references the pairing code`).toBe(
        file === "components/oxpilot/connect-dialog.tsrx",
      );
    }
    // The verbatim pair command is the only surface it ever reaches.
    const connect = readFileSync(join(COMPONENT_DIR, "connect-dialog.tsrx"), "utf8");
    expect(connect).toMatch(/ox pilot pair \$\{pairing\.pairing_code\}/);
  });

  it("zero telemetry: no metrics/beacons/storage/console in the oxpilot scope", () => {
    const telemetry =
      /performance\.(mark|measure)|PerformanceObserver|sendBeacon|localStorage|sessionStorage|console\./;
    for (const { file, src } of oxpilotSources()) {
      expect(telemetry.test(src), `${file} carries a telemetry/storage/console call`).toBe(false);
    }
  });

  it("stream fetches pin the encodeURIComponent session paths", () => {
    const stream = readFileSync(join(SRC, "lib", "oxpilot-stream.ts"), "utf8");
    expect(stream).toMatch(
      /fetch\(`\/api\/oxpilot\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/chat`/,
    );
    expect(stream).toMatch(
      /fetch\(`\/api\/oxpilot\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/chat\/stream`/,
    );
    const chat = readFileSync(join(SRC, "lib", "oxpilot-chat.tsrx"), "utf8");
    expect(chat).toMatch(/\/api\/oxpilot\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/chat/);
    expect(stream).toMatch(/new EventSource\(/);
  });

  it("proc pins: session_create target_id, grant_list session_id, queue_delete", () => {
    // Session → target binding rides an optional `target_id` on create.
    const dialog = readFileSync(join(COMPONENT_DIR, "new-session-dialog.tsrx"), "utf8");
    expect(dialog).toMatch(/target_id/);
    // The session-scoped consent read carries the flagged `session_id` filter.
    const queries = readFileSync(join(SRC, "lib", "oxpilot-queries.ts"), "utf8");
    expect(queries).toMatch(/grantList[\s\S]*session_id|session_id[\s\S]*grantList/);
    // The inline queued-message cancel rides `oxpilot.queue_delete`.
    expect(queries).toMatch(/queueDelete/);
  });
});
