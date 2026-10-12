import { useForm } from "@octanejs/tanstack-form";
import type { RepoFileCommitPolicyResponse } from "@oxidean/api-client";
import { useEffect } from "octane";
import { Button } from "@/components/ui/button";
import { AppLink } from "@/components/ui/app-link";
import { Input } from "@/components/ui/input";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { cn } from "@/lib/utils";

type FileCommitValues = {
  /** Commit message (blank → server default per operation). */
  message: string;
  /** `direct` commits onto `branch`; `branch` creates a new branch + PR. */
  mode: "direct" | "branch";
  /** Optional explicit branch name (server generates `web-edit/*` when blank). */
  new_branch: string;
  /** Optional PR title (defaults to the commit subject). */
  pr_title: string;
};

type FileCommitFormProps = {
  /** Resolved commit policy for the base branch (null while loading). */
  policy: RepoFileCommitPolicyResponse | null;
  /** Policy query still in flight — submit stays disabled until resolved. */
  policyPending: boolean;
  /** Base branch label shown next to the direct-commit radio. */
  baseBranch: string;
  /** Submit in flight. */
  pending: boolean;
  /** Submit error to surface above the form. */
  error: string;
  submitLabel: string;
  /** Placeholder/example text for the commit message input. */
  messagePlaceholder?: string;
  cancelHref: string;
  onSubmit: (values: FileCommitValues) => void | Promise<void>;
};

// The branch panel lives in its own component scope — a form.Field nested
// inside another Field's render-prop children crashes under octane 0.10
// (function children get invoked eagerly by the children resolver).
// Rendering stays identical: the panel remains always-mounted with `hidden`
// toggling visibility (DOM-race rule).
function FileCommitBranchPanel(props: {
  form: { Field: (props: { name: string; children?: unknown }) => unknown };
  pending: boolean;
  branchSelected: boolean;
}) @{
  <div
    className={cn(
      "mt-3 ml-8 flex flex-col gap-3 border-l border-border pl-4",
      !props.branchSelected && "hidden",
    )}
    data-testid="file-commit-branch-panel"
  >
    <props.form.Field name="new_branch">
      {(branchField) => (
        <div className="flex flex-col gap-2">
          <label for="file-commit-new-branch" className="text-[13px] font-medium text-foreground">
            Branch name
          </label>
          <Input
            id="file-commit-new-branch"
            name="new_branch"
            value={branchField.state.value}
            onBlur={branchField.handleBlur}
            onInput={(e) => branchField.handleChange((e.target as HTMLInputElement).value)}
            placeholder="Leave blank to auto-generate"
            disabled={props.pending}
            data-testid="file-commit-new-branch"
          />
        </div>
      )}
    </props.form.Field>
    <props.form.Field name="pr_title">
      {(titleField) => (
        <div className="flex flex-col gap-2">
          <label for="file-commit-pr-title" className="text-[13px] font-medium text-foreground">
            Pull request title
          </label>
          <Input
            id="file-commit-pr-title"
            name="pr_title"
            value={titleField.state.value}
            onBlur={titleField.handleBlur}
            onInput={(e) => titleField.handleChange((e.target as HTMLInputElement).value)}
            placeholder="Defaults to the commit message"
            disabled={props.pending}
            data-testid="file-commit-pr-title"
          />
        </div>
      )}
    </props.form.Field>
  </div>;
}

/**
 * Shared commit block for every `repo.file.*` flow (GIT-19): message +
 * "commit directly" vs "new branch + pull request" radio driven by
 * `repo.file.commitPolicy`. The branch panel stays mounted and toggles via
 * `hidden` — mounting it beside the RadioGroup on select races Base UI's
 * indicator portal with Octane reconciliation (DOM-race rule).
 */
export function FileCommitForm({
  policy,
  policyPending,
  baseBranch,
  pending,
  error,
  submitLabel,
  messagePlaceholder = "Commit message",
  cancelHref,
  onSubmit,
}: FileCommitFormProps) @{
  const requiresPr = policy?.requires_pr ?? false;
  const disabled = pending || policyPending || !policy;

  const form = useForm({
    defaultValues: {
      message: "",
      mode: "direct",
      new_branch: "",
      pr_title: "",
    } as FileCommitValues,
    onSubmit: async ({ value }) => {
      await onSubmit({ ...value });
    },
  });

  // Late-arriving policy may downgrade "direct" to the PR flow — force the
  // radio so a stale selection can't submit a direct commit to a protected
  // branch.
  useEffect(() => {
    if (requiresPr) {
      form.setFieldValue("mode", "branch");
    }
  }, [requiresPr]);

  <form
    method="post"
    action="#"
    className="flex flex-col gap-4"
    onSubmit={(e) => {
      e.preventDefault();
      void form.handleSubmit();
    }}
    data-testid="file-commit-form"
  >
    @if (error) {
      <div
        role="alert"
        className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[14px] text-destructive"
      >
        {error as string}
      </div>;
    }
    <form.Field name="message">
      {(field) => (
        <div className="flex flex-col gap-2">
          <Input
            id="file-commit-message"
            name="message"
            value={field.state.value}
            onBlur={field.handleBlur}
            onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
            placeholder={messagePlaceholder}
            disabled={pending}
            aria-label="Commit message"
            data-testid="file-commit-message"
          />
        </div>
      )}
    </form.Field>

    <form.Field name="mode">
      {(field) => (
        <>
          <RadioGroup
            value={field.state.value}
            disabled={disabled}
            onValueChange={(v) => {
              if (v === "direct" || v === "branch") field.handleChange(v);
            }}
            data-testid="file-commit-mode"
          >
            <label className="flex cursor-pointer items-start gap-3">
              <RadioGroupItem
                value="direct"
                disabled={requiresPr}
                className="mt-0.5"
                data-testid="file-commit-mode-direct"
              />
              <span className="flex flex-col gap-1">
                <span className="text-[14px] text-foreground">
                  {`Commit directly to ${baseBranch}` as string}
                </span>
                @if (requiresPr) {
                  <span className="text-[13px] text-muted-foreground">
                    {`${baseBranch} is protected — direct commits are not allowed.` as string}
                  </span>;
                }
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-3">
              <RadioGroupItem
                value="branch"
                className="mt-0.5"
                data-testid="file-commit-mode-branch"
              />
              <span className="flex flex-col gap-1">
                <span className="text-[14px] text-foreground">
                  Create a new branch and start a pull request
                </span>
                <span className="text-[13px] text-muted-foreground">
                  Changes commit on a new branch; a pull request targets {baseBranch as string}.
                </span>
              </span>
            </label>
          </RadioGroup>
          {/* Always mounted — `hidden` toggles visibility so RadioGroup sibling
              reconciliation never mounts/unmounts this panel (DOM-race rule). */}
          <FileCommitBranchPanel
            form={form}
            pending={pending}
            branchSelected={field.state.value === "branch"}
          />
        </>
      )}
    </form.Field>

    <div className="flex items-center gap-3">
      <Button type="submit" disabled={disabled} data-testid="file-commit-submit">
        {(pending ? "Committing…" : submitLabel) as string}
      </Button>
      <AppLink href={cancelHref as string} className="text-[14px] text-muted-foreground underline">
        Cancel
      </AppLink>
      @if (policyPending) {
        <span className="text-[13px] text-muted-foreground">
          Checking branch protection…
        </span>;
      }
    </div>
  </form>;
}
