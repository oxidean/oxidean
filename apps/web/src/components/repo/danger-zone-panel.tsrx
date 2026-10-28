import type { OwnerType, RepoPublic } from "@oxidean/api-client";

import { navigate as astroNavigate } from "astro:transitions/client";
import { useState } from "octane";
import { OwnerLookup } from "@/components/repo/owner-lookup";
import { TypeToConfirmDialog } from "@/components/repo/type-to-confirm-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { routeHref } from "@/lib/route-params";
import { useRepoStore } from "@/lib/repo-store";
import { toastError, toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const SOFT_DELETE_BODY =
  "Removes this repository from Oxidean. Files may be purged from disk later. Type the repository name to confirm.";
const TRANSFER_BODY =
  "Transfers this repository to another user or organization. Type the repository name to confirm.";
const ARCHIVE_BODY =
  "Marks this repository read-only. Code stays browsable and clonable; pushes, issues, pull requests, and releases are disabled until it is unarchived. Type the repository name to confirm.";
const UNARCHIVE_BODY =
  "Restores write access. Pushes, issues, pull requests, and releases become editable again. Type the repository name to confirm.";

/** Repo Settings → Danger zone — rename, transfer, archive, soft-delete. */
export function DangerZonePanel(props: {
  owner: string;
  name: string;
  repo: RepoPublic;
}) @{
  const owner = props.owner;
  const repoName = props.name;
  const repo = props.repo;
  const setRepo = useRepoStore((s) => s.setRepo);

  const [newName, setNewName] = useState("");
  const [renamePending, setRenamePending] = useState(false);

  const [destOwner, setDestOwner] = useState("");
  const [destOwnerType, setDestOwnerType] = useState<OwnerType>("user");
  const [transferOpen, setTransferOpen] = useState(false);
  const [transferConfirm, setTransferConfirm] = useState("");
  const [transferPending, setTransferPending] = useState(false);
  const [transferError, setTransferError] = useState("");

  const [archived, setArchived] = useState(() => !!repo.archived);
  const [archiveOpen, setArchiveOpen] = useState(false);
  const [archiveConfirm, setArchiveConfirm] = useState("");
  const [archivePending, setArchivePending] = useState(false);
  const [archiveError, setArchiveError] = useState("");

  const [deleteOpen, setDeleteOpen] = useState(false);
  const [confirmName, setConfirmName] = useState("");
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");

  async function submitRename() {
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
      await astroNavigate(
        routeHref("/$owner/$repo/settings/danger", {
          owner: res.data.repo.owner_username,
          repo: res.data.repo.name,
        }),
      );
    } catch {
      toastError(NETWORK_ERROR);
      setRenamePending(false);
    }
  }

  async function submitTransfer() {
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
      void astroNavigate(`/${nextOwner}/${nextName}/settings/danger`);
    } catch {
      setTransferError(NETWORK_ERROR);
      setTransferPending(false);
    }
  }

  async function submitArchive() {
    const next = !archived;
    setArchiveError("");
    setArchivePending(true);
    try {
      const res = await apiClient.repo.setArchived({ owner, name: repoName, archived: next });
      if (!res.ok) {
        setArchiveError(res.error.message || "Could not update archive state.");
        setArchivePending(false);
        return;
      }
      setArchivePending(false);
      setArchiveOpen(false);
      setArchiveConfirm("");
      setArchived(res.data.archived ?? next);
      setRepo(res.data);
      toastSuccess(next ? "Repository archived." : "Repository unarchived.");
    } catch {
      setArchiveError(NETWORK_ERROR);
      setArchivePending(false);
    }
  }

  async function submitDelete() {
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
      void astroNavigate("/");
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  <section
    className="flex flex-col gap-6 rounded-md border border-destructive/40 p-4"
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
        Move this repository to another user or organization. You must type the repository name to
        confirm.
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

    <div className="flex flex-col gap-3" aria-labelledby="archive-heading">
      <h3 id="archive-heading" className="text-[16px] font-semibold text-foreground">
        {(archived ? "Unarchive repository" : "Archive repository") as string}
      </h3>
      <p className="text-[14px] leading-[1.4] text-muted-foreground">
        {
          (archived
            ? "Restore writes — pushes, issues, pull requests, and releases become editable again."
            : "Make this repository read-only. Code stays browsable and clonable; pushes, issues, pull requests, and releases are disabled until unarchived.") as string
        }
      </p>
      <Button
        type="button"
        variant={archived ? "secondary" : "destructive"}
        className="w-full sm:w-auto"
        disabled={archivePending}
        onClick={() => {
          setArchiveError("");
          setArchiveConfirm("");
          setArchiveOpen(true);
        }}
      >
        {(archived ? "Unarchive repository" : "Archive repository") as string}
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
      open={archiveOpen}
      title={(archived ? "Unarchive repository?" : "Archive repository?") as string}
      description={(archived ? UNARCHIVE_BODY : ARCHIVE_BODY) as string}
      confirmLabel="Repository name"
      confirmValue={archiveConfirm}
      expectedName={repo.name}
      onConfirmValueChange={setArchiveConfirm}
      pending={archivePending}
      error={archiveError}
      confirmButtonLabel={(archived ? "Unarchive repository" : "Archive repository") as string}
      pendingButtonLabel={(archived ? "Unarchiving…" : "Archiving…") as string}
      inputId="archive-confirm"
      testId="repo-archive-confirm"
      onOpenChange={(open) => {
        if (archivePending) return;
        setArchiveOpen(open);
        if (!open) {
          setArchiveConfirm("");
          setArchiveError("");
        }
      }}
      onConfirm={() => void submitArchive()}
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
  </section>;
}
