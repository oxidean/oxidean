import type { BranchProtectionRulePublic, TagProtectionRulePublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function protectionQueryKey(owner: string, name: string) {
  return ["repo", "branchProtection", owner, name] as const;
}

function tagProtectionQueryKey(owner: string, name: string) {
  return ["repo", "tagProtection", owner, name] as const;
}

/** Repo Settings → Branch protection (ORG-05 / D-25). */
export function BranchProtectionPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const rulesQ = useQuery({
    queryKey: protectionQueryKey(owner, name),
    queryFn: async (): Promise<BranchProtectionRulePublic[]> => {
      const res = await apiClient.repo.branchProtection.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load branch protection.");
      return res.data.rules;
    },
    enabled: canAdmin,
  });

  const [pattern, setPattern] = useState("main");
  const [requireReviews, setRequireReviews] = useState(true);
  const [reviewCount, setReviewCount] = useState("1");
  const [contexts, setContexts] = useState("");
  const [strict, setStrict] = useState(false);
  const [dismissStale, setDismissStale] = useState(false);
  const [requireConversations, setRequireConversations] = useState(false);
  const [requireLastPush, setRequireLastPush] = useState(false);
  const [allowForce, setAllowForce] = useState(false);
  const [allowDeletes, setAllowDeletes] = useState(false);
  const [enforceAdmins, setEnforceAdmins] = useState(false);
  const [linearHistory, setLinearHistory] = useState(false);
  const [lockBranch, setLockBranch] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  function resetForm() {
    setEditingId(null);
    setPattern("main");
    setRequireReviews(true);
    setReviewCount("1");
    setContexts("");
    setStrict(false);
    setDismissStale(false);
    setRequireConversations(false);
    setRequireLastPush(false);
    setAllowForce(false);
    setAllowDeletes(false);
    setEnforceAdmins(false);
    setLinearHistory(false);
    setLockBranch(false);
  }

  function loadRuleForEdit(rule: BranchProtectionRulePublic) {
    setEditingId(rule.id);
    setPattern(rule.pattern);
    setRequireReviews(rule.require_reviews);
    setReviewCount(String(rule.required_approving_review_count || 1));
    setContexts(rule.required_status_contexts.join(", "));
    setStrict(rule.strict_status_checks);
    setDismissStale(rule.dismiss_stale_reviews);
    setRequireConversations(rule.require_conversation_resolution);
    setRequireLastPush(rule.require_last_push_approval);
    setAllowForce(rule.allow_force_pushes);
    setAllowDeletes(rule.allow_deletions);
    setEnforceAdmins(rule.enforce_admins);
    setLinearHistory(rule.required_linear_history);
    setLockBranch(rule.lock_branch);
    setError("");
  }

  async function saveRule() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    const count = Math.min(6, Math.max(1, Number.parseInt(reviewCount, 10) || 1));
    const ctxList = contexts
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const body = {
      owner,
      name,
      pattern: pattern.trim(),
      require_reviews: requireReviews,
      required_approving_review_count: count,
      dismiss_stale_reviews: dismissStale,
      require_conversation_resolution: requireConversations,
      require_last_push_approval: requireLastPush,
      required_status_contexts: ctxList,
      strict_status_checks: strict,
      allow_force_pushes: allowForce,
      allow_deletions: allowDeletes,
      enforce_admins: enforceAdmins,
      required_linear_history: linearHistory,
      lock_branch: lockBranch,
    };
    try {
      const res = editingId
        ? await apiClient.repo.branchProtection.update({ ...body, id: editingId })
        : await apiClient.repo.branchProtection.create(body);
      if (!res.ok) {
        setError(
          res.error.message || (editingId ? "Could not update rule." : "Could not create rule."),
        );
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: protectionQueryKey(owner, name) });
      toastSuccess(editingId ? "Branch protection rule updated." : "Branch protection rule added.");
      resetForm();
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function deleteRule(id: string) {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.branchProtection.delete({ owner, name, id });
      if (!res.ok) {
        setError(res.error.message || "Could not delete rule.");
        setPending(false);
        return;
      }
      if (editingId === id) resetForm();
      await queryClient.invalidateQueries({ queryKey: protectionQueryKey(owner, name) });
      toastSuccess("Branch protection rule deleted.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const rules = rulesQ.data ?? [];

  <>
    <section
      className="mt-12 flex flex-col gap-4"
      aria-labelledby="branch-protection-heading"
      data-testid="branch-protection-settings"
    >
      <h2
        id="branch-protection-heading"
        className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
      >
        Branch protection
      </h2>
      <p className="text-[14px] text-muted-foreground">
        Require pull request reviews and/or status checks before merging. Patterns support{" "}
        <code className="text-[13px]">*</code> and <code className="text-[13px]">?</code> wildcards.
      </p>

      @if (!canAdmin) {
        <p className="text-[14px] text-muted-foreground">
          Only repository admins can manage rules.
        </p>;
      } @else {
        <>
          @if (rulesQ.isPending) {
            <p className="text-[14px] text-muted-foreground">Loading rules…</p>;
          } @else {
            @if (rulesQ.isError) {
              <p role="alert" className="text-[14px] text-destructive">
                {
                  (rulesQ.error instanceof Error
                    ? rulesQ.error.message
                    : "Could not load branch protection.") as string
                }
              </p>;
            } @else {
              <>
                @if (rules.length === 0) {
                  <p
                    className="text-[14px] text-muted-foreground"
                    data-testid="bp-empty"
                  >
                    No branch protection rules yet.
                  </p>;
                } @else {
                  <ul className="flex flex-col gap-3" data-testid="bp-rule-list">
                    @for (const rule of rules; key rule.id) {
                      <li className="rounded-md border border-border p-3 text-[14px]">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <strong className="font-mono">
                            {rule.pattern as string}
                          </strong>
                          <div className="flex gap-1">
                            <Button
                              type="button"
                              variant="ghost"
                              disabled={pending}
                              onClick={() => loadRuleForEdit(rule)}
                            >
                              Edit
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              disabled={pending}
                              onClick={() => void deleteRule(rule.id)}
                            >
                              Delete
                            </Button>
                          </div>
                        </div>
                        <p className="mt-1 text-muted-foreground">
                          @if (
                            rule.require_reviews
                          ) {
                            <>
                              Require{" "}
                              {(
                                rule.required_approving_review_count as number
                              ).toString()}{" "}
                              approving review(s).
                            </>;
                          }
                          @if (
                            rule.required_status_contexts.length > 0
                          ) {
                            <>
                              {" "}
                              Checks:{" "}
                              {
                                rule.required_status_contexts.join(
                                  ", ",
                                ) as string
                              }
                              .
                            </>;
                          }
                        </p>
                      </li>;
                    }
                  </ul>;
                }

                <form
                  method="post"
                  action="#"
                  className="mt-4 flex flex-col gap-3 rounded-md border border-border p-4"
                  data-testid="bp-add-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void saveRule();
                  }}
                >
                  <h3 className="text-[16px] font-semibold text-foreground">
                    {(editingId ? "Edit rule" : "Add rule") as string}
                  </h3>
                  <div className="flex flex-col gap-1">
                    <Label for="bp-pattern">Branch name pattern</Label>
                    <Input
                      id="bp-pattern"
                      value={pattern}
                      onInput={(e) => setPattern((e.target as HTMLInputElement).value)}
                      placeholder="main or release/*"
                    />
                  </div>
                  <label className="flex items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={requireReviews}
                      onChange={(e) =>
                        setRequireReviews((e.target as HTMLInputElement).checked)
                      }
                    />
                    Require pull request reviews
                  </label>
                  @if (requireReviews) {
                    <div className="flex flex-col gap-1">
                      <Label for="bp-count">
                        Required approving reviews (1–6)
                      </Label>
                      <Input
                        id="bp-count"
                        type="number"
                        min="1"
                        max="6"
                        value={reviewCount}
                        onInput={(e) =>
                          setReviewCount((e.target as HTMLInputElement).value)
                        }
                      />
                    </div>;
                  }
                  <div className="flex flex-col gap-1">
                    <Label for="bp-contexts">
                      Required status contexts (comma-separated)
                    </Label>
                    <Input
                      id="bp-contexts"
                      value={contexts}
                      onInput={(e) => setContexts((e.target as HTMLInputElement).value)}
                      placeholder="ci/test, lint"
                    />
                  </div>
                  <div className="grid gap-2 sm:grid-cols-2">
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={strict}
                        onChange={(e) =>
                          setStrict((e.target as HTMLInputElement).checked)
                        }
                      />
                      Require branch up to date (strict)
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={dismissStale}
                        onChange={(e) =>
                          setDismissStale((e.target as HTMLInputElement).checked)
                        }
                      />
                      Dismiss stale reviews
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={requireConversations}
                        onChange={(e) =>
                          setRequireConversations(
                            (e.target as HTMLInputElement).checked,
                          )
                        }
                      />
                      Require conversation resolution
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={requireLastPush}
                        onChange={(e) =>
                          setRequireLastPush((e.target as HTMLInputElement).checked)
                        }
                      />
                      Require approval of the most recent push
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={allowForce}
                        onChange={(e) =>
                          setAllowForce((e.target as HTMLInputElement).checked)
                        }
                      />
                      Allow force pushes
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={allowDeletes}
                        onChange={(e) =>
                          setAllowDeletes((e.target as HTMLInputElement).checked)
                        }
                      />
                      Allow deletions
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={enforceAdmins}
                        onChange={(e) =>
                          setEnforceAdmins((e.target as HTMLInputElement).checked)
                        }
                      />
                      Do not allow bypassing (enforce admins)
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={linearHistory}
                        onChange={(e) =>
                          setLinearHistory((e.target as HTMLInputElement).checked)
                        }
                      />
                      Require linear history
                    </label>
                    <label className="flex items-center gap-2 text-[14px]">
                      <input
                        type="checkbox"
                        checked={lockBranch}
                        onChange={(e) =>
                          setLockBranch((e.target as HTMLInputElement).checked)
                        }
                      />
                      Lock branch
                    </label>
                  </div>
                  @if (error) {
                    <p role="alert" className="text-[14px] text-destructive">
                      {error as string}
                    </p>;
                  }
                  <div className="flex flex-wrap gap-2">
                    <Button type="submit" disabled={pending || !pattern.trim()}>
                      {
                        (pending
                          ? "Saving…"
                          : editingId
                            ? "Update rule"
                            : "Add rule") as string
                      }
                    </Button>
                    @if (editingId) {
                      <Button
                        type="button"
                        variant="ghost"
                        disabled={pending}
                        onClick={() => resetForm()}
                      >
                        Cancel
                      </Button>;
                    }
                  </div>
                </form>
              </>;
            }
          }
        </>;
      }
    </section>
    <TagProtectionPanel owner={owner} name={name} can_admin={canAdmin} />
  </>;
}

