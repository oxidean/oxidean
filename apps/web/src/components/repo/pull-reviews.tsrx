import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { PullReviewPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";

type PullReviewsProps = {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  isAuthor: boolean;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export function PullReviews(props: PullReviewsProps) @{
  const queryClient = useQueryClient();
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [requestUser, setRequestUser] = useState("");

  const reviewsQ = useQuery({
    queryKey: ["pull", "reviews", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.reviews.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const requestsQ = useQuery({
    queryKey: ["pull", "reviewRequests", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.reviewRequests.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const reviews: PullReviewPublic[] = reviewsQ.data?.reviews ?? [];
  const requested: string[] = requestsQ.data?.usernames ?? [];

  async function submit(state: "approved" | "changes_requested" | "commented") {
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.reviews.submit({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      state,
      body: body || null,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Review failed");
      return;
    }
    setBody("");
    await queryClient.invalidateQueries({
      queryKey: ["pull", "reviews", props.owner, props.repoName, props.number],
    });
  }

  async function dismiss(id: string) {
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.reviews.dismiss({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      review_id: id,
      reason: "dismissed",
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Dismiss failed");
      return;
    }
    await queryClient.invalidateQueries({
      queryKey: ["pull", "reviews", props.owner, props.repoName, props.number],
    });
  }

  async function addRequest() {
    if (!requestUser.trim()) return;
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.reviewRequests.add({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      username: requestUser.trim(),
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not request review");
      return;
    }
    setRequestUser("");
    await queryClient.invalidateQueries({
      queryKey: ["pull", "reviewRequests", props.owner, props.repoName, props.number],
    });
  }

  <div className="space-y-4" data-testid="pull-reviews">
    <h3 className="text-sm font-semibold">Reviews</h3>
    @if (reviewsQ.isPending) {
      <p className="text-sm text-muted-foreground">Loading reviews…</p>;
    } @else {
      @if (reviews.length === 0) {
        <p className="text-sm text-muted-foreground">No reviews yet.</p>;
      } @else {
        <ul className="space-y-2">
          @for (const r of reviews; key r.id) {
            <li className="rounded-md border border-border px-3 py-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">
                  {("@" + r.author_username) as string}
                </span>
                <span className="text-muted-foreground">{r.state as string}</span>
                @if (
                  props.canWrite &&
                  r.state !== "dismissed"
                ) {
                  <button
                    type="button"
                    className="text-[13px] text-muted-foreground underline-offset-2 hover:underline"
                    disabled={busy}
                    onClick={() => void dismiss(r.id)}
                  >
                    Dismiss
                  </button>;
                }
              </div>
              @if (r.body) {
                <MarkdownBody
                  markdown={r.body}
                  owner={props.owner}
                  repo={props.repoName}
                  className="mt-2"
                />;
              }
            </li>;
          }
        </ul>;
      }
    }

    @if (props.canWrite) {
      <div className="space-y-2 rounded-md border border-border p-3">
        <MarkdownWritePreview
          value={body}
          onValueChange={setBody}
          placeholder="Review comment (markdown supported)"
          rows={4}
          disabled={busy}
          owner={props.owner}
          repo={props.repoName}
        />
        <div className="flex flex-wrap gap-2">
          @if (!props.isAuthor) {
            <>
              <Button
                type="button"
                disabled={busy}
                onClick={() => void submit("approved")}
              >
                Approve
              </Button>
              <Button
                type="button"
                disabled={busy}
                onClick={() => void submit("changes_requested")}
              >
                Request changes
              </Button>
            </>;
          }
          <Button type="button" disabled={busy} onClick={() => void submit("commented")}>
            Comment
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2 pt-2">
          <input
            className="rounded-md border border-border bg-background px-2 py-1 text-sm"
            value={requestUser}
            placeholder="Request reviewer username"
            onInput={(e) => setRequestUser((e.currentTarget as HTMLInputElement).value)}
          />
          <Button type="button" disabled={busy} onClick={() => void addRequest()}>
            Request review
          </Button>
        </div>
        @if (requested.length > 0) {
          <p className="text-[13px] text-muted-foreground">
            {("Requested: " + requested.join(", ")) as string}
          </p>;
        }
      </div>;
    }
    @if (error) {
      <p className="text-sm text-destructive">{error as string}</p>;
    }
  </div>;
}
