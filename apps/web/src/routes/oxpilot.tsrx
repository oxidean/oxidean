import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { OxpilotSession } from "@oxidean/api-client";
import { useState } from "octane";
import { Bot, Check, ChevronDown, MessageSquarePlus } from "@octanejs/lucide";
import { AppPage } from "@/components/app-page";
import { AppLink } from "@/components/ui/app-link";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import {
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectList,
  SelectPopup,
  SelectPortal,
  SelectPositioner,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { usePageRedirect } from "@/lib/nav.tsrx";
import {
  invalidateOxpilotQueries,
  oxpilotSessionCreate,
  oxpilotSessionListQueryOptions,
  oxpilotStatusQueryOptions,
} from "@/lib/oxpilot-queries";
import { formatRelative } from "@/lib/relative-time";
import { authSessionQueryOptions, repoListMineQueryOptions } from "@/lib/session-queries";

/** `/oxpilot` — session list + new-session form; flag-off explanatory state. */
export function OxpilotPage() @{
  <AppPage>
    <OxpilotContent />
  </AppPage>;
}

function flagOffReason(reason: string | null | undefined): string {
  switch (reason) {
    case "flag_off":
      return "the OXIDEAN_OXPILOT_ENABLED flag is off";
    case "misconfigured":
      return "the provider configuration is incomplete";
    case "no_provider":
      return "no provider is configured";
    default:
      return "the feature is not enabled";
  }
}

function OxpilotContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/oxpilot" : null);
  const statusQ = useQuery(oxpilotStatusQueryOptions());
  const enabled = statusQ.data?.enabled === true;
  const sessionsQ = useQuery(oxpilotSessionListQueryOptions(enabled));
  const sessions: OxpilotSession[] = sessionsQ.data ?? [];

  <main className="mx-auto w-full max-w-4xl px-4 py-8" data-testid="oxpilot-page">
    <PageHeader
      title="Oxpilot"
      description="Agentic chat, builder, and debugger sessions bound to your repositories."
    />
    @if (statusQ.isPending || meQ.isPending) {
      <div className="mt-6 flex flex-col gap-3">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-24 w-full" />
      </div>;
    } @else {
      @if (!enabled) {
        <OxpilotFlagOff reason={statusQ.data?.reason} />;
      } @else {
        <>
          <NewSessionCard />
          <section className="mt-8">
            <h2 className="font-[family-name:var(--font-display)] text-[16px] font-semibold text-foreground">
              Sessions
            </h2>
            @if (sessionsQ.isPending) {
              <Skeleton className="mt-3 h-32 w-full" />;
            } @else {
              @if (sessions.length === 0) {
                <div className="mt-3">
                  <EmptyState
                    icon={<Bot size={22} />}
                    title="No sessions yet"
                    description="Start a session above — Oxpilot streams a reply and keeps the transcript here."
                  />
                </div>;
              } @else {
                <List className="mt-3" data-testid="oxpilot-session-list">
                  @for (const s of sessions; key s.id) {
                    <ListRow className="flex items-center justify-between gap-3 px-4 py-3">
                      <div className="min-w-0">
                        <AppLink
                          href={`/oxpilot/${s.id}`}
                          className="block truncate text-[14px] font-medium text-foreground hover:underline"
                        >
                          {s.title as string}
                        </AppLink>
                        <p className="mt-0.5 truncate text-[13px] text-muted-foreground">
                          {
                            `${s.repo_owner}/${s.repo_name} · ${s.branch}` as string
                          }
                        </p>
                      </div>
                      <span className="shrink-0 text-[12px] text-muted-foreground">
                        {formatRelative(s.last_activity_at) as string}
                      </span>
                    </ListRow>;
                  }
                </List>;
              }
            }
          </section>
        </>;
      }
    }
  </main>;
}

/** Disabled surface — names the envs an admin must set (BR — flag-off UX). */
function OxpilotFlagOff(props: { reason: string | null | undefined }) @{
  <div className="mt-6">
    <EmptyState
      icon={<Bot size={22} />}
      title="Oxpilot is not enabled on this instance"
      description={
        `This instance reports ${flagOffReason(props.reason)}. An administrator can enable it by ` +
        `setting OXIDEAN_OXPILOT_ENABLED=1 and configuring a provider with ` +
        `OXIDEAN_OXPILOT_PROVIDER (for example \`stub\` for a local deterministic provider). ` +
        `See docs/CONFIGURATION.md for the full OXIDEAN_OXPILOT_* surface.`
      }
    />
  </div>;
}

