import { createStore, useStore } from "@octanejs/zustand";
import { createContext, useContext } from "octane";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";

export type RepoLayoutStatus = "ok" | "not_found" | "error";

type RepoLayoutLoaderData = {
  owner: string;
  repoName: string;
  status: RepoLayoutStatus;
  repo: RepoPublic | null;
  me: UserPublic | null;
  message: string;
  /** Browser-facing origin for clone URLs (SSR’d). */
  publicOrigin: string;
  /** Advertised Git SSH host (SSR’d from OXIDEAN_SSH_HOST / origin). */
  sshHost: string;
  /** Advertised Git SSH port (SSR’d from OXIDEAN_SSH_PORT). */
  sshPort: number;
};

type RepoStoreState = {
  owner: string;
  repoName: string;
  status: RepoLayoutStatus;
  repo: RepoPublic | null;
  me: UserPublic | null;
  message: string;
  publicOrigin: string;
  sshHost: string;
  sshPort: number;
  setRepo: (repo: RepoPublic) => void;
  setMe: (me: UserPublic | null) => void;
};

type RepoStoreApi = ReturnType<typeof createRepoStore>;

export function createRepoStore(init: {
  owner: string;
  repoName: string;
  status: RepoLayoutStatus;
  repo: RepoPublic | null;
  me: UserPublic | null;
  message?: string;
  publicOrigin?: string;
  sshHost?: string;
  sshPort?: number;
}) {
  return createStore<RepoStoreState>((set) => ({
    owner: init.owner,
    repoName: init.repoName,
    status: init.status,
    repo: init.repo,
    me: init.me,
    message: init.message ?? "",
    publicOrigin: init.publicOrigin ?? "",
    sshHost: init.sshHost ?? "",
    sshPort: init.sshPort ?? 0,
    setRepo: (repo) => set({ repo, status: "ok" }),
    setMe: (me) => set({ me }),
  }));
}

export const RepoStoreContext = createContext<RepoStoreApi | null>(null);

/** Used when a page is rendered outside the layout (unit tests). */
const fallbackRepoStore = createRepoStore({
  owner: "",
  repoName: "",
  status: "error",
  repo: null,
  me: null,
  publicOrigin: "",
  sshHost: "",
  sshPort: 0,
});

export function useRepoStore<T>(selector: (s: RepoStoreState) => T): T {
  const store = useContext(RepoStoreContext) ?? fallbackRepoStore;
  return useStore(store, selector);
}

/**
 * Layout-data compat for ported leaf routes — was
 * `useLoaderData({ from: "/$owner/$repo" })`. Reads the RepoStoreProvider
 * fields the leaves consume.
 */
export function useRepoLayoutData(): RepoLayoutLoaderData | undefined {
  const owner = useRepoStore((s) => s.owner);
  const repoName = useRepoStore((s) => s.repoName);
  const status = useRepoStore((s) => s.status);
  const repo = useRepoStore((s) => s.repo);
  const me = useRepoStore((s) => s.me);
  const message = useRepoStore((s) => s.message);
  const publicOrigin = useRepoStore((s) => s.publicOrigin);
  const sshHost = useRepoStore((s) => s.sshHost);
  const sshPort = useRepoStore((s) => s.sshPort);
  if (!owner || !repoName) return undefined;
  return { owner, repoName, status, repo, me, message, publicOrigin, sshHost, sshPort };
}
