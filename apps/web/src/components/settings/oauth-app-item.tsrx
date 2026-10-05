import {
  oauthAppDeleteMutationOptions,
  oauthAppRegenerateSecretMutationOptions,
  oauthAppUpdateMutationOptions,
  type OAuthAppPublic,
} from "@oxidean/api-client";
import { useMutation, useQueryClient } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { OAuthSecretReveal } from "@/components/settings/oauth-secret-reveal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ListRow } from "@/components/ui/list";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

type OAuthAppItemProps = {
  app: OAuthAppPublic;
};

function parseRedirectUris(raw: string): string[] {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** One owned OAuth app row: details + inline edit + delete + secret rotation. */
export function OAuthAppItem({ app }: OAuthAppItemProps) @{
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [newSecret, setNewSecret] = useState("");
  const [error, setError] = useState("");

  const edit = useForm({
    defaultValues: {
      name: app.name,
      redirect_uris: app.redirect_uris.join("\n"),
    },
    onSubmit: ({ value }) => {
      if (update.isPending) return;
      setError("");
      const name = value.name.trim();
      if (!name) {
        setError("Application name is required.");
        return;
      }
      const uris = parseRedirectUris(value.redirect_uris);
      if (uris.length === 0) {
        setError("At least one callback URL is required.");
        return;
      }
      update.mutate({ id: app.id, name, redirect_uris: uris });
    },
  });

  const invalidateList = () => queryClient.invalidateQueries({ queryKey: ["oauthApp", "list"] });

  const update = useMutation({
    ...oauthAppUpdateMutationOptions(apiClient),
    onSuccess: async () => {
      setEditing(false);
      setError("");
      await invalidateList();
    },
    onError: (err: Error) => setError(err.message || "Couldn't update the application."),
  });

  const remove = useMutation({
    ...oauthAppDeleteMutationOptions(apiClient),
    onSuccess: async () => {
      setConfirmingDelete(false);
      await invalidateList();
    },
    onError: (err: Error) => {
      setConfirmingDelete(false);
      setError(err.message || "Couldn't delete the application.");
    },
  });

  const rotate = useMutation({
    ...oauthAppRegenerateSecretMutationOptions(apiClient),
    onSuccess: async (res) => {
      setNewSecret(res.client_secret);
      await invalidateList();
    },
    onError: (err: Error) => setError(err.message || "Couldn't rotate the secret."),
  });

  const busy = update.isPending || remove.isPending || rotate.isPending;

  <ListRow className="block p-4" data-testid={`oauth-app-${app.id}`}>
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p
            className="truncate text-[14px] font-semibold text-foreground"
            title={app.name as string}
          >
            {app.name as string}
          </p>
          <Badge variant="outline">OAuth app</Badge>
        </div>
        <p className="mt-1 font-mono text-[12px] text-muted-foreground">
          {"Client ID: " + app.client_id}
        </p>
        <p className="mt-1 font-mono text-[12px] text-muted-foreground">
          {"Client secret: " + app.client_secret_prefix + "***"}
        </p>
        <p className="mt-1 text-[13px] text-muted-foreground">
          {"Callback URLs: " + app.redirect_uris.join(", ")}
        </p>
        <p className="mt-1 text-[13px] text-muted-foreground">
          {"Registered " + formatRelative(app.created_at)}
        </p>
      </div>
      <div className="flex shrink-0 flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={busy}
          onClick={() => {
            setEditing(!editing);
            setConfirmingDelete(false);
          }}
        >
          {(editing ? "Close" : "Edit") as string}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={busy}
          onClick={() => void rotate.mutateAsync()}
        >
          {(rotate.isPending ? "Rotating…" : "Rotate secret") as string}
        </Button>
        @if (!confirmingDelete) {
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => setConfirmingDelete(true)}
          >
            Delete
          </Button>;
        } @else {
          <Button
            type="button"
            variant="destructive"
            disabled={busy}
            onClick={() => void remove.mutateAsync()}
          >
            {(remove.isPending ? "Deleting…" : "Confirm delete") as string}
          </Button>;
        }
      </div>
    </div>

    @if (newSecret) {
      <div className="mt-4">
        <OAuthSecretReveal secret={newSecret} />
      </div>;
    }

    @if (error) {
      <p role="alert" className="mt-3 text-[14px] text-destructive">
        {error as string}
      </p>;
    }

    @if (editing) {
      <form
        method="post"
        action="#"
        className="mt-4 flex flex-col gap-4 border-t border-border pt-4"
        onSubmit={(e) => {
          e.preventDefault();
          void edit.handleSubmit();
        }}
      >
        <edit.Field name="name">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor={`oauth-app-name-${app.id}`}>Application name</Label>
              <Input
                id={`oauth-app-name-${app.id}`}
                name="name"
                value={field.state.value}
                disabled={busy}
                onBlur={field.handleBlur}
                onInput={(e) =>
                  field.handleChange((e.currentTarget as HTMLInputElement).value)
                }
                autoComplete="off"
              />
            </div>
          )}
        </edit.Field>
        <edit.Field name="redirect_uris">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor={`oauth-app-uris-${app.id}`}>Callback URLs</Label>
              <Textarea
                id={`oauth-app-uris-${app.id}`}
                name="redirect_uris"
                rows={3}
                className="font-mono text-[13px]"
                value={field.state.value}
                disabled={busy}
                onBlur={field.handleBlur}
                onInput={(e) =>
                  field.handleChange((e.currentTarget as HTMLTextAreaElement).value)
                }
              />
              <p className="text-[13px] text-muted-foreground">
                One URL per line. https required — http is only allowed for localhost.
              </p>
            </div>
          )}
        </edit.Field>
        <div>
          <Button type="submit" disabled={busy}>
            {(update.isPending ? "Saving…" : "Save changes") as string}
          </Button>
        </div>
      </form>;
    }
  </ListRow>;
}
