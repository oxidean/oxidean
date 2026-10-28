/**
 * `/oxpilot` — index mount, flag-off explanatory state, and the session
 * workspace composer append path over a stubbed SSE POST.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const statusMock = vi.fn();
const sessionListMock = vi.fn();
const sessionCreateMock = vi.fn();
const repoListMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    oxpilot: {
      status: (...args: unknown[]) => statusMock(...args),
      sessionList: (...args: unknown[]) => sessionListMock(...args),
      sessionCreate: (...args: unknown[]) => sessionCreateMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => repoListMineMock(...args),
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
};

function enabledStatus() {
  statusMock.mockResolvedValue({
    ok: true,
    data: { enabled: true, provider: { id: "stub", model: "stub-1" }, reason: null },
  });
}

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

  it("enabled: renders the session list + new-session form", async () => {
    window.history.pushState({}, "", "/oxpilot");
    meMock.mockResolvedValue({ ok: true, data: user });
    enabledStatus();
    sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [session] } });
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
  function stubEventSource() {
    // Snapshot replay over the GET stream — an empty transcript attach.
    class FakeES {
      onmessage: ((ev: { data: string }) => void) | null = null;
      onerror: (() => void) | null = null;
      readyState = 1;
      constructor(public url: string) {
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

  function snapshotFrame() {
    return {
      type: "snapshot",
      session_id: "s-1",
      messages: seedMessages,
      active_turn: null,
      pending_queue: [],
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
        { type: "append", message_id: "m-a1", seq: 1, role: "assistant", delta: "Hello " },
        { type: "append", message_id: "m-a1", seq: 2, role: "assistant", delta: "there" },
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
            JSON.stringify({ error: { code: "turn_active", message: "a turn is live" } }),
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
  });
});
