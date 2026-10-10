import {
  gpgKeyListQueryOptions,
  sshKeyListQueryOptions,
  type GpgKeyListItem,
  type SshKeyListItem,
  type UserPublic,
} from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { GpgKeyAddForm } from "@/components/settings/gpg-key-add-form";
import { GpgKeyList } from "@/components/settings/gpg-key-list";
import { SshKeyAddForm } from "@/components/settings/ssh-key-add-form";
import { SshKeyList } from "@/components/settings/ssh-key-list";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { fetchGpgKeyList, fetchSessionMe, fetchSshKeyList } from "@/lib/ssr-auth";

type SshKeysLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      user: UserPublic;
      keys: SshKeyListItem[];
      gpgKeys: GpgKeyListItem[];
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function settingsSshKeysQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsSshKeys"],
    queryFn: async (): Promise<SshKeysLoaderData> => {
      try {
        const res = await fetchSessionMe();
        if (!res.ok) {
          if (res.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: res.error.message || "Could not load session.",
          };
        }
        const listRes = await fetchSshKeyList();
        if (!listRes.ok) {
          return {
            kind: "error",
            message: listRes.error.message || "Could not load SSH keys.",
          };
        }
        const gpgRes = await fetchGpgKeyList();
        if (!gpgRes.ok) {
          return {
            kind: "error",
            message: gpgRes.error.message || "Could not load GPG keys.",
          };
        }
        return {
          kind: "ready",
          user: res.data,
          keys: listRes.data,
          gpgKeys: gpgRes.data,
        };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

type Phase =
  | { kind: "loading" }
  | { kind: "ready"; user: UserPublic }
  | { kind: "error"; message: string };

/** Exported for ssh-keys.integration.test.ts (GIT-04 / D-SSH-06). */
export function SshKeysPage() @{
  <AppPage>
    <SshKeysContent />
  </AppPage>;
}

function SshKeysContent() @{
  const loaderData = useQuery(settingsSshKeysQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/ssh-keys");
    }
  }, [loaderData]);

  const [phase, setPhase] = useState<Phase>(() => {
    if (loaderData?.kind === "ready") {
      return { kind: "ready", user: loaderData.user };
    }
    if (loaderData?.kind === "error") {
      return { kind: "error", message: loaderData.message };
    }
    if (loaderData?.kind === "unauthenticated") {
      return { kind: "loading" };
    }
    return { kind: "loading" };
  });

  const [keys, setKeys] = useState<SshKeyListItem[] | undefined>(() =>
    loaderData?.kind === "ready" ? loaderData.keys : undefined,
  );
  const [gpgKeys, setGpgKeys] = useState<GpgKeyListItem[] | undefined>(() =>
    loaderData?.kind === "ready" ? loaderData.gpgKeys : undefined,
  );
  const [showSshForm, setShowSshForm] = useState(false);
  const [showGpgForm, setShowGpgForm] = useState(false);

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      setPhase({ kind: "ready", user: loaderData.user });
      setKeys(loaderData.keys);
      setGpgKeys(loaderData.gpgKeys);
      return;
    }
    if (loaderData.kind === "error") {
      setPhase({ kind: "error", message: loaderData.message });
    }
  }, [loaderData]);

  useEffect(() => {
    if (loaderData) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await apiClient.auth.me();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign("/login?returnTo=/settings/ssh-keys");
          return;
        }
        const listRes = await apiClient.sshKey.list();
        if (cancelled) return;
        if (!listRes.ok) {
          setPhase({
            kind: "error",
            message: listRes.error.message || "Could not load SSH keys.",
          });
          return;
        }
        const gpgRes = await apiClient.gpgKey.list();
        if (cancelled) return;
        if (!gpgRes.ok) {
          setPhase({
            kind: "error",
            message: gpgRes.error.message || "Could not load GPG keys.",
          });
          return;
        }
        setKeys(listRes.data);
        setGpgKeys(gpgRes.data);
        setPhase({ kind: "ready", user: res.data });
      } catch {
        if (cancelled) return;
        setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loaderData]);

  const verified = phase.kind === "ready" ? phase.user.email_verified === true : false;

  const keyList = useQuery({
    ...sshKeyListQueryOptions(apiClient),
    enabled: phase.kind === "ready",
    ...(keys !== undefined ? { initialData: keys } : {}),
  });
  const gpgList = useQuery({
    ...gpgKeyListQueryOptions(apiClient),
    enabled: phase.kind === "ready",
    ...(gpgKeys !== undefined ? { initialData: gpgKeys } : {}),
  });
  const keyCount = keyList.data?.length ?? keys?.length ?? 0;
  const gpgCount = gpgList.data?.length ?? gpgKeys?.length ?? 0;
  const atSshCap = keyCount >= 25;
  const atGpgCap = gpgCount >= 25;
  const canAddSsh = verified && !atSshCap;
  const canAddGpg = verified && !atGpgCap;

  @if (phase.kind === "loading") {
    <></>;
  } @else {
    @if (phase.kind === "error") {
      <SettingsShell bare>
        <p role="alert" className="text-[16px] text-destructive">
          {phase.message as string}
        </p>
      </SettingsShell>;
    } @else {
      <AccountSettingsLayout active="ssh-keys" data-testid="settings-ssh-keys-page">
        <div className="min-w-0">
          <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            SSH and GPG keys
          </h1>
          <p className="mt-2 text-[16px] text-muted-foreground">
            SSH keys authenticate git over SSH and can sign commits. GPG keys verify OpenPGP
            signatures. Use a verified account email (or noreply) as your git committer email.
          </p>
        </div>

        <section className="mt-10">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h2 className="text-[18px] font-semibold text-foreground">SSH keys</h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                Choose authentication, commit signing, or both when adding a key.
              </p>
            </div>
            <Button type="button" disabled={!canAddSsh} onClick={() => setShowSshForm(true)}>
              Add SSH key
            </Button>
          </div>
          @if (!verified) {
            <p className="mt-3 text-[14px] text-muted-foreground">
              Verify your email to add keys.
            </p>;
          } @else {
            @if (atSshCap) {
              <p className="mt-3 text-[14px] text-muted-foreground">
                Maximum of 25 SSH keys
              </p>;
            }
          }
          @if (showSshForm && canAddSsh) {
            <SshKeyAddForm disabled={!canAddSsh} onAdded={() => setShowSshForm(false)} />;
          }
          <SshKeyList enabled={true} initialData={keys} />
        </section>

        <section className="mt-12">
          <h2 className="text-[18px] font-semibold text-foreground">Commit signing setup</h2>
          <p className="mt-1 text-[14px] text-muted-foreground">
            Configure local git, then push signed commits. Verified badges appear when the signature
            matches a registered key and the committer email matches your account.
          </p>
          <div className="mt-4 space-y-4">
            <div className="rounded-md border border-border bg-card p-4">
              <p className="text-[14px] font-medium text-foreground">SSH signing</p>
              <pre className="mt-2 overflow-x-auto rounded bg-muted p-3 font-mono text-[12px] text-foreground">
                {
                  "git config --global gpg.format ssh\ngit config --global user.signingkey ~/.ssh/id_ed25519.pub\ngit config --global commit.gpgsign true" as string
                }
              </pre>
            </div>
            <div className="rounded-md border border-border bg-card p-4">
              <p className="text-[14px] font-medium text-foreground">GPG signing</p>
              <pre className="mt-2 overflow-x-auto rounded bg-muted p-3 font-mono text-[12px] text-foreground">
                {
                  "git config --global user.signingkey <KEYID>\ngit config --global commit.gpgsign true" as string
                }
              </pre>
            </div>
          </div>
        </section>

        <section className="mt-12">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h2 className="text-[18px] font-semibold text-foreground">GPG keys</h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                Paste an ASCII-armored OpenPGP public key. The committer email must appear in a key
                UID.
              </p>
            </div>
            <Button type="button" disabled={!canAddGpg} onClick={() => setShowGpgForm(true)}>
              Add GPG key
            </Button>
          </div>
          @if (verified && atGpgCap) {
            <p className="mt-3 text-[14px] text-muted-foreground">Maximum of 25 GPG keys</p>;
          }
          @if (showGpgForm && canAddGpg) {
            <GpgKeyAddForm disabled={!canAddGpg} onAdded={() => setShowGpgForm(false)} />;
          }
          <GpgKeyList enabled={true} initialData={gpgKeys} />
        </section>
      </AccountSettingsLayout>;
    }
  }
}
