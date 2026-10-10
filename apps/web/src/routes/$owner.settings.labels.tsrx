import { useForm } from "@octanejs/tanstack-form";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { LabelPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { Tags } from "@octanejs/lucide";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { AppPage } from "@/components/app-page";
import {
  OrgSettingsShell,
  orgSettingsAccessQueryOptions,
  orgSettingsOwner,
} from "@/components/org/org-settings-shell.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need organization admin access to manage labels.";

function labelsQueryKey(slug: string) {
  return ["label", "listForOrg", slug] as const;
}

function swatchStyle(color: string): { backgroundColor: string } {
  const hex = color.startsWith("#") ? color : `#${color}`;
  return { backgroundColor: hex };
}

/** Org default label catalog — Admin only (D-ISS-05 / D-ISS-07). */
/** `/{owner}/settings/labels` — org label catalog (title via shell). */
export function OrgLabelsPage() @{
  <AppPage>
    <OrgSettingsShell active="labels">
      <OrgLabelsContent />
    </OrgSettingsShell>
  </AppPage>;
}

/** Org default label catalog — Admin only (D-ISS-05 / D-ISS-07). */
function OrgLabelsContent() @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const data = useQuery(orgSettingsAccessQueryOptions(owner)).data;
  const org = data?.kind === "ready" ? data.org : null;
  const slug = org?.slug ?? owner;
  const queryClient = useQueryClient();

  const labelsQ = useQuery({
    queryKey: labelsQueryKey(slug),
    queryFn: async () => {
      const res = await apiClient.label.listForOrg({ slug });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.labels;
    },
    enabled: !!slug,
  });

  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);
  const [rowError, setRowError] = useState("");

  const form = useForm({
    defaultValues: {
      name: "",
      color: "0366d6",
      description: "",
    },
    onSubmit: async ({ value }) => {
      setFormError("");
      const trimmed = value.name.trim();
      if (!trimmed) {
        setFormError("Name is required.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.label.create({
          scope: "org",
          owner: slug,
          name: trimmed,
          color: value.color,
          description: value.description.trim() || null,
        });
        if (!res.ok) {
          if (res.error.code === "org.forbidden") {
            setFormError(FORBIDDEN);
          } else {
            setFormError(res.error.message || NETWORK_ERROR);
          }
          setPending(false);
          return;
        }
        form.reset();
        setPending(false);
        await refresh();
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  const labels: LabelPublic[] = labelsQ.data ?? [];

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: labelsQueryKey(slug) });
  }

  async function deleteLabel(lab: LabelPublic) {
    setRowError("");
    try {
      const res = await apiClient.label.delete({
        id: lab.id,
        owner: slug,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  <div data-testid="org-settings-labels">
    <h2 className="mb-1 text-[18px] font-semibold text-foreground">Labels</h2>
    <p className="mb-6 text-[14px] text-muted-foreground">
      Default labels inherited by organization repositories. Repos can hide or add local labels.
    </p>

    @if (formError) {
      <div className="mb-4">
        <AuthErrorBanner message={formError} />
      </div>;
    }
    @if (rowError) {
      <div className="mb-4">
        <AuthErrorBanner message={rowError} />
      </div>;
    }

    <form
      method="post"
      action="#"
      className="mb-8 flex flex-col gap-3 rounded-md border border-border p-4"
      onSubmit={(e) => {
        e.preventDefault();
        void form.handleSubmit();
      }}
    >
      <p className="text-[14px] font-semibold text-foreground">New label</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <form.Field name="name">
          {(field) => (
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="org-label-name">Name</Label>
              <Input
                id="org-label-name"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                disabled={pending}
              />
            </div>
          )}
        </form.Field>
        <form.Field name="color">
          {(field) => (
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="org-label-color">Color (hex)</Label>
              <Input
                id="org-label-color"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                disabled={pending}
              />
            </div>
          )}
        </form.Field>
      </div>
      <form.Field name="description">
        {(field) => (
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="org-label-desc">Description</Label>
            <Input
              id="org-label-desc"
              value={field.state.value}
              onBlur={field.handleBlur}
              onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
              disabled={pending}
            />
          </div>
        )}
      </form.Field>
      <div>
        <Button type="submit" disabled={pending}>
          Create label
        </Button>
      </div>
    </form>

    @if (labelsQ.isPending) {
      <List>
        @for (const i of [0, 1, 2]; key i) {
          <ListRow className="justify-between">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-8 w-16 rounded-md" />
          </ListRow>;
        }
      </List>;
    } @else {
      @if (labels.length === 0) {
        <EmptyState
          icon={<Tags aria-hidden size={22} />}
          title="No org labels yet"
          description="Create shared labels that member repositories can reuse for issues and pull requests."
        />;
      } @else {
        <List>
          @for (const lab of labels; key lab.id) {
            <ListRow className="flex-wrap justify-between gap-3">
              <div className="flex items-center gap-2">
                <span
                  className="inline-block size-3 rounded-full border border-border"
                  style={swatchStyle(lab.color)}
                  aria-hidden
                />
                <span className="text-[15px] font-medium text-foreground">
                  {lab.name as string}
                </span>
                @if (lab.description) {
                  <span className="text-[13px] text-muted-foreground">
                    {lab.description as string}
                  </span>;
                }
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => deleteLabel(lab)}
              >
                Delete
              </Button>
            </ListRow>;
          }
        </List>;
      }
    }
  </div>;
}

export default OrgLabelsPage;
