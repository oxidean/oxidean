import type { DeployKeyPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function deployKeysQueryKey(owner: string, name: string) {
  return ["repo", "deployKeys", owner, name] as const;
}

function formatLastUsed(at: string | null | undefined): string {
  if (!at) return "Never used";
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return `Last used ${at}`;
  return `Last used ${d.toISOString().slice(0, 10)}`;
}

/** Repo Settings → Deploy keys (GIT-23). Transport-only SSH credentials. */
export function DeployKeysPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const keysQ = useQuery({
    queryKey: deployKeysQueryKey(owner, name),
    queryFn: async (): Promise<DeployKeyPublic[]> => {
      const res = await apiClient.repo.deployKey.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load deploy keys.");
      return res.data.keys;
    },
    enabled: canAdmin,
  });

  const [title, setTitle] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [canWrite, setCanWrite] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  async function addKey() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.deployKey.create({
        owner,
        name,
        title: title.trim(),
        public_key: publicKey.trim(),
        can_write: canWrite,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not add deploy key.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: deployKeysQueryKey(owner, name) });
      toastSuccess("Deploy key added.");
      setTitle("");
      setPublicKey("");
      setCanWrite(false);
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function deleteKey(id: string) {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.deployKey.delete({ owner, name, id });
      if (!res.ok) {
        setError(res.error.message || "Could not delete deploy key.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: deployKeysQueryKey(owner, name) });
      toastSuccess("Deploy key deleted.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const keys = keysQ.data ?? [];

  <section
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="deploy-keys-heading"
    data-testid="deploy-keys-settings"
  >
    <h2
      id="deploy-keys-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Deploy keys
    </h2>
    <p className="text-[14px] text-muted-foreground">
      SSH keys scoped to this repository for servers and CI. A read-only key can clone and fetch; a
      read/write key can also push. Deploy keys are transport credentials only — they do not grant
      account, web, or API access.
    </p>

    @if (!canAdmin) {
      <p className="text-[14px] text-muted-foreground">
        Only repository admins can manage deploy keys.
      </p>;
    } @else {
      <>
        @if (keysQ.isPending) {
          <p className="text-[14px] text-muted-foreground">Loading deploy keys…</p>;
        } @else {
          @if (keysQ.isError) {
            <p role="alert" className="text-[14px] text-destructive">
              {
                (keysQ.error instanceof Error
                  ? keysQ.error.message
                  : "Could not load deploy keys.") as string
              }
            </p>;
          } @else {
            <>
              @if (keys.length === 0) {
                <p
                  className="text-[14px] text-muted-foreground"
                  data-testid="dk-empty"
                >
                  No deploy keys yet.
                </p>;
              } @else {
                <ul className="flex flex-col gap-3" data-testid="dk-key-list">
                  @for (const key of keys; key key.id) {
                    <li className="rounded-md border border-border p-3 text-[14px]">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex min-w-0 flex-col gap-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <strong>{key.title as string}</strong>
                            <span
                              className="rounded-full border border-border px-2 py-0.5 text-[12px] text-muted-foreground"
                              data-testid={`dk-scope-${key.id}`}
                            >
                              {
                                (key.can_write
                                  ? "Read/write"
                                  : "Read-only") as string
                              }
                            </span>
                          </div>
                          <code className="break-all font-mono text-[13px] text-muted-foreground">
                            {key.fingerprint as string}
                          </code>
                          <span className="text-[13px] text-muted-foreground">
                            {formatLastUsed(key.last_used_at) as string}
                          </span>
                        </div>
                        <Button
                          type="button"
                          variant="ghost"
                          disabled={pending}
                          data-testid={`dk-delete-${key.id}`}
                          onClick={() => void deleteKey(key.id)}
                        >
                          Delete
                        </Button>
                      </div>
                    </li>;
                  }
                </ul>;
              }

              <form
                method="post"
                action="#"
                className="mt-4 flex flex-col gap-3 rounded-md border border-border p-4"
                data-testid="dk-add-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void addKey();
                }}
              >
                <h3 className="text-[16px] font-semibold text-foreground">
                  Add deploy key
                </h3>
                <div className="flex flex-col gap-1">
                  <Label for="dk-title">Title</Label>
                  <Input
                    id="dk-title"
                    value={title}
                    onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
                    placeholder="CI server"
                    disabled={pending}
                  />
                </div>
                <div className="flex flex-col gap-1">
                  <Label for="dk-public-key">Public key</Label>
                  <Textarea
                    id="dk-public-key"
                    value={publicKey}
                    onInput={(e) =>
                      setPublicKey((e.currentTarget as HTMLTextAreaElement).value)
                    }
                    placeholder="ssh-ed25519 AAAA… comment"
                    rows={3}
                    disabled={pending}
                    data-testid="dk-public-key"
                  />
                  <p className="text-[13px] text-muted-foreground">
                    Paste an OpenSSH public key (ssh-ed25519 or RSA ≥2048). The same key
                    may be attached to other repositories, but not registered as an
                    account SSH key.
                  </p>
                </div>
                <div className="flex flex-col gap-2" data-testid="dk-scope-picker">
                  <p className="text-[14px] font-medium text-foreground">Permissions</p>
                  <RadioGroup
                    value={canWrite ? "write" : "read"}
                    disabled={pending}
                    onValueChange={(v) => {
                      if (v === "read") setCanWrite(false);
                      if (v === "write") setCanWrite(true);
                    }}
                    className="gap-3"
                    aria-label="Deploy key permissions"
                  >
                    <label className="flex cursor-pointer items-start gap-3">
                      <RadioGroupItem
                        value="read"
                        className="mt-0.5"
                        data-testid="dk-scope-read"
                      />
                      <span className="flex flex-col gap-0.5">
                        <span className="text-[14px] font-medium text-foreground">
                          Read-only
                        </span>
                        <span className="text-[13px] text-muted-foreground">
                          Clone and fetch only.
                        </span>
                      </span>
                    </label>
                    <label className="flex cursor-pointer items-start gap-3">
                      <RadioGroupItem
                        value="write"
                        className="mt-0.5"
                        data-testid="dk-scope-write"
                      />
                      <span className="flex flex-col gap-0.5">
                        <span className="text-[14px] font-medium text-foreground">
                          Read/write
                        </span>
                        <span className="text-[13px] text-muted-foreground">
                          Clone, fetch, and push. Branch protection still applies.
                        </span>
                      </span>
                    </label>
                  </RadioGroup>
                </div>
                @if (error) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {error as string}
                  </p>;
                }
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="submit"
                    disabled={pending || !title.trim() || !publicKey.trim()}
                    data-testid="dk-add-submit"
                  >
                    {(pending ? "Adding…" : "Add deploy key") as string}
                  </Button>
                </div>
              </form>
            </>;
          }
        }
      </>;
    }
  </section>;
}
