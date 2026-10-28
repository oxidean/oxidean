import type {
  OxpilotSession,
  OxpilotSessionCreateRequest,
  OxpilotTargetView,
} from "@oxidean/api-client";
import { repoRefsQueryOptions } from "@oxidean/api-client";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { Check, ChevronDown } from "@octanejs/lucide";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import {
  invalidateOxpilotQueries,
  oxpilotSessionCreate,
  oxpilotTargetListQuery,
} from "@/lib/oxpilot-queries";
import { isBranchRef, shortRefName } from "@/lib/repo-browse";
import { repoListMineQueryOptions } from "@/lib/session-queries";

/** Branch `Select` sentinel — "use the repo default" ⇒ branch omitted. */
const BRANCH_DEFAULT = "__repo_default__";
/** Target `RadioGroup` sentinel — no computer-use binding. */
const TARGET_NONE = "__none__";

/**
 * M3 new-session dialog — repo `Select` (required), branch `Select`
 * (options from `repo.refs{owner,name}`, `refs/heads/*` shortened;
 * repository default ⇒ `branch` omitted so the server defaults), goal
 * `Textarea` (autofocus, optional), optional computer-use target
 * `RadioGroup` ("No target" default + `connected`-lifecycle rows only —
 * the client refuses a non-connected pick). Submit ⇒ `session_create`
 * (+ `target_id` per the flagged additive amendment — extra field is
 * ignored by today's server) → invalidate + `onCreated`. Errors render
 * inline above the actions; Esc/Cancel closes with no call.
 */
