import { createFileRoute, useLoaderData, useNavigate, useParams } from "@octanejs/tanstack-router";
import type { OwnerType, RepoVisibility } from "@oxidean/api-client";
import { useState } from "octane";
import { CollaboratorsPanel } from "@/components/repo/collaborators-panel";
import { BranchProtectionPanel } from "@/components/repo/branch-protection-panel";
import { LfsBrowser } from "@/components/repo/lfs-browser";
import { ActionsSettingsPanel } from "@/components/repo/actions-settings-panel";
import { LfsSettingsPanel } from "@/components/repo/lfs-settings-panel";
import { TemplateRepoSettingsPanel } from "@/components/repo/template-repo-settings-panel";
import { MergeSettingsPanel } from "@/components/repo/merge-settings-panel";
import { OwnerLookup } from "@/components/repo/owner-lookup";
import { TopicChipsInput } from "@/components/repo/topics-input";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoFeaturesPanel } from "@/components/repo/repo-features-panel";
import { WebhooksPanel } from "@/components/repo/webhooks-panel";
import { MirrorSettingsPanel } from "@/components/repo/mirror-settings-panel";
import { TypeToConfirmDialog } from "@/components/repo/type-to-confirm-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { type RepoLayoutLoaderData, useRepoStore } from "@/lib/repo-store";
import { toastError, toastSuccess } from "@/lib/toast";

export const Route = createFileRoute("/$owner/$repo/settings")({
  component: RepoSettingsPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Settings · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
});

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PRIVATE_HELPER = "Only people with access can see this private repository.";
const SOFT_DELETE_BODY =
  "Removes this repository from Oxidean. Files may be purged from disk later. Type the repository name to confirm.";
const TRANSFER_BODY =
  "Transfers this repository to another user or organization. Type the repository name to confirm.";

