import { createFileRoute, useParams } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { InvitesAcceptResponse, InvitesGetResponse, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AuthFormSkeleton } from "@/components/page-skeletons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { useAppNavigate } from "@/lib/app-navigate";
import { AppLink } from "@/components/ui/app-link";

export const Route = createFileRoute("/invites/$token")({
  component: InviteAcceptPage,
  head: () => ({
    meta: [{ title: "Accept invitation · Oxidean" }, { name: "referrer", content: "no-referrer" }],
  }),
});

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const INVALID_INVITE =
  "This invitation is invalid or has expired. Ask an administrator to send a new invite.";
const EMAIL_MISMATCH =
  "You're signed in with a different email than this invitation. Sign out, then open the link again or create an account for the invited email.";
const LOGIN_REQUIRED =
  "An account with this email already exists. Sign in, then open the invitation link again.";
const CLOSED_SIGNUP_SUPPORT = "This invite lets you join even when public sign-up is closed.";
const LOGGED_IN_SUPPORT =
  "Accept to use this invitation with the role or access you were invited for.";
const TAKEN_ERROR = "That username is already taken. Try another username.";
const RESERVED_ERROR = "That username is reserved. Choose a different username.";

type Phase =
  | { kind: "loading" }
  | { kind: "logged-in"; user: UserPublic }
  | { kind: "anon" }
  | { kind: "done"; accept: InvitesAcceptResponse };

function reasonMessage(reason?: string | null): string {
  if (reason === "revoked") {
    return "This invitation was revoked. Ask an administrator to send a new invite.";
  }
  if (reason === "exhausted") {
    return "This invitation link has no seats left. Ask an administrator for a new invite.";
  }
  return INVALID_INVITE;
}

function inviteTargetText(invite: InvitesGetResponse): string {
  if (invite.kind === "org") {
    return `You've been invited to join the organization ${invite.org_display_name ?? invite.org_slug ?? ""}.`;
  }
  if (invite.kind === "repo") {
    return `You've been invited to collaborate on ${invite.repo_owner ?? ""}/${invite.repo_name ?? ""}.`;
  }
  return "You've been invited to join this Oxidean instance.";
}

