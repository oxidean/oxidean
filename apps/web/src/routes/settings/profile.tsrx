import { AppPage } from "@/components/app-page";
import { useForm } from "@octanejs/tanstack-form";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { EmailListItem, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AccountEmailsSection } from "@/components/settings/account-emails-section";
import { ProfileAvatarField } from "@/components/settings/profile-avatar-field";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { setAuthMeCache } from "@/lib/session-queries";
import { fetchEmailList, fetchUserGetProfile } from "@/lib/ssr-auth";
import { toastError, toastSuccess, toastWarning } from "@/lib/toast";

export type ProfileLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: UserPublic; emails: EmailListItem[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const BIO_MAX = 160;

function settingsProfileQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsProfile"],
    queryFn: async (): Promise<ProfileLoaderData> => {
      try {
        const res = await fetchUserGetProfile();
        if (!res.ok) {
          if (res.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: res.error.message || "Could not load profile.",
          };
        }
        const listRes = await fetchEmailList();
        if (!listRes.ok) {
          return {
            kind: "error",
            message: listRes.error.message || "Could not load email addresses.",
          };
        }
        return { kind: "ready", user: res.data, emails: listRes.data };
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
  | { kind: "ready"; user: UserPublic; emails: EmailListItem[] }
  | { kind: "error"; message: string };

/** Exported for profile.integration.test.ts (RESEARCH P1 / D-QH-03). */
export function ProfilePage() @{
  <AppPage>
    <ProfileContent />
  </AppPage>;
}

export function ProfileContent() @{
  const queryClient = useQueryClient();
  const loaderData = useQuery(settingsProfileQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/profile");
    }
  }, [loaderData]);

  const initialUser = loaderData?.kind === "ready" ? loaderData.user : null;

  const [phase, setPhase] = useState<Phase>(() => {
    if (loaderData?.kind === "ready") {
      return { kind: "ready", user: loaderData.user, emails: loaderData.emails };
    }
    if (loaderData?.kind === "error") {
      return { kind: "error", message: loaderData.message };
    }
    if (loaderData?.kind === "unauthenticated") {
      return { kind: "loading" };
    }
    return { kind: "loading" };
  });
  const [avatarUrl, setAvatarUrl] = useState<string | null | undefined>(
    () => initialUser?.avatar_url,
  );
  const [avatarUploadPending, setAvatarUploadPending] = useState(false);
  const [avatarRemovePending, setAvatarRemovePending] = useState(false);
  const [pending, setPending] = useState(false);

  const profileForm = useForm({
    defaultValues: {
      display_name: initialUser?.display_name ?? "",
      username: initialUser?.username ?? "",
      bio: initialUser?.bio ?? "",
    },
    onSubmit: async ({ value }) => {
      setPending(true);
      const lockedUsername =
        phase.kind === "ready" ? phase.user.username : value.username.trim() || "";
      try {
        const res = await apiClient.user.updateProfile({
          display_name: value.display_name.trim(),
          username: lockedUsername,
          bio: value.bio.slice(0, BIO_MAX),
        });
        if (!res.ok) {
          toastError(res.error.message || "Could not save profile. Try again.");
          setPending(false);
          return;
        }
        setPhase((prev) => {
          if (prev.kind !== "ready") return { kind: "ready", user: res.data, emails: [] };
          return { kind: "ready", user: res.data, emails: prev.emails };
        });
        profileForm.setFieldValue("display_name", res.data.display_name ?? "");
        profileForm.setFieldValue("username", res.data.username ?? "");
        profileForm.setFieldValue("bio", res.data.bio ?? "");
        setAvatarUrl(res.data.avatar_url);
        setAuthMeCache(queryClient, res.data);
        toastSuccess("Profile saved.");
        setPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      const user = loaderData.user;
      profileForm.setFieldValue("display_name", user.display_name ?? "");
      profileForm.setFieldValue("username", user.username ?? "");
      profileForm.setFieldValue("bio", user.bio ?? "");
      setAvatarUrl(user.avatar_url);
      setPhase({ kind: "ready", user, emails: loaderData.emails });
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
        const res = await apiClient.user.getProfile();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign("/login?returnTo=/settings/profile");
          return;
        }
        const listRes = await apiClient.email.list();
        if (cancelled) return;
        if (!listRes.ok) {
          setPhase({
            kind: "error",
            message: listRes.error.message || "Could not load email addresses.",
          });
          return;
        }
        const user = res.data;
        profileForm.setFieldValue("display_name", user.display_name ?? "");
        profileForm.setFieldValue("username", user.username ?? "");
        profileForm.setFieldValue("bio", user.bio ?? "");
        setAvatarUrl(user.avatar_url);
        setPhase({ kind: "ready", user, emails: listRes.data });
      } catch {
        if (cancelled) return;
        setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loaderData]);

  async function onAvatarSelected(file: File | undefined) {
    if (!file) return;
    setAvatarUploadPending(true);
    const formData = new FormData();
    formData.append("avatar", file);
    try {
      const res = await fetch("/api/user/avatar", {
        method: "POST",
        credentials: "include",
        body: formData,
      });
      const json = (await res.json()) as {
        ok?: boolean;
        avatar_url?: string;
        error?: { message?: string };
      };
      if (!res.ok || !json.ok || !json.avatar_url) {
        toastError(json.error?.message || "Avatar upload failed. Try again.");
        setAvatarUploadPending(false);
        throw new Error("avatar upload failed");
      }
      const nextUrl = `${json.avatar_url}?t=${Date.now()}`;
      setAvatarUrl(nextUrl);
      setPhase((prev) => {
        if (prev.kind !== "ready") return prev;
        const nextUser = { ...prev.user, avatar_url: json.avatar_url };
        setAuthMeCache(queryClient, nextUser);
        return { kind: "ready", user: nextUser, emails: prev.emails };
      });
      toastSuccess("Profile picture updated.");
      setAvatarUploadPending(false);
    } catch (err) {
      setAvatarUploadPending(false);
      if (err instanceof Error && err.message === "avatar upload failed") {
        throw err;
      }
      toastError(NETWORK_ERROR);
      throw err instanceof Error ? err : new Error(NETWORK_ERROR);
    }
  }

  async function onAvatarRemove() {
    setAvatarRemovePending(true);
    try {
      const res = await fetch("/api/user/avatar", {
        method: "DELETE",
        credentials: "include",
      });
      const json = (await res.json()) as {
        ok?: boolean;
        error?: { message?: string };
      };
      if (!res.ok || !json.ok) {
        toastError(json.error?.message || "Could not remove profile picture.");
        setAvatarRemovePending(false);
        return;
      }
      setAvatarUrl(null);
      setPhase((prev) => {
        if (prev.kind !== "ready") return prev;
        const nextUser = { ...prev.user, avatar_url: null };
        setAuthMeCache(queryClient, nextUser);
        return { kind: "ready", user: nextUser, emails: prev.emails };
      });
      toastSuccess("Profile picture removed.");
      setAvatarRemovePending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setAvatarRemovePending(false);
    }
  }

  @if (phase.kind === "loading") {
    <></>;
  } @else {
    @if (phase.kind === "error") {
      <SettingsShell bare>
        <p className="text-[16px] text-destructive">{phase.message as string}</p>
      </SettingsShell>;
    } @else {
      <AccountSettingsLayout active="profile" data-testid="settings-profile-page">
        <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
          Account
        </h1>

        <div className="mt-8 flex flex-col gap-6">
          <ProfileAvatarField
            user={phase.user}
            src={avatarUrl}
            uploadPending={avatarUploadPending}
            removePending={avatarRemovePending}
            onCroppedFile={(file) => onAvatarSelected(file)}
            onRemove={() => onAvatarRemove()}
            onReject={() => {
              toastWarning("Choose a JPEG, PNG, or WebP image under 2 MB.");
            }}
          />

          <form
            method="post"
            action="#"
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              void profileForm.handleSubmit();
            }}
          >
            <profileForm.Field name="display_name">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="display-name">Display name</Label>
                  <Input
                    id="display-name"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
                    autoComplete="name"
                  />
                </div>
              )}
            </profileForm.Field>
            <profileForm.Field name="username">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="username">Username</Label>
                  <Input
                    id="username"
                    value={field.state.value}
                    readOnly
                    disabled
                    autoComplete="username"
                    aria-readonly="true"
                  />
                  <p className="text-[14px] text-muted-foreground">
                    Usernames cannot be changed. Profile URLs and git remotes use{" "}
                    <span className="font-mono">/{field.state.value || "username"}/…</span>.
                  </p>
                </div>
              )}
            </profileForm.Field>
            <profileForm.Field name="bio">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="bio">Bio</Label>
                  <Textarea
                    id="bio"
                    rows={4}
                    maxLength={BIO_MAX}
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) =>
                      field.handleChange(
                        (e.currentTarget as HTMLTextAreaElement).value.slice(0, BIO_MAX),
                      )
                    }
                  />
                  <p className="text-[14px] text-muted-foreground">
                    Optional · up to 160 characters
                  </p>
                </div>
              )}
            </profileForm.Field>
            <Button type="submit" disabled={pending} className="w-full sm:w-auto">
              {(pending ? "Working…" : "Save profile") as string}
            </Button>
          </form>
        </div>

        <AccountEmailsSection initialEmails={phase.emails} />
      </AccountSettingsLayout>;
    }
  }
}