/** Admin settings — visibility, collaborators, rename/transfer, soft-delete (D-ORG-04 / GIT-16/17). */
export function RepoSettingsPage() @{
  const params = useParams({ from: "/$owner/$repo/settings" }) as {
    owner: string;
    repo: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = useNavigate();
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const setRepo = useRepoStore((s) => s.setRepo);

  const repo = layout?.repo ?? null;
  const layoutError = layout?.status === "error" ? layout.message : "";
  // Gate on API can_admin — not me.id === owner_id (D-ORG-04 / T-10-02).
  const canAdmin = !!repo?.can_admin;
  const showNotFound =
    !layout || layout.status === "not_found" || (layout.status === "ok" && !!repo && !canAdmin);

  const [visibility, setVisibility] = useState<RepoVisibility>(() => repo?.visibility ?? "public");
  const [visPending, setVisPending] = useState(false);
  const [description, setDescription] = useState(() => repo?.description ?? "");
  const [homepage, setHomepage] = useState(() => repo?.homepage ?? "");
  const [editTopics, setEditTopics] = useState<string[]>(() => repo?.topics ?? []);
  const [metaPending, setMetaPending] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [confirmName, setConfirmName] = useState("");
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");

  const [newName, setNewName] = useState("");
  const [renamePending, setRenamePending] = useState(false);

  const [destOwner, setDestOwner] = useState("");
  const [destOwnerType, setDestOwnerType] = useState<OwnerType>("user");
  const [transferOpen, setTransferOpen] = useState(false);
  const [transferConfirm, setTransferConfirm] = useState("");
  const [transferPending, setTransferPending] = useState(false);
  const [transferError, setTransferError] = useState("");

  async function saveVisibility() {
    if (!repo) return;
    setVisPending(true);
    try {
      const res = await apiClient.repo.updateVisibility({
        owner,
        name: repoName,
        visibility,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not update visibility.");
        setVisPending(false);
        return;
      }
      setVisibility(res.data.visibility);
      setRepo(res.data);
      toastSuccess("Visibility updated.");
      setVisPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setVisPending(false);
    }
  }

  async function saveMetadata() {
    if (!repo) return;
    setMetaPending(true);
    try {
      const res = await apiClient.repo.updateMetadata({
        owner,
        name: repoName,
        description,
        homepage: homepage.trim(),
        topics: editTopics,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not update repository details.");
        setMetaPending(false);
        return;
      }
      setDescription(res.data.description ?? "");
      setHomepage(res.data.homepage ?? "");
      setEditTopics(res.data.topics ?? []);
      setRepo(res.data);
      toastSuccess("Repository details updated.");
      setMetaPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setMetaPending(false);
    }
  }

  async function submitRename() {
    if (!repo) return;
    const trimmed = newName.trim();
    if (!trimmed || trimmed === repo.name) return;
    setRenamePending(true);
    try {
      const res = await apiClient.repo.rename({
        owner,
        name: repoName,
        newName: trimmed,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not rename repository.");
        setRenamePending(false);
        return;
      }
      setRepo(res.data.repo);
      toastSuccess("Repository renamed.");
      await navigate({
        to: "/$owner/$repo/settings",
        params: {
          owner: res.data.repo.owner_username,
          repo: res.data.repo.name,
        },
      });
    } catch {
      toastError(NETWORK_ERROR);
      setRenamePending(false);
    }
  }

  async function submitTransfer() {
    if (!repo) return;
    setTransferError("");
    setTransferPending(true);
    try {
      const res = await apiClient.repo.transfer({
        owner,
        name: repoName,
        destOwner: destOwner.trim(),
        destOwnerType,
        confirmName: transferConfirm.trim(),
      });
      if (!res.ok) {
        setTransferError(res.error.message || "Could not transfer repository.");
        setTransferPending(false);
        return;
      }
      const nextOwner = res.data.repo.owner_username;
      const nextName = res.data.repo.name;
      setTransferPending(false);
      setTransferOpen(false);
      toastSuccess("Repository transferred.");
      window.location.assign(`/${nextOwner}/${nextName}/settings`);
    } catch {
      setTransferError(NETWORK_ERROR);
      setTransferPending(false);
    }
  }

  async function submitDelete() {
    if (!repo) return;
    setDeleteError("");
    setDeletePending(true);
    try {
      const res = await apiClient.repo.softDelete({
        owner,
        name: repoName,
        confirmName: confirmName.trim(),
      });
      if (!res.ok) {
        setDeleteError(res.error.message || "Could not delete repository.");
        setDeletePending(false);
        return;
      }
      setDeletePending(false);
      setDeleteOpen(false);
      window.location.assign("/");
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  @if (showNotFound) {
    <RepoNotFound />;
  } @else if (layoutError) {
    <div className="max-w-lg py-6 text-center">
      <p role="alert" className="text-[16px] text-destructive">
        {layoutError as string}
      </p>
    </div>;
  } @else if (repo && canAdmin) {
    <>
      <div className="max-w-3xl">
        <section className="mt-8 flex flex-col gap-4" aria-labelledby="general-heading">
          <h2
            id="general-heading"
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
          >
            General
          </h2>
          <p className="text-[14px] text-muted-foreground">
            Description, homepage, and topics shown on the Code home About panel.
          </p>
          <div className="flex flex-col gap-2">
            <Label htmlFor="repo-description">Description</Label>
            <Textarea
              id="repo-description"
              value={description}
              onInput={(e) => setDescription((e.currentTarget as HTMLTextAreaElement).value)}
              disabled={metaPending}
              rows={3}
              maxLength={350}
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="repo-homepage">Homepage</Label>
            <Input
              id="repo-homepage"
              value={homepage}
              onInput={(e) => setHomepage((e.currentTarget as HTMLInputElement).value)}
              disabled={metaPending}
              placeholder="https://example.com"
              autoComplete="off"
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="repo-topics">Topics</Label>
            <TopicChipsInput
              id="repo-topics"
              topics={editTopics}
              onTopicsChange={setEditTopics}
              disabled={metaPending}
              placeholder="Add topics…"
            />
            <p className="text-[13px] text-muted-foreground">
              Lowercase slugs (letters, digits, hyphens) — comma or Enter adds a topic.
            </p>
          </div>
          <Button
            type="button"
            disabled={metaPending}
            onClick={() => void saveMetadata()}
            className="w-full sm:w-auto"
          >
            {(metaPending ? "Saving…" : "Save details") as string}
          </Button>
        </section>

        <section className="mt-8 flex flex-col gap-4" aria-labelledby="vis-heading">
          <h2
            id="vis-heading"
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
          >
            Visibility
          </h2>
          <p className="text-[14px] text-muted-foreground">Choose who can see this repository.</p>
          <div className="flex flex-wrap gap-2" role="group" aria-label="Visibility">
            <Button
              type="button"
              variant={visibility === "public" ? "default" : "secondary"}
              aria-pressed={visibility === "public"}
              disabled={visPending}
              onClick={() => setVisibility("public")}
            >
              Public
            </Button>
            <Button
              type="button"
              variant={visibility === "private" ? "default" : "secondary"}
              aria-pressed={visibility === "private"}
              disabled={visPending}
              onClick={() => setVisibility("private")}
            >
              Private
            </Button>
          </div>
          @if (visibility === "private") {
            <p className="text-[14px] text-muted-foreground">{PRIVATE_HELPER as string}</p>;
          }
          <Button
            type="button"
            disabled={visPending || visibility === repo.visibility}
            onClick={() => void saveVisibility()}
            className="w-full sm:w-auto"
          >
            {(visPending ? "Saving…" : "Save visibility") as string}
          </Button>
        </section>

        <CollaboratorsPanel owner={owner} name={repoName} />

        <BranchProtectionPanel owner={owner} name={repoName} can_admin={canAdmin} />

        <LfsSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
        <TemplateRepoSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />
        <RepoFeaturesPanel owner={owner} name={repoName} can_admin={canAdmin} repo={repo} />
        <ActionsSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />

        <MergeSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />

        <WebhooksPanel owner={owner} name={repoName} can_admin={canAdmin} />
        <MirrorSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} />

        <LfsBrowser owner={owner} name={repoName} />

        <section
          className="mt-12 flex flex-col gap-6 rounded-md border border-destructive/40 p-4"
          aria-labelledby="danger-heading"
        >
          <h2
            id="danger-heading"
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-destructive"
          >
            Danger zone
          </h2>

          <div className="flex flex-col gap-3" aria-labelledby="rename-heading">
            <h3 id="rename-heading" className="text-[16px] font-semibold text-foreground">
              Rename repository
            </h3>
            <p className="text-[14px] leading-[1.4] text-muted-foreground">
              Change the repository name. Old URLs redirect for a retention period.
            </p>
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-rename-name">New name</Label>
              <Input
                id="repo-rename-name"
                value={newName}
                onInput={(e) => setNewName((e.currentTarget as HTMLInputElement).value)}
                autoComplete="off"
                disabled={renamePending}
                placeholder={repo.name as string}
              />
            </div>
            <Button
              type="button"
              variant="destructive"
              className="w-full sm:w-auto"
              disabled={renamePending || !newName.trim() || newName.trim() === repo.name}
              onClick={() => void submitRename()}
            >
              {(renamePending ? "Renaming…" : "Rename repository") as string}
            </Button>
          </div>

          <div className="flex flex-col gap-3" aria-labelledby="transfer-heading">
            <h3 id="transfer-heading" className="text-[16px] font-semibold text-foreground">
              Transfer ownership
            </h3>
            <p className="text-[14px] leading-[1.4] text-muted-foreground">
              Move this repository to another user or organization. You must type the repository
              name to confirm.
            </p>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Destination type">
              <Button
                type="button"
                variant={destOwnerType === "user" ? "default" : "secondary"}
                aria-pressed={destOwnerType === "user"}
                disabled={transferPending}
                onClick={() => {
                  setDestOwnerType("user");
                  setDestOwner("");
                }}
              >
                User
              </Button>
              <Button
                type="button"
                variant={destOwnerType === "org" ? "default" : "secondary"}
                aria-pressed={destOwnerType === "org"}
                disabled={transferPending}
                onClick={() => {
                  setDestOwnerType("org");
                  setDestOwner("");
                }}
              >
                Organization
              </Button>
            </div>
            <OwnerLookup
              id="repo-transfer-dest"
              label="Destination owner"
              ownerType={destOwnerType}
              value={destOwner}
              onValueChange={setDestOwner}
              disabled={transferPending}
            />
            <Button
              type="button"
              variant="destructive"
              className="w-full sm:w-auto"
              disabled={transferPending || !destOwner.trim()}
              onClick={() => {
                setTransferError("");
                setTransferConfirm("");
                setTransferOpen(true);
              }}
            >
              Transfer repository
            </Button>
          </div>

          <div className="flex flex-col gap-3">
            <p className="text-[14px] leading-[1.4] text-muted-foreground">
              Soft-delete removes this repository from Oxidean. Disk cleanup may run later.
            </p>
            <Button
              type="button"
              variant="destructive"
              className="w-full sm:w-auto"
              onClick={() => {
                setDeleteError("");
                setConfirmName("");
                setDeleteOpen(true);
              }}
            >
              Delete repository
            </Button>
          </div>
        </section>
      </div>

      <TypeToConfirmDialog
        open={transferOpen}
        title="Transfer repository?"
        description={TRANSFER_BODY}
        confirmLabel="Repository name"
        confirmValue={transferConfirm}
        expectedName={repo.name}
        onConfirmValueChange={setTransferConfirm}
        pending={transferPending}
        error={transferError}
        confirmButtonLabel="Transfer repository"
        pendingButtonLabel="Transferring…"
        inputId="transfer-confirm"
        testId="repo-transfer-confirm"
        onOpenChange={(open) => {
          if (transferPending) return;
          setTransferOpen(open);
          if (!open) {
            setTransferConfirm("");
            setTransferError("");
          }
        }}
        onConfirm={() => void submitTransfer()}
      />

      <TypeToConfirmDialog
        open={deleteOpen}
        title="Delete repository?"
        description={SOFT_DELETE_BODY}
        confirmLabel="Repository name"
        confirmValue={confirmName}
        expectedName={repo.name}
        onConfirmValueChange={setConfirmName}
        pending={deletePending}
        error={deleteError}
        confirmButtonLabel="Delete repository"
        pendingButtonLabel="Deleting…"
        inputId="soft-delete-confirm"
        testId="repo-delete-confirm"
        onOpenChange={(open) => {
          if (deletePending) return;
          setDeleteOpen(open);
          if (!open) {
            setConfirmName("");
            setDeleteError("");
          }
        }}
        onConfirm={() => void submitDelete()}
      />
    </>;
  }
}