export function NewSessionDialog(props: {
  open: boolean;
  onClose: () => void;
  onCreated: (session: OxpilotSession) => void;
  /** Optional hook for the "[Connect new…]" shortcut into the ConnectDialog. */
  onConnectRequest?: () => void;
}) @{
  const qc = useQueryClient();
  const reposQ = useQuery(repoListMineQueryOptions());
  const repos = reposQ.data ?? [];
  const [repoKey, setRepoKey] = useState("");
  const [branch, setBranch] = useState("");
  const [goal, setGoal] = useState("");
  const [targetId, setTargetId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [repoOwner, repoName] = repoKey.split("/", 2);
  const selectedRepo = repos.find((r) => r.owner_username === repoOwner && r.name === repoName);

  const refsQ = useQuery({
    ...repoRefsQueryOptions(apiClient, { owner: repoOwner ?? "", name: repoName ?? "" }),
    enabled: props.open && !!repoOwner && !!repoName,
  });
  const branches = (refsQ.data?.refs ?? [])
    .filter((r) => isBranchRef(r.name))
    .map((r) => shortRefName(r.name));

  const targetsQ = useQuery(oxpilotTargetListQuery(props.open));
  const connectedTargets = (targetsQ.data ?? []).filter(
    (t: OxpilotTargetView) => t.lifecycle === "connected",
  );

  const createM = useMutation({
    mutationFn: (input: OxpilotSessionCreateRequest) => oxpilotSessionCreate(input),
    onSuccess: (session) => {
      invalidateOxpilotQueries(qc);
      props.onCreated(session);
      props.onClose();
    },
    onError: (e) => {
      setError(e instanceof Error ? e.message : "Could not create the session.");
    },
  });

  // Goal textarea autofocus — the pinned M3 focus target once the popup
  // is visible (keepMounted portals need the post-open tick).
  useEffect(() => {
    if (!props.open) return;
    const t = window.setTimeout(() => {
      (document.getElementById("oxpilot-new-goal") as HTMLTextAreaElement | null)?.focus();
    }, 0);
    return () => window.clearTimeout(t);
  }, [props.open]);

  function pickRepo(v: unknown) {
    const key = typeof v === "string" ? v : "";
    setRepoKey(key);
    const [o, n] = key.split("/", 2);
    const repo = repos.find((r) => r.owner_username === o && r.name === n);
    setBranch(repo?.default_branch ?? "");
  }

  function submit(ev: SubmitEvent) {
    ev.preventDefault();
    setError(null);
    const [owner, name] = repoKey.split("/", 2);
    if (!owner || !name) {
      setError("Pick a repository for the session.");
      return;
    }
    if (targetId) {
      const pick = connectedTargets.find((t) => t.id === targetId);
      if (!pick || pick.lifecycle !== "connected") {
        setError("Pick a connected computer-use target or no target.");
        return;
      }
    }
    const g = goal.trim();
    const req: OxpilotSessionCreateRequest & { target_id?: string } = {
      repo_owner: owner,
      repo_name: name,
      ...(branch.trim() ? { branch: branch.trim() } : {}),
      ...(g ? { goal: g } : {}),
      ...(targetId ? { target_id: targetId } : {}),
    };
    createM.mutate(req);
  }

  <Dialog
    open={props.open}
    onOpenChange={(next) => {
      if (createM.isPending) return;
      if (!next) {
        setError(null);
        props.onClose();
      }
    }}
  >
    <DialogContent
      data-testid="oxpilot-new-session"
      className="max-h-[min(90vh,42rem)] overflow-y-auto"
    >
      <DialogHeader>
        <DialogTitle>New session</DialogTitle>
      </DialogHeader>
      <form method="post" action="#" className="flex flex-col gap-4" onSubmit={submit}>
        <div className="flex flex-col gap-2">
          <Label htmlFor="oxpilot-repo">Repository</Label>
          <SelectRoot value={repoKey} onValueChange={pickRepo}>
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
          <Label htmlFor="oxpilot-branch">Branch</Label>
          <SelectRoot
            value={branch || BRANCH_DEFAULT}
            onValueChange={(v) => setBranch(typeof v === "string" && v !== BRANCH_DEFAULT ? v : "")}
          >
            <SelectTrigger id="oxpilot-branch" className="w-full max-w-md" aria-label="Branch">
              <SelectValue>
                {
                  (branch ||
                    `Repository default (${selectedRepo?.default_branch ?? "…"})`) as string
                }
              </SelectValue>
              <SelectIcon>
                <ChevronDown aria-hidden size={16} />
              </SelectIcon>
            </SelectTrigger>
            {/* octane-dom-race-ok: same anchor-scoped portal pattern as the
                repo Select above. */}
            <SelectPortal>
              <SelectPositioner sideOffset={4}>
                <SelectPopup className="min-w-[16rem]">
                  <SelectList>
                    <SelectItem value={BRANCH_DEFAULT}>
                      <SelectItemIndicator>
                        <Check aria-hidden size={16} className="text-primary" />
                      </SelectItemIndicator>
                      <SelectItemText>
                        {`Repository default (${selectedRepo?.default_branch ?? "…"})` as string}
                      </SelectItemText>
                    </SelectItem>
                    @for (const b of branches; key b) {
                      <SelectItem value={b}>
                        <SelectItemIndicator>
                          <Check aria-hidden size={16} className="text-primary" />
                        </SelectItemIndicator>
                        <SelectItemText>{b as string}</SelectItemText>
                      </SelectItem>;
                    }
                  </SelectList>
                </SelectPopup>
              </SelectPositioner>
            </SelectPortal>
          </SelectRoot>
          @if (repoKey && !refsQ.isPending && branches.length === 0) {
            <p className="text-[13px] text-muted-foreground">
              No branches found — the repository default will be used.
            </p>;
          }
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="oxpilot-new-goal">Goal (optional)</Label>
          <Textarea
            id="oxpilot-new-goal"
            rows={3}
            placeholder="What should Oxpilot work on?"
            value={goal}
            onInput={(e) => setGoal((e.target as HTMLTextAreaElement).value)}
          />
        </div>

        <div className="flex flex-col gap-2">
          <p className="text-[14px] font-normal leading-[1.4] text-foreground">
            Computer-use target (optional)
          </p>
          <RadioGroup
            value={targetId || TARGET_NONE}
            aria-label="Computer-use target"
            onValueChange={(v) => setTargetId(typeof v === "string" && v !== TARGET_NONE ? v : "")}
          >
            <label className="flex cursor-pointer items-start gap-3">
              <RadioGroupItem value={TARGET_NONE} className="mt-0.5" />
              <span className="text-[14px] text-foreground">No target</span>
            </label>
            @for (const t of connectedTargets; key t.id) {
              <label className="flex cursor-pointer items-start gap-3">
                <RadioGroupItem value={t.id} className="mt-0.5" />
                <span className="text-[14px] text-foreground">{t.label as string}</span>
              </label>;
            }
          </RadioGroup>
          @if (props.onConnectRequest) {
            <div>
              <Button
                type="button"
                variant="link"
                size="sm"
                className="h-auto px-0"
                onClick={() => props.onConnectRequest?.()}
              >
                Connect new…
              </Button>
            </div>;
          }
        </div>

        @if (error) {
          <AuthErrorBanner message={error} />;
        }
        <DialogFooter>
          <Button
            type="button"
            variant="secondary"
            disabled={createM.isPending}
            onClick={() => props.onClose()}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={createM.isPending || !repoKey}>
            {(createM.isPending ? "Starting…" : "Start session") as string}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>;
}