function NewSessionCard() @{
  const qc = useQueryClient();
  const reposQ = useQuery(repoListMineQueryOptions());
  const repos = reposQ.data ?? [];
  const [repoKey, setRepoKey] = useState("");
  const [goal, setGoal] = useState("");
  const [branch, setBranch] = useState("");
  const [error, setError] = useState<string | null>(null);

  const createM = useMutation({
    mutationFn: oxpilotSessionCreate,
    onSuccess: (session) => {
      invalidateOxpilotQueries(qc);
      window.location.href = `/oxpilot/${session.id}`;
    },
    onError: (e) => {
      setError(e instanceof Error ? e.message : "Could not create the session.");
    },
  });

  function submit(ev: SubmitEvent) {
    ev.preventDefault();
    setError(null);
    const [owner, name] = repoKey.split("/", 2);
    if (!owner || !name) {
      setError("Pick a repository for the session.");
      return;
    }
    const g = goal.trim();
    createM.mutate({
      repo_owner: owner,
      repo_name: name,
      ...(branch.trim() ? { branch: branch.trim() } : {}),
      ...(g ? { goal: g } : {}),
    });
  }

  <section
    className="mt-6 rounded-lg border border-border bg-card p-5"
    data-testid="oxpilot-new-session"
  >
    <h2 className="flex items-center gap-2 font-[family-name:var(--font-display)] text-[16px] font-semibold text-foreground">
      <MessageSquarePlus size={18} aria-hidden />
      New session
    </h2>
    <form method="post" action="#" className="mt-4 flex flex-col gap-4" onSubmit={submit}>
      @if (error) {
        <AuthErrorBanner message={error} />;
      }
      <div className="flex flex-col gap-2">
        <Label htmlFor="oxpilot-repo">Repository</Label>
        <SelectRoot
          value={repoKey}
          onValueChange={(v) => setRepoKey(typeof v === "string" ? v : "")}
        >
          <SelectTrigger
            id="oxpilot-repo"
            className="w-full max-w-md"
            aria-label="Repository"
            data-testid="oxpilot-repo-trigger"
          >
            <SelectValue>{(repoKey || "Choose a repository") as string}</SelectValue>
            <SelectIcon>
              <ChevronDown aria-hidden size={16} />
            </SelectIcon>
          </SelectTrigger>
          {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
              escape hatch; popup is anchor-scoped under the Select root. */}
          <SelectPortal>
            <SelectPositioner sideOffset={4}>
              <SelectPopup className="min-w-[16rem]">
                <SelectList>
                  @for (const r of repos; key r.id) {
                    <SelectItem
                      value={`${r.owner_username}/${r.name}`}
                      data-testid="oxpilot-repo-option"
                    >
                      <SelectItemIndicator>
                        <Check aria-hidden size={16} className="text-primary" />
                      </SelectItemIndicator>
                      <SelectItemText>
                        {`${r.owner_username}/${r.name}` as string}
                      </SelectItemText>
                    </SelectItem>;
                  }
                </SelectList>
              </SelectPopup>
            </SelectPositioner>
          </SelectPortal>
        </SelectRoot>
        @if (!reposQ.isPending && repos.length === 0) {
          <p className="text-[13px] text-muted-foreground">
            No repositories yet — create one first, then bind a session to it.
          </p>;
        }
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="oxpilot-goal">Goal (optional)</Label>
        <Textarea
          id="oxpilot-goal"
          rows={3}
          className="max-w-2xl"
          placeholder="What should Oxpilot work on?"
          value={goal}
          onInput={(e) => setGoal((e.target as HTMLTextAreaElement).value)}
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="oxpilot-branch">Branch (optional — defaults to the repo default)</Label>
        <Input
          id="oxpilot-branch"
          className="max-w-md"
          autoComplete="off"
          placeholder="main"
          value={branch}
          onInput={(e) => setBranch((e.target as HTMLInputElement).value)}
        />
      </div>
      <div>
        <Button type="submit" disabled={createM.isPending || repos.length === 0}>
          {createM.isPending ? "Starting…" : "Start session"}
        </Button>
      </div>
    </form>
  </section>;
}
