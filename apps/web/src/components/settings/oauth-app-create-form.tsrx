import { oauthAppCreateMutationOptions, type OAuthAppPublic } from "@oxidean/api-client";
import { useMutation, useQueryClient } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";

type OAuthAppCreateFormProps = {
  /** Fires with the new app + one-time plaintext client_secret. */
  onCreated?: (app: OAuthAppPublic, secret: string) => void;
};

/** Redirect URIs textarea → trimmed non-empty lines (https, or http on loopback). */
function parseRedirectUris(raw: string): string[] {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** Register a third-party OAuth application (API-03). */
export function OAuthAppCreateForm({ onCreated }: OAuthAppCreateFormProps) @{
  const queryClient = useQueryClient();
  const [error, setError] = useState("");

  const form = useForm({
    defaultValues: {
      name: "",
      redirect_uris: "",
    },
    onSubmit: ({ value }) => {
      if (create.isPending) return;
      setError("");
      const name = value.name.trim();
      if (!name) {
        setError("Application name is required.");
        return;
      }
      const uris = parseRedirectUris(value.redirect_uris);
      if (uris.length === 0) {
        setError("At least one callback (redirect) URL is required.");
        return;
      }
      create.mutate({ name, redirect_uris: uris });
    },
  });

  const create = useMutation({
    ...oauthAppCreateMutationOptions(apiClient),
    onSuccess: async (res) => {
      form.reset();
      setError("");
      await queryClient.invalidateQueries({ queryKey: ["oauthApp", "list"] });
      onCreated?.(res.app, res.client_secret);
    },
    onError: (err: Error) => {
      setError(err.message || "Couldn't register the application.");
    },
  });

  <form
    method="post"
    action="#"
    className="flex flex-col gap-4 rounded-md border border-border bg-card p-4"
    onSubmit={(e) => {
      e.preventDefault();
      void form.handleSubmit();
    }}
    data-testid="oauth-app-create-form"
  >
    <form.Field name="name">
      {(field) => (
        <div className="flex flex-col gap-2">
          <Label htmlFor="oauth-app-name">Application name</Label>
          <Input
            id="oauth-app-name"
            name="name"
            value={field.state.value}
            disabled={create.isPending}
            onBlur={field.handleBlur}
            onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
            placeholder="My CI bot"
            autoComplete="off"
          />
        </div>
      )}
    </form.Field>
    <form.Field name="redirect_uris">
      {(field) => (
        <div className="flex flex-col gap-2">
          <Label htmlFor="oauth-app-redirect-uris">Callback URLs</Label>
          <Textarea
            id="oauth-app-redirect-uris"
            name="redirect_uris"
            rows={3}
            className="font-mono text-[13px]"
            placeholder={"https://example.com/oauth/callback\nhttp://localhost:9999/callback"}
            value={field.state.value}
            disabled={create.isPending}
            onBlur={field.handleBlur}
            onInput={(e) => field.handleChange((e.currentTarget as HTMLTextAreaElement).value)}
          />
          <p className="text-[13px] text-muted-foreground">
            One URL per line. https required — http is only allowed for localhost.
          </p>
        </div>
      )}
    </form.Field>
    @if (error) {
      <p role="alert" className="text-[14px] text-destructive">
        {error as string}
      </p>;
    }
    <div>
      <Button type="submit" disabled={create.isPending}>
        {(create.isPending ? "Registering…" : "Register application") as string}
      </Button>
    </div>
  </form>;
}
