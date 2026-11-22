import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { OrgTeamPublic } from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { AppLink } from "@/components/ui/app-link";
import { apiClient } from "@/lib/api-client";
import { AppPage } from "@/components/app-page";
import {
  OrgSettingsShell,
  orgSettingsAccessQueryOptions,
  orgSettingsOwner,
} from "@/components/org/org-settings-shell.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export function teamsQueryKey(slug: string) {
  return ["org", "teams", slug] as const;
}

/** `/{owner}/settings/teams` — org teams list + create (COL-05). */
export function OrgTeamsPage() @{
  <AppPage>
    <OrgSettingsShell active="teams">
      <OrgTeamsContent />
    </OrgSettingsShell>
  </AppPage>;
}

function OrgTeamsContent() @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const data = useQuery(orgSettingsAccessQueryOptions(owner)).data;
  const queryClient = useQueryClient();

  const org = data?.kind === "ready" ? data.org : null;
  const slug = org?.slug ?? owner;
  const canAdmin = data?.kind === "ready" && data.canAdmin;

  const teamsQ = useQuery({
    queryKey: teamsQueryKey(slug),
    enabled: !!slug,
    queryFn: async () => {
      const res = await apiClient.org.teams.list({ slug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });
  const teams: OrgTeamPublic[] = teamsQ.data?.teams ?? [];

  const [showCreate, setShowCreate] = useState(false);
  const [createError, setCreateError] = useState("");
  const [createPending, setCreatePending] = useState(false);

  const createForm = useForm({
    defaultValues: {
      name: "",
      team_slug: "",
      description: "",
    },
    onSubmit: async ({ value }) => {
      setCreateError("");
      if (!value.name.trim()) {
        setCreateError("Team name is required.");
        return;
      }
      setCreatePending(true);
      try {
        const res = await apiClient.org.teams.create({
          slug,
          name: value.name.trim(),
          team_slug: value.team_slug.trim() || null,
          description: value.description.trim() || null,
        });
        if (!res.ok) {
          setCreateError(res.error.message || NETWORK_ERROR);
          setCreatePending(false);
          return;
        }
        createForm.reset();
        setShowCreate(false);
        setCreatePending(false);
        await queryClient.invalidateQueries({ queryKey: teamsQueryKey(slug) });
      } catch {
        setCreateError(NETWORK_ERROR);
        setCreatePending(false);
      }
    },
  });

  <section className="flex flex-col gap-4" data-testid="org-teams-page">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h2 className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
          Teams
        </h2>
        <p className="text-[14px] text-muted-foreground">
          Group members into teams and grant them shared access to organization repositories.
        </p>
      </div>
      @if (canAdmin && !showCreate) {
        <Button
          type="button"
          className="h-11 w-fit"
          data-testid="team-create-open"
          onClick={() => setShowCreate(true)}
        >
          New team
        </Button>;
      }
    </div>

    @if (showCreate && canAdmin) {
      <form
        className="flex flex-col gap-4 rounded-md border border-border p-4"
        method="post"
        action="#"
        data-testid="team-create-form"
        onSubmit={(e) => {
          e.preventDefault();
          void createForm.handleSubmit();
        }}
      >
        <h3 className="text-[16px] font-semibold text-foreground">Create team</h3>
        @if (createError) {
          <AuthErrorBanner message={createError} />;
        }
        <createForm.Field name="name">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="team-create-name">Name</Label>
              <Input
                id="team-create-name"
                data-testid="team-create-name"
                value={field.state.value}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                placeholder="Core developers"
                autoComplete="off"
              />
            </div>
          )}
        </createForm.Field>
        <createForm.Field name="team_slug">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="team-create-slug">Slug (optional)</Label>
              <Input
                id="team-create-slug"
                data-testid="team-create-slug"
                value={field.state.value}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                placeholder="core-developers"
                autoComplete="off"
              />
              <p className="text-[12px] text-muted-foreground">
                Lowercase letters, numbers, and hyphens. Derived from the name when blank.
              </p>
            </div>
          )}
        </createForm.Field>
        <createForm.Field name="description">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="team-create-description">Description (optional)</Label>
              <Textarea
                id="team-create-description"
                data-testid="team-create-description"
                value={field.state.value}
                onInput={(e) => field.handleChange((e.target as HTMLTextAreaElement).value)}
                rows={2}
              />
            </div>
          )}
        </createForm.Field>
        <div className="flex flex-wrap gap-2">
          <Button
            type="submit"
            className="h-11"
            data-testid="team-create-submit"
            disabled={createPending}
          >
            @if (createPending) {
              <>Creating…</>;
            } @else {
              <>Create team</>;
            }
          </Button>
          <Button
            type="button"
            variant="secondary"
            className="h-11"
            disabled={createPending}
            onClick={() => {
              setShowCreate(false);
              setCreateError("");
              createForm.reset();
            }}
          >
            Cancel
          </Button>
        </div>
      </form>;
    }

    @if (teamsQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading teams…</p>;
    } @else if (teamsQ.isError) {
      <p role="alert" className="text-[14px] text-destructive">
        {(teamsQ.error instanceof Error ? teamsQ.error.message : NETWORK_ERROR) as string}
      </p>;
    } @else if (teams.length === 0) {
      <p className="text-[14px] text-muted-foreground" data-testid="teams-empty">
        No teams yet. Create one to manage repository access for a group of members.
      </p>;
    } @else {
      <div className="overflow-x-auto">
        <table className="w-full min-w-[32rem] border-collapse text-left text-[14px]">
          <thead>
            <tr className="border-b border-border text-muted-foreground">
              <th className="py-2 pr-4 font-medium">Team</th>
              <th className="py-2 pr-4 font-medium">Members</th>
              <th className="py-2 font-medium">Repositories</th>
            </tr>
          </thead>
          <tbody>
            @for (const team of teams; key team.id) {
              <tr
                className="border-b border-border"
                data-testid={`team-row-${team.slug}`}
              >
                <td className="py-3 pr-4">
                  <AppLink
                    href={`/${slug}/settings/teams/${team.slug}`}
                    className="font-medium text-foreground no-underline hover:underline"
                    data-testid={`team-link-${team.slug}`}
                  >
                    {team.name as string}
                  </AppLink>
                  <div className="text-[12px] text-muted-foreground">
                    {"@" + (team.slug as string)}
                  </div>
                  @if (team.description) {
                    <div className="mt-1 text-[13px] text-muted-foreground">
                      {team.description as string}
                    </div>;
                  }
                </td>
                <td className="py-3 pr-4 text-muted-foreground">
                  {team.member_count as number}
                </td>
                <td className="py-3 text-muted-foreground">
                  {team.repo_count as number}
                </td>
              </tr>;
            }
          </tbody>
        </table>
      </div>;
    }
  </section>;
}
