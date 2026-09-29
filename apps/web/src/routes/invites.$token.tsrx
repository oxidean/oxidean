import { createFileRoute, useParams } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AuthFormSkeleton } from "@/components/page-skeletons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";

export const Route = createFileRoute("/invites/$token")({
  component: InviteAcceptPage,
  head: () => ({
    meta: [{ title: "Accept invitation · Oxidean" }, { name: "referrer", content: "no-referrer" }],
  }),
});

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const INVALID_INVITE =
  "This invitation is invalid or has expired. Ask an organization admin to send a new invite.";
const EMAIL_MISMATCH =
  "You're signed in with a different email than this invitation. Sign out, then open the link again or create an account for the invited email.";
const LOGIN_REQUIRED =
  "An account with this email already exists. Sign in, then open the invitation link again.";
const CLOSED_SIGNUP_SUPPORT =
  "This invite lets you join even when public sign-up is closed. You'll create an account for the invited email only.";
const LOGGED_IN_SUPPORT = "Accept to join the organization with the role you were invited for.";
const TAKEN_ERROR = "That username is already taken. Try another username.";
const RESERVED_ERROR = "That username is reserved. Choose a different username.";

type Phase =
  | { kind: "loading" }
  | { kind: "logged-in"; user: UserPublic }
  | { kind: "anon" }
  | { kind: "done"; orgSlug: string };

export function InviteAcceptPage() @{
  const params = useParams({ from: "/invites/$token" }) as { token?: string };
  const token = String(params?.token ?? "").trim();

  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [error, setError] = useState("");

  const acceptForm = useForm({
    defaultValues: {},
    onSubmit: async () => {
      await acceptInvite();
    },
  });

  const anonForm = useForm({
    defaultValues: {
      username: "",
      password: "",
      confirm: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (value.password !== value.confirm) {
        setError("Passwords do not match. Fix the highlighted fields and try again.");
        return;
      }
      await acceptInvite({
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
        const me = await apiClient.auth.me();
        if (cancelled) return;
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

  async function acceptInvite(fields?: { username?: string; password?: string }) {
    setError("");
    if (!token) {
      setError(INVALID_INVITE);
      return;
    }
    try {
      const res = await apiClient.org.invites.accept({
        token,
        username: fields?.username ?? null,
        password: fields?.password ?? null,
      });
      if (!res.ok) {
        const code = res.error.code;
        if (code === "org.invalid_invite") setError(INVALID_INVITE);
        else if (code === "org.invite_email_mismatch") setError(EMAIL_MISMATCH);
        else if (code === "org.invite_login_required") setError(LOGIN_REQUIRED);
        else if (code === "auth.taken") setError(TAKEN_ERROR);
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
      setPhase({ kind: "done", orgSlug: res.data.org.slug });
      window.location.assign(`/${res.data.org.slug}`);
    } catch {
      setError(NETWORK_ERROR);
    }
  }

  const loginHref = `/login?returnTo=${encodeURIComponent(`/invites/${token}`)}`;

  @if (phase.kind === "loading") {
    <AuthShell title="Accept invitation" support="Checking your session…">
      <AuthFormSkeleton />
    </AuthShell>;
  } @else {
    @if (phase.kind === "done") {
      <AuthShell
        title="Invitation accepted"
        support={`Redirecting to /${phase.orgSlug as string}…`}
      >
        <p className="text-sm text-muted-foreground">You're in. Opening the organization…</p>
      </AuthShell>;
    } @else {
      @if (phase.kind === "logged-in") {
        <AuthShell title="Accept invitation" support={LOGGED_IN_SUPPORT}>
          @if (error) {
            <AuthErrorBanner message={error} />;
          }
          <p className="text-sm text-muted-foreground">
            Signed in as{" "}
            <span className="font-medium text-foreground">{phase.user.username as string}</span>.
            Join with this account if it matches the invited email.
          </p>
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
            <a className="underline underline-offset-2" href={loginHref}>
              Sign in with a different account
            </a>
            .
          </p>
        </AuthShell>;
      } @else {
        <AuthShell title="Accept invitation" support={CLOSED_SIGNUP_SUPPORT}>
          @if (error) {
            <AuthErrorBanner message={error} />;
          }
          <p className="text-sm text-muted-foreground">
            Create an account for the invited email, or{" "}
            <a className="underline underline-offset-2" href={loginHref}>
              sign in
            </a>{" "}
            if you already have one.
          </p>
          <form
            className="flex flex-col gap-4"
            method="post"
            action="#"
            onSubmit={(e) => {
              e.preventDefault();
              void anonForm.handleSubmit();
            }}
          >
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
                    onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
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
                    onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
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
                    onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
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
