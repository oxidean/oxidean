import { useEffect, useState } from "octane";
import { useForm } from "@octanejs/tanstack-form";
import { Check, ChevronDown } from "@octanejs/lucide";
import type { RepoPublic } from "@oxidean/api-client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
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
import { PatExpiryField } from "@/components/settings/pat-expiry-field";
import {
  resolveExpiresAt,
  tomorrowDateValue,
  type PatExpiryPreset,
} from "@/components/settings/pat-expiry";
import { PatFgRepoPicker } from "@/components/settings/pat-fg-repo-picker";
import { apiClient } from "@/lib/api-client";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type PatFgFormProps = {
  onCreated: (token: string) => void;
};

type ContentsChoice = "read" | "write";
type PackagesChoice = "none" | "read" | "write";

function repoFullName(repo: RepoPublic): string {
  return `${repo.owner_username}/${repo.name}`;
}

function contentsLabel(v: ContentsChoice): string {
  return v === "write" ? "Read and write" : "Read-only";
}

function packagesLabel(v: PackagesChoice): string {
  if (v === "write") return "Read and write";
  if (v === "read") return "Read-only";
  return "No access";
}

/** Fine-grained PAT create form — keep panels mounted; narrow Field/Subscribe. */
export function PatFgForm({ onCreated }: PatFgFormProps) @{
  const [noteError, setNoteError] = useState("");
  const [repoError, setRepoError] = useState("");
  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);
  const [repos, setRepos] = useState<RepoPublic[] | null>(null);
  const [reposError, setReposError] = useState("");
  const [repoFilter, setRepoFilter] = useState("");

  const form = useForm({
    defaultValues: {
      note: "",
      repoAccess: "selected" as "all" | "selected",
      selectedIds: [] as string[],
      contents: "read" as ContentsChoice,
      packagesPerm: "none" as PackagesChoice,
      expiryPreset: "30" as PatExpiryPreset,
      expiryDate: tomorrowDateValue(),
    },
    onSubmit: async ({ value }) => {
      setNoteError("");
      setRepoError("");
      setFormError("");
      const trimmed = value.note.trim();
      if (!trimmed) {
        setNoteError("Note is required.");
        return;
      }
      if (value.repoAccess === "selected" && value.selectedIds.length === 0) {
        setRepoError("Select at least one repository.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.pat.createFineGrained({
          name: trimmed,
          repo_access: value.repoAccess,
          repository_ids: value.repoAccess === "selected" ? value.selectedIds : undefined,
          contents: value.contents,
          packages: value.packagesPerm === "none" ? null : value.packagesPerm,
          expires_at: resolveExpiresAt(value.expiryPreset, value.expiryDate),
        });
        if (!res.ok) {
          if (res.error.code === "auth.email_unverified") {
            setFormError("Verify your email before creating a personal access token.");
          } else if (res.error.code === "pat.note_required") {
            setNoteError("Note is required.");
          } else if (res.error.code === "pat.repos_required") {
            setRepoError("Select at least one repository.");
          } else {
            setFormError(res.error.message || NETWORK_ERROR);
          }
          setPending(false);
          return;
        }
        onCreated(res.data.token);
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await apiClient.repo.listMine();
        if (cancelled) return;
        if (!res.ok) {
          setReposError(res.error.message || NETWORK_ERROR);
          setRepos([]);
          return;
        }
        setRepos(res.data.repos);
      } catch {
        if (cancelled) return;
        setReposError(NETWORK_ERROR);
        setRepos([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  function toggleRepo(id: string, checked: boolean) {
    const current = form.getFieldValue("selectedIds");
    if (checked) {
      if (current.includes(id)) return;
      form.setFieldValue("selectedIds", [...current, id]);
    } else {
      form.setFieldValue(
        "selectedIds",
        current.filter((x) => x !== id),
      );
    }
  }

  const reposLoading = repos === null;
  const ownedRepos = repos ?? [];
  const filterNeedle = repoFilter.trim().toLowerCase();
  const filteredRepos =
    filterNeedle.length === 0
      ? ownedRepos
      : ownedRepos.filter((r) => repoFullName(r).toLowerCase().includes(filterNeedle));

  <form
    method="post"
    action="#"
    className="mt-8 flex flex-col gap-6"
    data-testid="pat-fg-form"
    onSubmit={(e) => {
      e.preventDefault();
      void form.handleSubmit();
    }}
  >
    <form.Field name="note">
      {(field) => (
        <div className="flex flex-col gap-2">
          <Label htmlFor="pat-fg-note">Note</Label>
          <Input
            id="pat-fg-note"
            value={field.state.value}
            aria-label="Note"
            autoComplete="off"
            disabled={pending}
            onBlur={field.handleBlur}
            onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
          />
          <p className="text-[14px] text-muted-foreground">
            What’s this token for? For example, laptop or CI.
          </p>
          <p
            role="alert"
            className={noteError ? "text-[14px] text-destructive" : "hidden"}
            data-testid="pat-fg-note-error"
          >
            {(noteError || "") as string}
          </p>
        </div>
      )}
    </form.Field>

    <div className="flex flex-col gap-3">
      <p className="text-[14px] font-medium text-foreground">Repository access</p>
      <form.Field name="repoAccess">
        {(field) => (
          <RadioGroup
            value={field.state.value}
            disabled={pending}
            onValueChange={(v) => {
              if (v === "all" || v === "selected") field.handleChange(v);
            }}
            className="gap-3"
          >
            <label className="flex cursor-pointer items-start gap-3">
              <RadioGroupItem value="all" className="mt-0.5" data-testid="fg-repo-access-all" />
              <span className="flex flex-col gap-1">
                <span className="text-[14px] text-foreground">All repositories</span>
                <span className="text-[14px] text-muted-foreground">
                  Includes all current and future repositories you own.
                </span>
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-3">
              <RadioGroupItem
                value="selected"
                className="mt-0.5"
                data-testid="fg-repo-access-selected"
              />
              <span className="text-[14px] text-foreground">Only select repositories</span>
            </label>
          </RadioGroup>
        )}
      </form.Field>

      <form.Subscribe
        selector={(s) => ({
          repoAccess: s.values.repoAccess,
          selectedIds: s.values.selectedIds,
        })}
      >
        {(v) => (
          <PatFgRepoPicker
            visible={v.repoAccess === "selected"}
            pending={pending}
            reposLoading={reposLoading}
            reposError={reposError}
            ownedRepos={ownedRepos}
            filteredRepos={filteredRepos}
            selectedIds={v.selectedIds}
            repoFilter={repoFilter}
            onRepoFilterChange={setRepoFilter}
            onToggleRepo={toggleRepo}
          />
        )}
      </form.Subscribe>

      <p
        role="alert"
        className={repoError ? "text-[14px] text-destructive" : "hidden"}
        data-testid="pat-fg-repo-error"
      >
        {(repoError || "") as string}
      </p>
    </div>

    <div className="flex flex-col gap-3">
      <p className="text-[14px] font-medium text-foreground">Repository permissions</p>
      <form.Field name="contents">
        {(field) => (
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex flex-col gap-0.5">
              <span className="text-[14px] text-foreground">Contents</span>
              <span className="text-[13px] text-muted-foreground">
                Repository code via git. Read-only allows clone and fetch; write also allows push.
              </span>
            </div>
            <SelectRoot
              value={field.state.value}
              onValueChange={(next) => {
                if (next === "read" || next === "write") field.handleChange(next);
              }}
            >
              <SelectTrigger
                className="w-full max-w-[12rem]"
                aria-label="Contents permission"
                disabled={pending}
                data-testid="fg-contents-perm"
              >
                <SelectValue>{contentsLabel(field.state.value) as string}</SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={16} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
                  hatch; popup is anchor-scoped under the Select root. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      <SelectItem value="read">
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>Read-only</SelectItemText>
                      </SelectItem>
                      <SelectItem value="write">
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>Read and write</SelectItemText>
                      </SelectItem>
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
          </div>
        )}
      </form.Field>

      <form.Field name="packagesPerm">
        {(field) => (
          <div
            className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between"
            data-testid="fg-packages-perm"
          >
            <div className="flex flex-col gap-0.5">
              <span className="text-[14px] text-foreground">Packages</span>
              <span className="text-[13px] text-muted-foreground">
                Package registry access. Contents alone does not grant package access.
              </span>
            </div>
            <SelectRoot
              value={field.state.value}
              onValueChange={(next) => {
                if (next === "none" || next === "read" || next === "write") {
                  field.handleChange(next);
                }
              }}
            >
              <SelectTrigger
                className="w-full max-w-[12rem]"
                aria-label="Packages permission"
                disabled={pending}
                data-testid="fg-packages-perm-select"
              >
                <SelectValue>{packagesLabel(field.state.value) as string}</SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={16} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
                  hatch; popup is anchor-scoped under the Select root. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      <SelectItem value="none">
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>No access</SelectItemText>
                      </SelectItem>
                      <SelectItem value="read">
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>Read-only</SelectItemText>
                      </SelectItem>
                      <SelectItem value="write">
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>Read and write</SelectItemText>
                      </SelectItem>
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
          </div>
        )}
      </form.Field>
    </div>

    <form.Field name="expiryPreset">
      {(presetField) => (
        <form.Field name="expiryDate">
          {(dateField) => (
            <PatExpiryField
              preset={presetField.state.value}
              customDate={dateField.state.value}
              disabled={pending}
              onPresetChange={(p) => presetField.handleChange(p)}
              onCustomDateChange={(d) => dateField.handleChange(d)}
              noneHint="This token will never expire. GitHub fine-grained tokens require an expiration; Oxidean still allows none for self-host flexibility."
            />
          )}
        </form.Field>
      )}
    </form.Field>

    <form.Subscribe
      selector={(s) => ({
        repoAccess: s.values.repoAccess,
        selectedCount: s.values.selectedIds.length,
        contents: s.values.contents,
        packagesPerm: s.values.packagesPerm,
        expiryPreset: s.values.expiryPreset,
      })}
    >
      {(v) => {
        const repoText =
          v.repoAccess === "all"
            ? "all repositories"
            : `${v.selectedCount} selected ${v.selectedCount === 1 ? "repository" : "repositories"}`;
        const pkgText =
          v.packagesPerm === "none"
            ? "no package access"
            : `packages ${packagesLabel(v.packagesPerm).toLowerCase()}`;
        const expText =
          v.expiryPreset === "none"
            ? "no expiration"
            : v.expiryPreset === "custom"
              ? "a custom expiration date"
              : `expiration in ${v.expiryPreset} days`;
        return (
          <p className="text-[14px] text-muted-foreground" data-testid="pat-fg-summary">
            {
              `This token will access ${repoText}, with contents ${contentsLabel(v.contents).toLowerCase()}, ${pkgText}, and ${expText}.` as string
            }
          </p>
        );
      }}
    </form.Subscribe>

    <p
      role="alert"
      className={formError ? "text-[16px] text-destructive" : "hidden"}
      data-testid="pat-fg-form-error"
    >
      {(formError || "") as string}
    </p>

    <Button type="submit" className="h-11 w-fit" disabled={pending}>
      {(pending ? "Generating…" : "Generate token") as string}
    </Button>
  </form>;
}
