import { useForm, useStore } from "@octanejs/tanstack-form";
import { useEffect, useState } from "octane";
import { AppPage } from "@/components/app-page";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AuthFormSkeleton } from "@/components/page-skeletons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { networkErrorMessage, rpcErrorMessage } from "@/lib/rpc-error-message";
import { readReturnToFromLocation, safeReturnTo } from "@/lib/return-to";

const DEFAULT_USERNAME = "system-administrator";
const NETWORK_ERROR = networkErrorMessage();
const DEFAULT_USERNAME_ERROR = "Choose a username other than the default system-administrator.";

/**
 * `/setup/credentials` — forced default-credential change (title/meta via
 * shell). The client effect below performs the must-change gate.
 */
export function CredentialsPage() @{
  <AppPage>
    <CredentialsContent />
  </AppPage>;
}

/** Exported for AUTH-06 integration tests (Keep password Switch + default username). */
export function CredentialsContent() @{
  const [ready, setReady] = useState(false);
  const [error, setError] = useState("");

  const form = useForm({
    defaultValues: {
      email: "",
      username: "",
      keepPassword: true,
      password: "",
      confirm: "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      const nextUsername = value.username.trim();
      if (nextUsername.toLowerCase() === DEFAULT_USERNAME) {
        setError(DEFAULT_USERNAME_ERROR);
        return;
      }
      if (!value.keepPassword) {
        if (value.password.length < 8) {
          setError(
            "Password must be at least 8 characters. Fix the highlighted fields and try again.",
          );
          return;
        }
        if (value.password !== value.confirm) {
          setError("Passwords do not match. Fix the highlighted fields and try again.");
          return;
        }
      }
      try {
        const res = await apiClient.auth.confirmAdminCredentials({
          username: nextUsername,
          email: value.email.trim() || null,
          password: value.keepPassword ? null : value.password,
          keep_password: value.keepPassword,
        });
        if (!res.ok) {
          const code = res.error.code;
          if (code === "auth.invalid_username") {
            setError(DEFAULT_USERNAME_ERROR);
          } else if (code === "auth.taken") {
            setError("That email or username is already taken. Try another.");
          } else if (
            code === "auth.invalid_email" ||
            code === "auth.weak_password" ||
            code === "rpc.bad_input"
          ) {
            setError(`${res.error.message}. Fix the highlighted fields and try again.`);
          } else if (code === "auth.credentials_not_required") {
            window.location.assign(safeReturnTo(readReturnToFromLocation()));
            return;
          } else if (code === "auth.unauthenticated") {
            window.location.assign(`/login?returnTo=${encodeURIComponent("/setup/credentials")}`);
            return;
          } else {
            setError(rpcErrorMessage(res.error));
          }
          return;
        }
        window.location.assign(safeReturnTo(readReturnToFromLocation()));
      } catch {
        setError(NETWORK_ERROR);
      }
    },
  });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const me = await apiClient.auth.me();
        if (cancelled) return;
        if (!me.ok) {
          window.location.assign(`/login?returnTo=${encodeURIComponent("/setup/credentials")}`);
          return;
        }
        if (!me.data.must_change_credentials) {
          window.location.assign(safeReturnTo(readReturnToFromLocation()));
          return;
        }
        form.setFieldValue("email", me.data.email);
        form.setFieldValue("username", me.data.username);
        setReady(true);
      } catch {
        if (cancelled) return;
        setError(NETWORK_ERROR);
        setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Read at component scope — form.Field nested inside a form.Subscribe
  // render-prop crashes under octane 0.10 (children resolver invokes function
  // children eagerly).
  const keepPassword = useStore(form.store, (s) => s.values.keepPassword);

  <AuthShell
    title="Confirm admin account"
    support="Review your admin email and username. Change any still-default values before continuing."
  >
    @if (error) {
      <AuthErrorBanner message={error} />;
    }

    @if (!ready) {
      <AuthFormSkeleton fields={3} />;
    }

    @if (ready) {
      <form
        className="flex flex-col gap-4"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        <form.Field name="email">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="credentials-email">Email</Label>
              <Input
                id="credentials-email"
                name="email"
                type="email"
                autoComplete="email"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLInputElement).value)
                }
                required
              />
            </div>
          )}
        </form.Field>
        <form.Field name="username">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="credentials-username">Username</Label>
              <Input
                id="credentials-username"
                name="username"
                autoComplete="username"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) =>
                  field.handleChange((ev.currentTarget as HTMLInputElement).value)
                }
                required
              />
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                Change the default system-administrator username before continuing.
              </p>
            </div>
          )}
        </form.Field>
        <form.Field name="keepPassword">
          {(field) => (
            <div className="flex flex-col gap-2">
              <div className="flex min-h-11 items-center gap-2">
                <Switch
                  id="credentials-keep-password"
                  checked={field.state.value}
                  onCheckedChange={(checked) => field.handleChange(checked === true)}
                />
                <Label htmlFor="credentials-keep-password">Keep current password</Label>
              </div>
            </div>
          )}
        </form.Field>
        @if (!keepPassword) {
          <div className="flex flex-col gap-4">
            <form.Field name="password">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="credentials-password">New password</Label>
                  <Input
                    id="credentials-password"
                    name="password"
                    type="password"
                    autoComplete="new-password"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(ev) =>
                      field.handleChange((ev.currentTarget as HTMLInputElement).value)
                    }
                    required
                  />
                </div>
              )}
            </form.Field>
            <form.Field name="confirm">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="credentials-confirm">Confirm password</Label>
                  <Input
                    id="credentials-confirm"
                    name="confirm"
                    type="password"
                    autoComplete="new-password"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(ev) =>
                      field.handleChange((ev.currentTarget as HTMLInputElement).value)
                    }
                    required
                  />
                </div>
              )}
            </form.Field>
          </div>;
        }
        <form.Subscribe selector={(s) => s.isSubmitting}>
          {(isSubmitting) => (
            <Button type="submit" className="w-full" disabled={isSubmitting}>
              {(isSubmitting ? "Working…" : "Save and continue") as string}
            </Button>
          )}
        </form.Subscribe>
      </form>;
    }
  </AuthShell>;
}
