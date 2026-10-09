import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useForm, useStore } from "@octanejs/tanstack-form";
import type { OrgMineEntry, RepoCreateDefaults, RepoVisibility } from "@oxidean/api-client";
import { Check, ChevronDown, Globe, Lock } from "@octanejs/lucide";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { TemplatePicker } from "@/components/repo/template-picker";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { listLicensePickerOptions } from "@/lib/spdx-licenses";
import { fetchRepoCreateDefaults } from "@/lib/ssr-auth";
import { fetchOrgListMine } from "@/lib/ssr-org";
import { cn } from "@/lib/utils";
import { AppPage } from "@/components/app-page";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const NAME_HELPER = "Letters, digits, hyphen, underscore, and period. Reserved names are rejected.";
const DUPLICATE_NAME_ERROR = "A repository with this name already exists. Choose a different name.";

/** Repo-create defaults — soft (null on failure), verified users only. */
function repoCreateDefaultsQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: ["repo", "createDefaults"],
    queryFn: async (): Promise<RepoCreateDefaults | null> => {
      const res = await fetchRepoCreateDefaults();
      return res.ok ? res.data : null;
    },
    enabled,
    retry: false,
    staleTime: 60_000,
  });
}

/** Owner/Admin orgs for the /new owner picker (D-ORG-06) — soft, verified only. */
function newOwnerOrgsQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: ["org", "listMine", "adminable"],
    queryFn: async (): Promise<OrgMineEntry[]> => {
      const res = await fetchOrgListMine();
      if (!res.ok) return [];
      return res.data.orgs.filter((o) => o.role === "owner" || o.role === "admin");
    },
    enabled,
    retry: false,
    staleTime: 60_000,
  });
}

function ownerOptionLabel(slug: string, selfUsername: string, orgs: OrgMineEntry[]): string {
  if (slug === selfUsername) return `@${selfUsername}`;
  const org = orgs.find((o) => o.slug === slug);
  if (org) {
    const name = org.display_name?.trim();
    return name ? `${name} (${org.slug})` : org.slug;
  }
  return slug;
}

/** `/new` — create repository (title/meta via shell). */
export function NewPage() @{
  <AppPage>
    <NewContent />
  </AppPage>;
}

