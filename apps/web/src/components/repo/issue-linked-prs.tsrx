import type { IssueLinkKind, IssueLinkPublic } from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type IssueLinkedPrsPanelProps = {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
};

function stubLabel(link: IssueLinkPublic): string {
  const n = link.target_number != null ? `#${link.target_number}` : "";
  if (link.kind === "pr" || link.kind === "pr_stub") {
    const title = (link.title ?? "").trim();
    const prefix = link.kind === "pr" ? "PR" : "PR stub";
    return title ? `${prefix} ${n}: ${title}` : `${prefix} ${n}`.trim();
  }
  const title = (link.title ?? "").trim();
  return title ? `Issue ${n}: ${title}` : `Issue ${n}`.trim();
}

/** Linked PRs sidebar — stub rows + manual Link issue/PR (ISS-04 / D-ISS-13 / D-ISS-14). */
export function IssueLinkedPrsPanel({
  owner,
  repoName,
  number,
  canWrite,
}: IssueLinkedPrsPanelProps) @{
  const queryClient = useQueryClient();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const form = useForm({
    defaultValues: {
      kind: "pr" as IssueLinkKind,
      targetNumber: "",
      title: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      const n = Number(value.targetNumber.trim());
      if (!Number.isFinite(n) || n < 1) {
        setError("Enter a positive target number.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.issue.links.add({
          owner,
          name: repoName,
          number,
          kind: value.kind,
          targetNumber: n,
          title: value.title.trim() || null,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not add link.");
          setPending(false);
          return;
        }
        form.reset();
        await refresh();
        setPending(false);
      } catch {
        setError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  const linksQ = useQuery({
    queryKey: ["issue", "links", "list", owner, repoName, number] as const,
    queryFn: async () => {
      const res = await apiClient.issue.links.list({
        owner,
        name: repoName,
        number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.links;
    },
    enabled: !!owner && !!repoName && number > 0,
  });

  const links: IssueLinkPublic[] = linksQ.data ?? [];

  async function refresh() {
    await queryClient.invalidateQueries({
      queryKey: ["issue", "links", "list", owner, repoName, number],
    });
  }

  async function removeLink(linkId: string) {
    setError("");
    setPending(true);
    try {
      const res = await apiClient.issue.links.remove({
        owner,
        name: repoName,
        number,
        linkId,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not remove link.");
        setPending(false);
        return;
      }
      await refresh();
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <section aria-labelledby="issue-linked-prs-heading" data-testid="issue-linked-prs">
    <h2 id="issue-linked-prs-heading" className="text-[14px] font-semibold text-foreground">
      Linked PRs
    </h2>
    @if (error) {
      <div className="mt-2">
        <AuthErrorBanner message={error} />
      </div>;
    }
    @if (linksQ.isPending) {
      <p className="mt-1 text-[13px] text-muted-foreground">Loading links…</p>;
    } @else {
      @if (links.length === 0) {
        <p className="mt-1 text-[13px] text-muted-foreground">
          No linked pull requests yet.
        </p>;
      } @else {
        <ul className="mt-2 flex flex-col gap-1.5" data-testid="issue-linked-prs-list">
          @for (const link of links; key link.id) {
            <li className="flex items-start justify-between gap-2 text-[13px] text-foreground">
              @if (
                link.kind === "pr" &&
                link.target_number != null
              ) {
                <AppLink
                  className="text-primary underline-offset-2 hover:underline"
                  href={
                    `/${owner}/${repoName}/pull/${link.target_number}` as string
                  }
                  data-testid={`issue-link-${link.id}`}
                >
                  {stubLabel(link) as string}
                </AppLink>;
              } @else {
                <span data-testid={`issue-link-${link.id}`}>
                  {stubLabel(link) as string}
                </span>;
              }
              @if (canWrite) {
                <button
                  type="button"
                  className="shrink-0 text-[12px] text-muted-foreground hover:text-foreground"
                  disabled={pending}
                  aria-label={`Unlink ${stubLabel(link)}`}
                  onClick={() => removeLink(link.id)}
                >
                  Unlink
                </button>;
              }
            </li>;
          }
        </ul>;
      }
    }
    @if (canWrite) {
      <form
        method="post"
        action="#"
        className="mt-3 flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        <p className="text-[12px] font-medium text-foreground">Link issue/PR</p>
        <form.Field name="kind">
          {(field) => (
            <div className="flex flex-col gap-1">
              <Label htmlFor="issue-link-kind">Kind</Label>
              <select
                id="issue-link-kind"
                className="h-8 rounded-md border border-border bg-background px-2 text-[13px] text-foreground"
                value={field.state.value}
                onChange={(e) =>
                  field.handleChange((e.target as HTMLSelectElement).value as IssueLinkKind)
                }
                disabled={pending}
              >
                <option value="pr">Pull request</option>
                <option value="pr_stub">PR stub (legacy)</option>
                <option value="issue">Issue</option>
              </select>
            </div>
          )}
        </form.Field>
        <form.Field name="targetNumber">
          {(field) => (
            <div className="flex flex-col gap-1">
              <Label htmlFor="issue-link-target">Number</Label>
              <Input
                id="issue-link-target"
                type="number"
                min={1}
                inputMode="numeric"
                value={field.state.value}
                placeholder="42"
                disabled={pending}
                onBlur={field.handleBlur}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
              />
            </div>
          )}
        </form.Field>
        <form.Field name="title">
          {(field) => (
            <div className="flex flex-col gap-1">
              <Label htmlFor="issue-link-title">Title (optional)</Label>
              <Input
                id="issue-link-title"
                type="text"
                value={field.state.value}
                placeholder="Stub title"
                disabled={pending}
                onBlur={field.handleBlur}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
              />
            </div>
          )}
        </form.Field>
        <Button type="submit" size="sm" disabled={pending}>
          Link
        </Button>
      </form>;
    }
  </section>;
}
