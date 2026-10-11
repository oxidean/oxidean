import { queryOptions } from "@octanejs/tanstack-query";
import type { RepoPublic } from "@oxidean/api-client";
import { fetchRepoGet } from "@/lib/ssr-repo";

/**
 * `repo.get` for the repo layout — mirrors the old route loader's
 * status/error shape so `RepoLayout` branches stay unchanged.
 */
export function repoLayoutQueryOptions(owner: string, repoName: string) {
  return queryOptions({
    queryKey: ["repo", "layout", owner, repoName],
    queryFn: async (): Promise<{
      status: "ok" | "not_found" | "error";
      repo: RepoPublic | null;
      message: string;
    }> => {
      try {
        const got = await fetchRepoGet({ owner, name: repoName });
        if (!got.ok) {
          if (got.error.code === "repo.not_found") {
            return { status: "not_found", repo: null, message: "" };
          }
          return {
            status: "error",
            repo: null,
            message: got.error.message || "Could not load repository.",
          };
        }
        return { status: "ok", repo: got.data, message: "" };
      } catch {
        return {
          status: "error",
          repo: null,
          message: "Can't reach Oxidean. Check your connection and try again.",
        };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}