/** Exported for /new Wave 0 integration tests (D-11 verify wall). */
function NewContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  const user = meQ.data ?? undefined;
  // Auth gate (formerly beforeLoad): anonymous → login.
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/new" : null);
  const verified = user?.email_verified === true;
  const defaultsQ = useQuery(repoCreateDefaultsQueryOptions(verified));
  const orgsQ = useQuery(newOwnerOrgsQueryOptions(verified));
  const showWall = !user || !verified;
  const ownerOrgs = orgsQ.data ?? [];

  const stacks = defaultsQ.data?.stacks ?? [];
  const gitignores = defaultsQ.data?.gitignores ?? [];
  const licenseOptions = listLicensePickerOptions();

  const initialVisibility: RepoVisibility =
    defaultsQ.data?.default_visibility === "private" ? "private" : "public";

  const [nameError, setNameError] = useState("");
  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);

  const selfUsername = user?.username ?? "";

  const form = useForm({
    defaultValues: {
      name: "",
      description: "",
      visibility: initialVisibility,
      stackId: "none",
      licenseId: "none",
      gitignoreId: "none",
      ownerSlug: user?.username ?? "",
    },
    onSubmit: async ({ value }) => {
      setNameError("");
      setFormError("");
      const trimmed = value.name.trim();
      if (!trimmed) {
        setNameError("Repository name is required.");
        return;
      }
      const selectedOwner = value.ownerSlug || selfUsername;
      setPending(true);
      try {
        const res = await apiClient.repo.create({
          name: trimmed,
          description: value.description.trim() || null,
          visibility: value.visibility,
          stack_id: value.stackId === "none" ? null : value.stackId,
          license_id: value.licenseId === "none" ? null : value.licenseId,
          gitignore_id: value.gitignoreId,
          owner: selectedOwner || null,
        });
        if (!res.ok) {
          const code = res.error.code;
          if (code === "repo.name_taken") {
            setNameError(DUPLICATE_NAME_ERROR);
          } else if (code === "repo.invalid_name") {
            setNameError(res.error.message);
          } else if (code === "auth.email_unverified") {
            setFormError("Verify your email before creating a repository.");
          } else {
            setFormError(res.error.message || NETWORK_ERROR);
          }
          setPending(false);
          return;
        }
        window.location.assign(`/${res.data.owner_username}/${res.data.name}`);
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  // Read at component scope — form.Field nested inside a form.Subscribe
  // render-prop crashes under octane 0.10 (children resolver invokes function
  // children eagerly).
  const values = useStore(form.store, (s) => s.values);
  const selectedOwner = values.ownerSlug || selfUsername;
  const ownerDisplay = ownerOptionLabel(selectedOwner, selfUsername, ownerOrgs);

  @if (showWall) {
    <AuthShell title="Verify your email" support="Verify your email before creating a repository.">
      <AppLink href="/verify" className={cn(buttonVariants({ variant: "default" }))}>
        Verify email
      </AppLink>
    </AuthShell>;
  } @else {
    {
      /* octane-dom-race-ok: /new create form — reviewed; Select list @for + error
        banner @if; restructure deferred from PAT mint work. */
    }
    <div className="mx-auto max-w-2xl px-4 py-16">
      <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
        Create a new repository
      </h1>
      <p className="mt-2 text-[16px] text-muted-foreground">
        Choose a personal account or an organization you administer.
      </p>

      <form
        className="mt-8 flex flex-col gap-6"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        @if (formError) {
          <AuthErrorBanner message={formError} />;
        }

        <div className="flex flex-col gap-2">
          <Label htmlFor="repo-owner">Owner</Label>
          <SelectRoot
            value={selectedOwner}
            onValueChange={(next) => {
              if (typeof next === "string") form.setFieldValue("ownerSlug", next);
            }}
          >
            <SelectTrigger id="repo-owner" className="w-full max-w-sm" aria-label="Owner">
              <SelectValue>{ownerDisplay as string}</SelectValue>
              <SelectIcon>
                <ChevronDown aria-hidden size={16} />
              </SelectIcon>
            </SelectTrigger>
            {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
                      escape hatch; popup is anchor-scoped under the Select root. */}
            <SelectPortal>
              <SelectPositioner sideOffset={4}>
                <SelectPopup className="min-w-[12rem]">
                  <SelectList>
                    <SelectItem value={selfUsername}>
                      <SelectItemIndicator>
                        <Check aria-hidden size={16} className="text-primary" />
                      </SelectItemIndicator>
                      <SelectItemText>{("@" + selfUsername) as string}</SelectItemText>
                    </SelectItem>
                    @for (const org of ownerOrgs; key org.slug) {
                      <SelectItem value={org.slug}>
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>
                          {
                            (org.display_name?.trim()
                              ? `${org.display_name.trim()} (${org.slug})`
                              : org.slug) as string
                          }
                        </SelectItemText>
                      </SelectItem>;
                    }
                  </SelectList>
                </SelectPopup>
              </SelectPositioner>
            </SelectPortal>
          </SelectRoot>
        </div>

        <form.Field name="name">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-name">Repository name</Label>
              <Input
                id="repo-name"
                name="name"
                autoComplete="off"
                value={field.state.value}
                aria-invalid={nameError ? true : undefined}
                onBlur={field.handleBlur}
                onInput={(ev) => {
                  field.handleChange((ev.currentTarget as HTMLInputElement).value);
                  if (nameError) setNameError("");
                }}
                required
              />
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                {NAME_HELPER}
              </p>
              @if (nameError) {
                <p role="alert" className="text-[14px] text-destructive">
                  {nameError as string}
                </p>;
              }
            </div>
          )}
        </form.Field>

        <form.Field name="description">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-description">Description (optional)</Label>
              <Textarea
                id="repo-description"
                name="description"
                rows={3}
                className="max-w-full break-words [overflow-wrap:anywhere]"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLTextAreaElement).value)
                }
              />
            </div>
          )}
        </form.Field>

        <fieldset className="flex flex-col gap-2">
          <legend className="text-[14px] font-normal leading-[1.4] text-foreground">
            Visibility
          </legend>
          <div className="flex flex-wrap gap-2" role="group" aria-label="Visibility">
            <Button
              type="button"
              variant={values.visibility === "public" ? "default" : "secondary"}
              aria-pressed={values.visibility === "public"}
              onClick={() => form.setFieldValue("visibility", "public")}
            >
              <Globe aria-hidden size={16} />
              Public
            </Button>
            <Button
              type="button"
              variant={values.visibility === "private" ? "default" : "secondary"}
              aria-pressed={values.visibility === "private"}
              onClick={() => form.setFieldValue("visibility", "private")}
            >
              <Lock aria-hidden size={16} />
              Private
            </Button>
          </div>
          <p className="text-[13px] text-muted-foreground">
            {
              (values.visibility === "public"
                ? "Anyone on this instance can see this repository."
                : "Only you and collaborators can see this repository.") as string
            }
          </p>
        </fieldset>

        <div className="flex flex-col gap-5">
          <div>
            <p className="text-[14px] font-semibold text-foreground">Templates (optional)</p>
            <p className="mt-1 text-[13px] text-muted-foreground">
              Seed an initial commit from an official-style stack starter, an instance pack, or a
              user template repository — or leave as None for an empty Quick setup repo.
            </p>
          </div>
          <TemplatePicker
            id="repo-stack"
            label="Stack / template"
            description="Official starters, instance packs, and published template repos. Provenance is shown on each card."
            value={values.stackId}
            options={stacks}
            noneLabel="None"
            noneDescription="No template files — empty repo with Quick setup."
            onChange={(id) => {
              form.setFieldValue("stackId", id);
              if (id === "none") {
                form.setFieldValue("gitignoreId", "none");
                return;
              }
              const pack = stacks.find((s) => s.id === id);
              form.setFieldValue("gitignoreId", pack?.default_gitignore ?? "none");
            }}
          />
          <TemplatePicker
            id="repo-license"
            label="License"
            description="LICENSE file for the initial commit. Browse popular licenses or search the full SPDX catalog."
            value={values.licenseId}
            options={licenseOptions}
            noneLabel="None"
            noneDescription="No LICENSE in the initial commit."
            browseGroups={["Popular"]}
            searchPlaceholder="Search SPDX licenses…"
            onChange={(id) => form.setFieldValue("licenseId", id)}
          />
          <TemplatePicker
            id="repo-gitignore"
            label=".gitignore"
            description="Language and tooling ignore rules. Auto-filled from the stack; set to None to skip."
            value={values.gitignoreId}
            options={gitignores}
            noneLabel="None"
            noneDescription="No .gitignore in the initial commit."
            onChange={(id) => form.setFieldValue("gitignoreId", id)}
          />
        </div>

        <Button type="submit" disabled={pending}>
          @if (pending) {
            <>Creating…</>;
          } @else {
            <>Create repository</>;
          }
        </Button>
      </form>
    </div>;
  }
}
