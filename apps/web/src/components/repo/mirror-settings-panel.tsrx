import type { RepoMirrorPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function mirrorQueryKey(owner: string, name: string) {
  return ["repo", "mirror", owner, name] as const;
}

async function copyToClipboard(text: string): Promise<"copied" | "error"> {
  try {
    if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
    await navigator.clipboard.writeText(text);
    return "copied";
  } catch {
    return "error";
  }
}

function CopyField(props: {
  label: string;
  value: string;
  ariaLabel: string;
  testId?: string;
}) @{
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  async function onCopy() {
    const next = await copyToClipboard(props.value);
    setCopyState(next);
    if (next === "copied") {
      window.setTimeout(() => setCopyState("idle"), 2000);
    }
  }

  <div className="flex flex-col gap-2" data-testid={props.testId}>
    <Label>{props.label as string}</Label>
    <div className="flex flex-col gap-2 sm:flex-row sm:items-stretch">
      <Input
        readOnly
        value={props.value as string}
        aria-label={props.ariaLabel as string}
        className="min-w-0 flex-1 font-mono text-[13px]"
        onFocus={(e) => {
          const el = e.currentTarget as HTMLInputElement;
          el.select();
        }}
      />
      <Button
        type="button"
        variant="secondary"
        className="shrink-0"
        disabled={!props.value}
        onClick={() => void onCopy()}
      >
        {(copyState === "copied" ? "Copied" : "Copy") as string}
      </Button>
    </div>
    @if (copyState === "error") {
      <p role="alert" className="text-[13px] text-destructive">
        Couldn’t copy. Select the value and copy manually.
      </p>;
    }
  </div>;
}

function WebhookSecretReveal(props: { secret: string; onDone: () => void }) @{
  const secret = props.secret;
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  async function copySecret() {
    const next = await copyToClipboard(secret);
    setCopyState(next);
    if (next === "copied") {
      window.setTimeout(() => setCopyState("idle"), 2000);
    }
  }

  <div
    className="rounded-md border border-destructive/40 bg-card p-4"
    data-testid="mirror-webhook-secret-reveal"
  >
    <h4 className="text-[15px] font-semibold text-foreground">Copy your inbound webhook secret</h4>
    <p className="mt-1 text-[14px] text-muted-foreground">
      You won’t see this value again. Paste it into the Push webhook on the remote forge.
    </p>
    <div className="mt-4 flex flex-col gap-3 sm:flex-row sm:items-stretch">
      <Input
        readOnly
        value={secret as string}
        aria-label="Inbound webhook secret"
        className="min-w-0 flex-1 font-mono text-[13px]"
        onFocus={(e) => {
          const el = e.currentTarget as HTMLInputElement;
          el.select();
        }}
      />
      <Button
        type="button"
        variant="secondary"
        className="shrink-0"
        onClick={() => void copySecret()}
      >
        {(copyState === "copied" ? "Copied" : "Copy secret") as string}
      </Button>
    </div>
    @if (copyState === "error") {
      <p role="alert" className="mt-2 text-[14px] text-destructive">
        Couldn’t copy. Select the secret and copy manually.
      </p>;
    }
    <Button type="button" className="mt-4 w-full sm:w-auto" onClick={props.onDone}>
      Done
    </Button>
  </div>;
}

/** Repo Settings — two-way mirror remote (HTTPS/SSH), webhook, Sync Now. */
export function MirrorSettingsPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const mirrorQ = useQuery({
    queryKey: mirrorQueryKey(owner, name),
    queryFn: async (): Promise<RepoMirrorPublic | null> => {
      const res = await apiClient.repo.mirror.get({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load mirror settings.");
      return res.data.mirror ?? null;
    },
    enabled: canAdmin,
  });

  const existing = mirrorQ.data ?? null;

  const [remoteUrl, setRemoteUrl] = useState("");
  const [authKind, setAuthKind] = useState<"https_token" | "ssh_key">("https_token");
  const [username, setUsername] = useState("git");
  const [secret, setSecret] = useState("");
  const [knownHosts, setKnownHosts] = useState("");
  const [pollSecs, setPollSecs] = useState("60");
  const [enabled, setEnabled] = useState(true);
  const [syncMode, setSyncMode] = useState<"merge" | "exact">("merge");
  const [pending, setPending] = useState(false);
  const [syncPending, setSyncPending] = useState(false);
  const [fetchKhPending, setFetchKhPending] = useState(false);
  const [error, setError] = useState("");
  const [revealedWebhook, setRevealedWebhook] = useState("");

  useEffect(() => {
    if (!existing) return;
    setRemoteUrl(existing.remote_url);
    setAuthKind(existing.auth_kind === "ssh_key" ? "ssh_key" : "https_token");
    setUsername(existing.username || "git");
    setKnownHosts(existing.known_hosts || "");
    setPollSecs(String(existing.poll_interval_secs));
    setEnabled(existing.enabled);
    setSyncMode(existing.sync_mode === "exact" ? "exact" : "merge");
  }, [existing?.id, existing?.updated_at]);

  async function save() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const poll = Number.parseInt(pollSecs, 10);
      const res = await apiClient.repo.mirror.upsert({
        owner,
        name,
        remote_url: remoteUrl.trim(),
        auth_kind: authKind,
        username: username.trim() || "git",
        secret: secret.trim() || null,
        known_hosts: knownHosts.trim() || null,
        poll_interval_secs: Number.isFinite(poll) ? poll : 60,
        enabled,
        sync_mode: syncMode,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not save mirror.");
        setPending(false);
        return;
      }
      if (res.data.webhook_secret) {
        setRevealedWebhook(res.data.webhook_secret);
      }
      setSecret("");
      await queryClient.invalidateQueries({ queryKey: mirrorQueryKey(owner, name) });
      toastSuccess("Mirror settings saved.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function syncNow() {
    if (!canAdmin || !existing) return;
    setError("");
    setSyncPending(true);
    try {
      const res = await apiClient.repo.mirror.syncNow({ owner, name });
      if (!res.ok) {
        setError(res.error.message || "Could not enqueue sync.");
        setSyncPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: mirrorQueryKey(owner, name) });
      toastSuccess("Two-way sync enqueued.");
      setSyncPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setSyncPending(false);
    }
  }

  async function generateKey() {
    if (!canAdmin || !existing) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.mirror.generateSshKey({ owner, name });
      if (!res.ok) {
        setError(res.error.message || "Could not generate SSH key.");
        setPending(false);
        return;
      }
      setAuthKind("ssh_key");
      await queryClient.invalidateQueries({ queryKey: mirrorQueryKey(owner, name) });
      toastSuccess("Deploy key generated — copy the public key onto the remote.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function fetchHostKey() {
    if (!canAdmin) return;
    const url = remoteUrl.trim();
    if (!url) {
      setError("Enter an SSH remote URL first (git@host:path or ssh://…).");
      return;
    }
    setError("");
    setFetchKhPending(true);
    try {
      const res = await apiClient.repo.mirror.fetchHostKey({
        owner,
        name,
        remote_url: url,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not fetch host key.");
        setFetchKhPending(false);
        return;
      }
      setKnownHosts(res.data.known_hosts.trim());
      toastSuccess(`Host key for ${res.data.host} loaded — save to pin it.`);
      setFetchKhPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setFetchKhPending(false);
    }
  }

  async function rotateWebhook() {
    if (!canAdmin || !existing) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.mirror.rotateWebhookSecret({ owner, name });
      if (!res.ok) {
        setError(res.error.message || "Could not rotate webhook secret.");
        setPending(false);
        return;
      }
      setRevealedWebhook(res.data.webhook_secret);
      await queryClient.invalidateQueries({ queryKey: mirrorQueryKey(owner, name) });
      toastSuccess("Webhook secret rotated — copy it now.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function removeMirror() {
    if (!canAdmin || !existing) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.mirror.delete({ owner, name });
      if (!res.ok) {
        setError(res.error.message || "Could not remove mirror.");
        setPending(false);
        return;
      }
      setRemoteUrl("");
      setSecret("");
      setKnownHosts("");
      setRevealedWebhook("");
      setAuthKind("https_token");
      await queryClient.invalidateQueries({ queryKey: mirrorQueryKey(owner, name) });
      toastSuccess("Mirror removed.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const canSave =
    !!remoteUrl.trim() &&
    (authKind === "https_token"
      ? !!(existing?.has_secret || secret.trim())
      : !!(knownHosts.trim() && (existing?.has_secret || secret.trim() || existing)));

  const canSaveSshBootstrap =
    authKind === "ssh_key" &&
    !!remoteUrl.trim() &&
    !!knownHosts.trim() &&
    !existing &&
    !secret.trim();

  const saveEnabled = canSave || canSaveSshBootstrap;

  @if (!canAdmin) {
    <></>;
  } @else {
    <section
      className="mt-12 flex flex-col gap-4"
      aria-labelledby="mirror-heading"
      data-testid="repo-mirror-settings"
    >
      <h2
        id="mirror-heading"
        className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
      >
        Two-way mirror
      </h2>
      <p className="text-[14px] text-muted-foreground">
        Sync this repository with one external git remote. Diverged branches merge; tags never
        overwrite. Sync runs after local git activity and inbound push webhooks.
      </p>

      @if (mirrorQ.isLoading) {
        <p className="text-[14px] text-muted-foreground">Loading mirror settings…</p>;
      }

      @if (error) {
        <p className="text-[14px] text-destructive" role="alert">
          {error as string}
        </p>;
      }

      @if (revealedWebhook) {
        <WebhookSecretReveal
          secret={revealedWebhook}
          onDone={() => setRevealedWebhook("")}
        />;
      }

      @if (!existing && !mirrorQ.isLoading) {
        <ol className="list-decimal space-y-2 pl-5 text-[14px] leading-relaxed text-muted-foreground">
          <li>
            <span className="font-medium text-foreground">Paste the remote clone URL</span>
            {" — "}
            HTTPS or{" "}
            <code className="rounded bg-muted px-1 font-mono text-[13px]">git@…</code>.
          </li>
          <li>
            <span className="font-medium text-foreground">Choose authentication</span>
            {" — "}
            HTTPS token (recommended) or an SSH deploy key.
          </li>
          <li>
            <span className="font-medium text-foreground">
              Save, then finish on the remote
            </span>
            {" — "}
            Register the Push webhook (and deploy key if SSH), then Synchronize now.
          </li>
        </ol>;
      }

      @if (existing) {
        <>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="text-[14px] text-foreground">
              <p>
                Status: <strong>{existing.last_status as string}</strong>
                @if (existing.last_synced_at) {
                  <span className="text-muted-foreground">
                    {" "}
                    · Last synced {existing.last_synced_at as string}
                  </span>;
                }
              </p>
              @if (existing.last_error) {
                <p className="mt-1 text-[14px] text-destructive" role="alert">
                  {existing.last_error as string}
                </p>;
              }
            </div>
            <Button
              type="button"
              disabled={syncPending || pending}
              onClick={() => void syncNow()}
            >
              {(syncPending ? "Enqueueing…" : "Synchronize now") as string}
            </Button>
          </div>
          @if (existing.ref_results.length > 0) {
            <ul className="list-inside list-disc text-[13px] text-muted-foreground">
              @for (const r of existing.ref_results; key r.refname) {
                <li>
                  {r.refname as string}: {r.outcome as string}
                  @if (r.detail) {
                    <span> — {r.detail as string}</span>;
                  }
                </li>;
              }
            </ul>;
          }
        </>;
      }

      <div className="flex flex-col gap-2">
        <Label htmlFor="mirror-url">Remote URL</Label>
        <Input
          id="mirror-url"
          value={remoteUrl}
          onInput={(e) => setRemoteUrl((e.currentTarget as HTMLInputElement).value)}
          placeholder="https://github.com/org/repo.git or git@github.com:org/repo.git"
          disabled={pending}
          autoComplete="off"
        />
        <p className="text-[13px] text-muted-foreground">
          Clone URL of the external repository to keep in sync.
        </p>
      </div>

      <div className="flex flex-col gap-3" data-testid="mirror-sync-mode">
        <p className="text-[14px] font-medium text-foreground">Sync mode</p>
        <RadioGroup
          value={syncMode}
          disabled={pending}
          onValueChange={(v) => {
            if (v === "merge" || v === "exact") setSyncMode(v);
          }}
          className="gap-3"
          aria-label="Sync mode"
        >
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem value="merge" className="mt-0.5" data-testid="mirror-sync-mode-merge" />
            <span className="flex flex-col gap-0.5">
              <span className="text-[14px] font-medium text-foreground">Merge (safe)</span>
              <span className="text-[13px] text-muted-foreground">
                Fast-forward or merge diverged branches. Protected tips open a PR. Deletes are not
                propagated.
              </span>
            </span>
          </label>
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem value="exact" className="mt-0.5" data-testid="mirror-sync-mode-exact" />
            <span className="flex flex-col gap-0.5">
              <span className="text-[14px] font-medium text-foreground">Exact sync (1:1)</span>
              <span className="text-[13px] text-muted-foreground">
                Force-update tips with last-writer-wins and propagate deletes both ways. Can
                overwrite history. Does not open mirror helper PRs.
              </span>
            </span>
          </label>
        </RadioGroup>
        @if (syncMode === "exact") {
          <p
            className="rounded-md border border-border bg-muted/40 px-3 py-2 text-[13px] text-muted-foreground"
            data-testid="mirror-exact-warning"
          >
            Exact sync can force-push and delete branches on both sides. Use only when you
            want true one-to-one mirroring.
          </p>;
        }
      </div>

      <div className="flex flex-col gap-3">
        <p className="text-[14px] font-medium text-foreground">Authentication</p>
        <RadioGroup
          value={authKind}
          disabled={pending}
          onValueChange={(v) => {
            if (v === "https_token" || v === "ssh_key") setAuthKind(v);
          }}
          className="gap-3"
          aria-label="Authentication"
        >
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem
              value="https_token"
              className="mt-0.5"
              data-testid="mirror-auth-kind-https"
            />
            <span className="flex flex-col gap-0.5">
              <span className="text-[14px] font-medium text-foreground">HTTPS token</span>
              <span className="text-[13px] text-muted-foreground">
                Recommended for GitHub and GitLab. Uses a remote forge PAT with write access.
              </span>
            </span>
          </label>
          <label className="flex cursor-pointer items-start gap-3">
            <RadioGroupItem value="ssh_key" className="mt-0.5" data-testid="mirror-auth-kind-ssh" />
            <span className="flex flex-col gap-0.5">
              <span className="text-[14px] font-medium text-foreground">SSH deploy key</span>
              <span className="text-[13px] text-muted-foreground">
                Dedicated key for this mirror — not your Oxidean account SSH keys.
              </span>
            </span>
          </label>
        </RadioGroup>
      </div>

      <div className="flex flex-col gap-3" data-testid="mirror-auth-fields">
        <div
          className={authKind === "https_token" ? "flex flex-col gap-3" : "hidden"}
          data-testid="mirror-auth-https"
          aria-hidden={authKind !== "https_token"}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="mirror-user">Username</Label>
            <Input
              id="mirror-user"
              value={username}
              onInput={(e) => setUsername((e.currentTarget as HTMLInputElement).value)}
              disabled={pending}
              autoComplete="off"
            />
            <p className="text-[13px] text-muted-foreground">
              Often <code className="rounded bg-muted px-1 font-mono text-[12px]">git</code> or{" "}
              <code className="rounded bg-muted px-1 font-mono text-[12px]">x-access-token</code> on
              GitHub. Remote forge username, not your Oxidean login.
            </p>
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="mirror-token">
              {
                (existing?.has_secret
                  ? "Remote token (leave blank to keep)"
                  : "Remote token / password") as string
              }
            </Label>
            <Input
              id="mirror-token"
              type="password"
              value={secret}
              onInput={(e) => setSecret((e.currentTarget as HTMLInputElement).value)}
              disabled={pending}
              autoComplete="new-password"
            />
            <p className="text-[13px] text-muted-foreground">
              Token on the <strong className="font-medium text-foreground">remote</strong> forge
              with write access — not an Oxidean PAT.
            </p>
          </div>
        </div>

        <div
          className={authKind === "ssh_key" ? "flex flex-col gap-3" : "hidden"}
          data-testid="mirror-auth-ssh"
          aria-hidden={authKind !== "ssh_key"}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="mirror-kh">Host key (known_hosts)</Label>
            <Textarea
              id="mirror-kh"
              className="min-h-[80px] font-mono text-[12px]"
              value={knownHosts}
              onInput={(e) => setKnownHosts((e.currentTarget as HTMLTextAreaElement).value)}
              disabled={pending || fetchKhPending}
              placeholder="github.com ssh-ed25519 AAAA…"
            />
            <div className="flex flex-wrap items-center gap-2">
              <Button
                type="button"
                variant="secondary"
                disabled={pending || fetchKhPending || !remoteUrl.trim()}
                onClick={() => void fetchHostKey()}
              >
                {(fetchKhPending ? "Fetching…" : "Fetch host key") as string}
              </Button>
              <p className="text-[13px] text-muted-foreground">
                Pins the remote SSH fingerprint. Required before sync.
              </p>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="mirror-key">
              {
                (existing?.has_secret
                  ? "Private key PEM (leave blank to keep)"
                  : existing
                    ? "Private key PEM (or generate below)"
                    : "Private key PEM (optional — or generate after save)") as string
              }
            </Label>
            <Textarea
              id="mirror-key"
              className="min-h-[80px] font-mono text-[12px]"
              value={secret}
              onInput={(e) => setSecret((e.currentTarget as HTMLTextAreaElement).value)}
              disabled={pending}
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
            />
            @if (existing) {
              <Button
                type="button"
                variant="secondary"
                className="w-fit"
                disabled={pending || !knownHosts.trim()}
                onClick={() => void generateKey()}
              >
                Generate deploy key
              </Button>;
            } @else {
              <p className="text-[13px] text-muted-foreground">
                Save the URL and host key first, then generate a deploy key (or paste a
                private key now).
              </p>;
            }
          </div>

          @if (existing?.ssh_public_key) {
            <>
              <CopyField
                label="Deploy public key"
                value={existing.ssh_public_key}
                ariaLabel="Mirror deploy public key"
                testId="mirror-ssh-public-key"
              />
              <p className="text-[13px] text-muted-foreground">
                Add this on the remote as a deploy key with{" "}
                <strong className="font-medium text-foreground">write</strong> access.
              </p>
            </>;
          }
        </div>
      </div>

      @if (existing) {
        <div className="flex flex-col gap-3">
          <h3 className="text-[15px] font-semibold text-foreground">Inbound webhook</h3>
          <p className="text-[14px] text-muted-foreground">
            On GitHub, GitLab, Gitea, or Forgejo, add a Push webhook with this URL and secret
            so remote-only pushes wake Oxidean.
          </p>
          <CopyField
            label="Webhook URL"
            value={existing.webhook_url}
            ariaLabel="Inbound mirror webhook URL"
            testId="mirror-webhook-url"
          />
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <p className="text-[14px] text-muted-foreground">
              Secret:{" "}
              <code className="text-foreground">
                {existing.webhook_secret_masked as string}
              </code>
            </p>
            <Button
              type="button"
              variant="secondary"
              disabled={pending}
              onClick={() => void rotateWebhook()}
            >
              Rotate secret
            </Button>
          </div>
        </div>;
      }

      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-end gap-4">
          <Button type="button" disabled={pending || !saveEnabled} onClick={() => void save()}>
            {(pending ? "Saving…" : existing ? "Save changes" : "Save mirror") as string}
          </Button>
          <div className="flex items-center gap-2 pb-1">
            <Switch
              checked={enabled}
              onCheckedChange={(v: boolean) => setEnabled(v)}
              disabled={pending}
              id="mirror-enabled"
            />
            <Label htmlFor="mirror-enabled">Enabled</Label>
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="mirror-poll">Backup poll (seconds)</Label>
            <Input
              id="mirror-poll"
              value={pollSecs}
              onInput={(e) => setPollSecs((e.currentTarget as HTMLInputElement).value)}
              disabled={pending}
              className="w-28"
            />
          </div>
        </div>
        <p className="text-[13px] text-muted-foreground">
          Poll is a backup if the remote cannot send webhooks (<code className="font-mono">0</code>{" "}
          = off). Event-driven sync stays on while the mirror is enabled.
        </p>
      </div>

      @if (existing) {
        <div className="flex flex-col gap-2 border-t border-border pt-4">
          <p className="text-[14px] text-muted-foreground">
            Removing the mirror stops sync and deletes stored credentials for this remote. The
            local repository is unchanged.
          </p>
          <Button
            type="button"
            variant="destructive"
            className="w-fit"
            disabled={pending}
            onClick={() => void removeMirror()}
          >
            Remove mirror
          </Button>
        </div>;
      }
    </section>;
  }
}