export function InviteAcceptPage() @{
  const params = useParams({ from: "/invites/$token" }) as { token?: string };
  const token = String(params?.token ?? "").trim();

  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [invite, setInvite] = useState<InvitesGetResponse | null>(null);
  const [error, setError] = useState("");
  const navigateApp = useAppNavigate();

  const acceptForm = useForm({
    defaultValues: {},
    onSubmit: async () => {
      await acceptInvite();
    },
  });

  const anonForm = useForm({
    defaultValues: {
      email: "",
      username: "",
      password: "",
      confirm: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (isLinkInvite && !value.email.trim()) {
        setError("Email is required. Fix the highlighted fields and try again.");
        return;
      }
      if (value.password !== value.confirm) {
        setError("Passwords do not match. Fix the highlighted fields and try again.");
        return;
      }
      await acceptInvite({
        email: value.email.trim() || undefined,
        username: value.username.trim(),
        password: value.password,
      });
    },
  });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!token) {
        setError(INVALID_INVITE);
        setPhase({ kind: "anon" });
        return;
      }
      try {
        const [me, preview] = await Promise.all([
          apiClient.auth.me(),
          apiClient.invites.get({ token }),
        ]);
        if (cancelled) return;
        if (!preview.ok) {
          setError(INVALID_INVITE);
          setPhase(me.ok ? { kind: "logged-in", user: me.data } : { kind: "anon" });
          return;
        }
        setInvite(preview.data);
        if (!preview.data.acceptable) {
          setError(reasonMessage(preview.data.reason));
        }
        if (me.ok) {
          setPhase({ kind: "logged-in", user: me.data });
        } else {
          setPhase({ kind: "anon" });
        }
      } catch {
        if (cancelled) return;
        setPhase({ kind: "anon" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token]);

  const isLinkInvite = invite != null && (invite.email == null || invite.email === "");

  async function acceptInvite(fields?: { email?: string; username?: string; password?: string }) {
    setError("");
    if (!token) {
      setError(INVALID_INVITE);
      return;
    }
    try {
      const res = await apiClient.invites.accept({
        token,
        email: fields?.email ?? null,
        username: fields?.username ?? null,
        password: fields?.password ?? null,
      });
      if (!res.ok) {
        const code = res.error.code;
        if (
          code === "org.invalid_invite" ||
          code === "invite.invalid" ||
          code === "admin.invalid_invite"
        ) {
          setError(INVALID_INVITE);
        } else if (code === "org.invite_email_mismatch" || code === "invite.email_mismatch") {
          setError(EMAIL_MISMATCH);
        } else if (code === "org.invite_login_required" || code === "invite.login_required") {
          setError(LOGIN_REQUIRED);
        } else if (code === "auth.taken") setError(TAKEN_ERROR);
        else if (code === "auth.reserved_username") setError(RESERVED_ERROR);
        else if (
          code === "auth.weak_password" ||
          code === "auth.invalid_username" ||
          code === "rpc.bad_input"
        ) {
          setError(`${res.error.message}. Fix the highlighted fields and try again.`);
        } else {
          setError(res.error.message || NETWORK_ERROR);
        }
        return;
      }
      const wasLoggedIn = phase.kind === "logged-in";
      setPhase({ kind: "done", accept: res.data });
      const href =
        res.data.kind === "org"
          ? `/${res.data.org.slug}`
          : res.data.kind === "repo"
            ? `/${res.data.owner}/${res.data.name}`
            : "/";
      if (wasLoggedIn) {
        void navigateApp(href);
      } else {
        // raw-nav-ok — anonymous accept may mint a session cookie; the reload
        // re-establishes auth state app-wide (chrome, authMe cache) atomically
        window.location.assign(href);
      }
    } catch {
      setError(NETWORK_ERROR);
    }
  }

  const loginHref = `/login?returnTo=${encodeURIComponent(`/invites/${token}`)}`;
  const acceptable = invite != null && invite.acceptable;
  const targetText = invite ? inviteTargetText(invite) : "";
  const boundEmail = invite?.email ?? "";

  @if (phase.kind === "loading") {
    <AuthShell title="Accept invitation" support="Checking your invitation…">
      <AuthFormSkeleton />
    </AuthShell>;
  } @else {
    @if (phase.kind === "done") {
      @if (phase.accept.kind === "org") {
        <AuthShell
          title="Invitation accepted"
          support={`Redirecting to /${phase.accept.org.slug as string}…`}
        >
          <p className="text-sm text-muted-foreground">You're in. Opening the organization…</p>
        </AuthShell>;
      } @else {
        @if (phase.accept.kind === "repo") {
          <AuthShell
            title="Invitation accepted"
            support={`Redirecting to /${phase.accept.owner as string}/${phase.accept.name as string}…`}
          >
            <p className="text-sm text-muted-foreground">
              {
                `You now have ${phase.accept.permission as string} access. Opening the repository…` as string
              }
            </p>
          </AuthShell>;
        } @else {
          <AuthShell title="Invitation accepted" support="Redirecting to your home…">
            <p className="text-sm text-muted-foreground">Your account is ready. Opening Oxidean…</p>
          </AuthShell>;
        }
      }
    } @else {
      @if (!acceptable) {
        <AuthShell title="Accept invitation" support="This invitation can't be used.">
          <AuthErrorBanner message={error || reasonMessage(invite?.reason)} />
        </AuthShell>;
      } @else {
        @if (phase.kind === "logged-in") {
          <AuthShell title="Accept invitation" support={LOGGED_IN_SUPPORT}>
            @if (error) {
              <AuthErrorBanner message={error} />;
            }
            @if (targetText) {
              <p className="text-sm text-muted-foreground">{targetText as string}</p>;
            }
            <p className="text-sm text-muted-foreground">
              Signed in as{" "}
              <span className="font-medium text-foreground">{phase.user.username as string}</span>.
              {
                (isLinkInvite
                  ? " Continue with this account to accept."
                  : " Continue with this account if it matches the invited email.") as string
              }
            </p>
            @if (boundEmail) {
              <p className="text-sm text-muted-foreground">{(boundEmail as string) + ""}</p>;
            }
            <p className="text-sm text-muted-foreground">{CLOSED_SIGNUP_SUPPORT}</p>
            <form
              className="flex flex-col gap-4"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void acceptForm.handleSubmit();
              }}
            >
              <acceptForm.Subscribe selector={(s) => s.isSubmitting}>
                {(isSubmitting) => (
                  <Button type="submit" disabled={isSubmitting}>
                    {/* octane-dom-race-ok: button label text only */}
                    @if (isSubmitting) {
                      <>Accepting…</>;
                    } @else {
                      <>Accept invitation</>;
                    }
                  </Button>
                )}
              </acceptForm.Subscribe>
            </form>
            <p className="text-sm text-muted-foreground">
              Wrong account?{" "}
              <AppLink className="underline underline-offset-2" href={loginHref}>
                Sign in with a different account
              </AppLink>
              .
            </p>
          </AuthShell>;
        } @else {
          <AuthShell title="Accept invitation" support={CLOSED_SIGNUP_SUPPORT}>
            @if (error) {
              <AuthErrorBanner message={error} />;
            }
            @if (targetText) {
              <p className="text-sm text-muted-foreground">{targetText as string}</p>;
            }
            @if (boundEmail) {
              <p className="text-sm text-muted-foreground">
                {`Create an account for ${boundEmail}, or ` as string}
                <AppLink className="underline underline-offset-2" href={loginHref}>
                  sign in
                </AppLink>{" "}
                if you already have one.
              </p>;
            } @else {
              <p className="text-sm text-muted-foreground">
                Create an account, or{" "}
                <AppLink className="underline underline-offset-2" href={loginHref}>
                  sign in
                </AppLink>{" "}
                if you already have one.
              </p>;
            }
            <form
              className="flex flex-col gap-4"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void anonForm.handleSubmit();
              }}
            >
              @if (isLinkInvite) {
                <anonForm.Field name="email">
                  {(field) => (
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="invite-email">Email</Label>
                      <Input
                        id="invite-email"
                        name="email"
                        type="email"
                        autoComplete="email"
                        value={field.state.value}
                        onBlur={field.handleBlur}
                        onInput={(e) =>
                          field.handleChange((e.currentTarget as HTMLInputElement).value)
                        }
                        required
                      />
                    </div>
                  )}
                </anonForm.Field>;
              }
              <anonForm.Field name="username">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="invite-username">Username</Label>
                    <Input
                      id="invite-username"
                      name="username"
                      autoComplete="username"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(e) =>
                        field.handleChange((e.currentTarget as HTMLInputElement).value)
                      }
                      required
                    />
                  </div>
                )}
              </anonForm.Field>
              <anonForm.Field name="password">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="invite-password">Password</Label>
                    <Input
                      id="invite-password"
                      name="password"
                      type="password"
                      autoComplete="new-password"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(e) =>
                        field.handleChange((e.currentTarget as HTMLInputElement).value)
                      }
                      required
                    />
                  </div>
                )}
              </anonForm.Field>
              <anonForm.Field name="confirm">
                {(field) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="invite-confirm">Confirm password</Label>
                    <Input
                      id="invite-confirm"
                      name="confirm"
                      type="password"
                      autoComplete="new-password"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(e) =>
                        field.handleChange((e.currentTarget as HTMLInputElement).value)
                      }
                      required
                    />
                  </div>
                )}
              </anonForm.Field>
              <anonForm.Subscribe selector={(s) => s.isSubmitting}>
                {(isSubmitting) => (
                  <Button type="submit" disabled={isSubmitting}>
                    {/* octane-dom-race-ok: button label text only */}
                    @if (isSubmitting) {
                      <>Creating account…</>;
                    } @else {
                      <>Create account and join</>;
                    }
                  </Button>
                )}
              </anonForm.Subscribe>
            </form>
          </AuthShell>;
        }
      }
    }
  }
}
