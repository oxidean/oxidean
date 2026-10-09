import { navigate } from "astro:transitions/client";
import { useQuery } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { AppPage } from "@/components/app-page";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const RESERVED_ERROR = "That username is reserved. Choose a different username.";
const TAKEN_ERROR = "That slug is already used by a user or organization. Choose a different slug.";
const SLUG_HELPER =
  "Letters, digits, hyphen, underscore, and period. Same rules as usernames — shared with the user namespace.";

/** `/orgs/new` — signed-in only (anonymous → /login?returnTo=…). */
export function OrgsNewPage() @{
  <AppPage>
    <OrgsNewContent />
  </AppPage>;
}

/** Exported for /orgs/new Wave 0 integration tests (ORG-01 / D-ORG-01 / D-ORG-06). */
function OrgsNewContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/orgs/new" : null);
  const user = meQ.data ?? undefined;
  const showWall = !user || !user.email_verified;

  const [slugError, setSlugError] = useState("");
  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);

  const form = useForm({
    defaultValues: {
      slug: "",
      display_name: "",
    },
    onSubmit: async ({ value }) => {
      setSlugError("");
      setFormError("");
      const trimmed = value.slug.trim();
      if (!trimmed) {
        setSlugError("Organization slug is required.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.org.create({
          slug: trimmed,
          display_name: value.display_name.trim() || null,
        });
        if (!res.ok) {
          const code = res.error.code;
          if (code === "org.slug_taken") {
            setSlugError(TAKEN_ERROR);
          } else if (code === "auth.reserved_username") {
            setSlugError(RESERVED_ERROR);
          } else if (code === "auth.invalid_username") {
            setSlugError(res.error.message);
          } else if (code === "auth.email_unverified") {
            setFormError("Verify your email before creating an organization.");
          } else {
            setFormError(res.error.message || NETWORK_ERROR);
          }
          setPending(false);
          return;
        }
        void navigate(`/${res.data.slug}`);
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  @if (showWall) {
    <AuthShell
      title="Verify your email"
      support="Verify your email before creating an organization."
    >
      <AppLink href="/verify" className={cn(buttonVariants({ variant: "default" }))}>
        Verify email
      </AppLink>
    </AuthShell>;
  } @else {
    <div className="mx-auto max-w-2xl px-4 py-16">
      <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
        Create a new organization
      </h1>
      <p className="mt-2 text-[16px] text-muted-foreground">
        Organizations own repositories under a shared slug namespace with users.
      </p>

      <form
        className="mt-8 flex flex-col gap-6"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        @if (formError) {
          <AuthErrorBanner message={formError} />;
        }

        <form.Field name="slug">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="org-slug">Slug</Label>
              <Input
                id="org-slug"
                name="slug"
                autoComplete="off"
                value={field.state.value}
                aria-invalid={slugError ? true : undefined}
                onBlur={field.handleBlur}
                onInput={(ev) => {
                  field.handleChange((ev.currentTarget as HTMLInputElement).value);
                  if (slugError) setSlugError("");
                }}
                required
              />
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                {SLUG_HELPER}
              </p>
              @if (slugError) {
                <p role="alert" className="text-[14px] text-destructive">
                  {slugError as string}
                </p>;
              }
            </div>
          )}
        </form.Field>

        <form.Field name="display_name">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="org-display-name">Display name (optional)</Label>
              <Input
                id="org-display-name"
                name="display_name"
                autoComplete="organization"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              />
            </div>
          )}
        </form.Field>

        <Button type="submit" disabled={pending}>
          @if (pending) {
            <>Creating…</>;
          } @else {
            <>Create organization</>;
          }
        </Button>
      </form>
    </div>;
  }
}