/** Repo Settings → Protected tags (GIT-21). */
export function TagProtectionPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const rulesQ = useQuery({
    queryKey: tagProtectionQueryKey(owner, name),
    queryFn: async (): Promise<TagProtectionRulePublic[]> => {
      const res = await apiClient.repo.tagProtection.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load tag protection.");
      return res.data.rules;
    },
    enabled: canAdmin,
  });

  const [pattern, setPattern] = useState("v*");
  const [allowCreate, setAllowCreate] = useState(false);
  const [allowUpdate, setAllowUpdate] = useState(false);
  const [allowDelete, setAllowDelete] = useState(false);
  const [enforceAdmins, setEnforceAdmins] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  function resetForm() {
    setEditingId(null);
    setPattern("v*");
    setAllowCreate(false);
    setAllowUpdate(false);
    setAllowDelete(false);
    setEnforceAdmins(false);
  }

  function loadRuleForEdit(rule: TagProtectionRulePublic) {
    setEditingId(rule.id);
    setPattern(rule.pattern);
    setAllowCreate(rule.allow_create);
    setAllowUpdate(rule.allow_update);
    setAllowDelete(rule.allow_delete);
    setEnforceAdmins(rule.enforce_admins);
    setError("");
  }

  async function saveRule() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    const body = {
      owner,
      name,
      pattern: pattern.trim(),
      allow_create: allowCreate,
      allow_update: allowUpdate,
      allow_delete: allowDelete,
      enforce_admins: enforceAdmins,
    };
    try {
      const res = editingId
        ? await apiClient.repo.tagProtection.update({ ...body, id: editingId })
        : await apiClient.repo.tagProtection.create(body);
      if (!res.ok) {
        setError(
          res.error.message || (editingId ? "Could not update rule." : "Could not create rule."),
        );
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: tagProtectionQueryKey(owner, name) });
      toastSuccess(editingId ? "Tag protection rule updated." : "Tag protection rule added.");
      resetForm();
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function deleteRule(id: string) {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.tagProtection.delete({ owner, name, id });
      if (!res.ok) {
        setError(res.error.message || "Could not delete rule.");
        setPending(false);
        return;
      }
      if (editingId === id) resetForm();
      await queryClient.invalidateQueries({ queryKey: tagProtectionQueryKey(owner, name) });
      toastSuccess("Tag protection rule deleted.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const rules = rulesQ.data ?? [];

  <section
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="tag-protection-heading"
    data-testid="tag-protection-settings"
  >
    <h2
      id="tag-protection-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Protected tags
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Restrict who can create, update, or delete tags matching a pattern. Patterns support{" "}
      <code className="text-[13px]">*</code> and <code className="text-[13px]">?</code> wildcards.
      Unchecked actions are blocked for everyone except admins.
    </p>

    @if (!canAdmin) {
      <p className="text-[14px] text-muted-foreground">
        Only repository admins can manage rules.
      </p>;
    } @else {
      <>
        @if (rulesQ.isPending) {
          <p className="text-[14px] text-muted-foreground">Loading rules…</p>;
        } @else {
          @if (rulesQ.isError) {
            <p role="alert" className="text-[14px] text-destructive">
              {
                (rulesQ.error instanceof Error
                  ? rulesQ.error.message
                  : "Could not load tag protection.") as string
              }
            </p>;
          } @else {
            <>
              @if (rules.length === 0) {
                <p
                  className="text-[14px] text-muted-foreground"
                  data-testid="tp-empty"
                >
                  No protected tag rules yet.
                </p>;
              } @else {
                <ul className="flex flex-col gap-3" data-testid="tp-rule-list">
                  @for (const rule of rules; key rule.id) {
                    <li className="rounded-md border border-border p-3 text-[14px]">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <strong className="font-mono">
                          {rule.pattern as string}
                        </strong>
                        <div className="flex gap-1">
                          <Button
                            type="button"
                            variant="ghost"
                            disabled={pending}
                            onClick={() => loadRuleForEdit(rule)}
                          >
                            Edit
                          </Button>
                          <Button
                            type="button"
                            variant="ghost"
                            disabled={pending}
                            onClick={() => void deleteRule(rule.id)}
                          >
                            Delete
                          </Button>
                        </div>
                      </div>
                      <p className="mt-1 text-muted-foreground">
                        {
                          ("Non-admin allowed actions: " +
                            ([
                              rule.allow_create ? "create" : null,
                              rule.allow_update ? "update" : null,
                              rule.allow_delete ? "delete" : null,
                            ]
                              .filter(Boolean)
                              .join(", ") || "none") +
                            (rule.enforce_admins
                              ? " · applies to admins too"
                              : "")) as string
                        }
                      </p>
                    </li>;
                  }
                </ul>;
              }

              <form
                method="post"
                action="#"
                className="mt-4 flex flex-col gap-3 rounded-md border border-border p-4"
                data-testid="tp-add-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void saveRule();
                }}
              >
                <h3 className="text-[16px] font-semibold text-foreground">
                  {(editingId ? "Edit tag rule" : "Add tag rule") as string}
                </h3>
                <div className="flex flex-col gap-1">
                  <Label for="tp-pattern">Tag name pattern</Label>
                  <Input
                    id="tp-pattern"
                    value={pattern}
                    onInput={(e) => setPattern((e.target as HTMLInputElement).value)}
                    placeholder="v* or release-?"
                  />
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  <label className="flex items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={allowCreate}
                      onChange={(e) =>
                        setAllowCreate((e.target as HTMLInputElement).checked)
                      }
                    />
                    Allow tag creation
                  </label>
                  <label className="flex items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={allowUpdate}
                      onChange={(e) =>
                        setAllowUpdate((e.target as HTMLInputElement).checked)
                      }
                    />
                    Allow tag updates (re-tag)
                  </label>
                  <label className="flex items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={allowDelete}
                      onChange={(e) =>
                        setAllowDelete((e.target as HTMLInputElement).checked)
                      }
                    />
                    Allow tag deletion
                  </label>
                  <label className="flex items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={enforceAdmins}
                      onChange={(e) =>
                        setEnforceAdmins((e.target as HTMLInputElement).checked)
                      }
                    />
                    Do not allow bypassing (enforce admins)
                  </label>
                </div>
                @if (error) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {error as string}
                  </p>;
                }
                <div className="flex flex-wrap gap-2">
                  <Button type="submit" disabled={pending || !pattern.trim()}>
                    {
                      (pending
                        ? "Saving…"
                        : editingId
                          ? "Update rule"
                          : "Add rule") as string
                    }
                  </Button>
                  @if (editingId) {
                    <Button
                      type="button"
                      variant="ghost"
                      disabled={pending}
                      onClick={() => resetForm()}
                    >
                      Cancel
                    </Button>;
                  }
                </div>
              </form>
            </>;
          }
        }
      </>;
    }
  </section>;
}
