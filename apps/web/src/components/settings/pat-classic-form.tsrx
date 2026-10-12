import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ClassicScopeGroups } from "@/components/settings/pat-classic-scopes";
import { PatExpiryFields } from "@/components/settings/pat-expiry-field";
import {
  resolveExpiresAt,
  tomorrowDateValue,
  type PatExpiryPreset,
} from "@/components/settings/pat-expiry";
import { selectedScopes, type ClassicScopeValues } from "@/components/settings/pat-scopes";
import { apiClient } from "@/lib/api-client";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type PatClassicFormProps = {
  onCreated: (token: string) => void;
};

/** Classic PAT create form — per-field Fields; no `@if` inside Subscribe trees. */
export function PatClassicForm({ onCreated }: PatClassicFormProps) @{
  const [noteError, setNoteError] = useState("");
  const [scopeError, setScopeError] = useState("");
  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);

  const form = useForm({
    defaultValues: {
      note: "",
      scopes: { repo: true } as ClassicScopeValues,
      expiryPreset: "30" as PatExpiryPreset,
      expiryDate: tomorrowDateValue(),
    },
    onSubmit: async ({ value }) => {
      setNoteError("");
      setScopeError("");
      setFormError("");
      const trimmed = value.note.trim();
      if (!trimmed) {
        setNoteError("Note is required.");
        return;
      }
      const scopes = selectedScopes(value.scopes);
      if (scopes.length === 0) {
        setScopeError("Select at least one scope.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.pat.createClassic({
          name: trimmed,
          scopes,
          expires_at: resolveExpiresAt(value.expiryPreset, value.expiryDate),
        });
        if (!res.ok) {
          if (res.error.code === "auth.email_unverified") {
            setFormError("Verify your email before creating a personal access token.");
          } else if (res.error.code === "pat.note_required") {
            setNoteError("Note is required.");
          } else {
            setFormError(res.error.message || NETWORK_ERROR);
          }
          return;
        }
        onCreated(res.data.token);
      } catch {
        setFormError(NETWORK_ERROR);
      } finally {
        setPending(false);
      }
    },
  });

  <form
    method="post"
    action="#"
    className="mt-8 flex flex-col gap-6"
    data-testid="pat-classic-form"
    onSubmit={(e) => {
      e.preventDefault();
      void form.handleSubmit();
    }}
  >
    <form.Field name="note">
      {(field) => (
        <div className="flex flex-col gap-2">
          <Label htmlFor="pat-classic-note">Note</Label>
          <Input
            id="pat-classic-note"
            value={field.state.value}
            aria-label="Note"
            autoComplete="off"
            disabled={pending}
            onBlur={field.handleBlur}
            onInput={(e) => field.handleChange((e.currentTarget as HTMLInputElement).value)}
          />
          <p className="text-[14px] text-muted-foreground">
            What's this token for? e.g. laptop, CI
          </p>
          <p
            role="alert"
            className={noteError ? "text-[14px] text-destructive" : "hidden"}
            data-testid="pat-classic-note-error"
          >
            {(noteError || "") as string}
          </p>
        </div>
      )}
    </form.Field>

    <form.Field name="scopes">
      {(field) => (
        <ClassicScopeGroups
          values={field.state.value}
          pending={pending}
          onToggle={(scope, checked) =>
            field.handleChange({ ...field.state.value, [scope]: checked })
          }
        />
      )}
    </form.Field>

    <p
      role="alert"
      className={scopeError ? "text-[14px] text-destructive" : "hidden"}
      data-testid="pat-classic-scope-error"
    >
      {(scopeError || "") as string}
    </p>

    <PatExpiryFields form={form} disabled={pending} />

    <form.Subscribe
      selector={(s) => ({
        scopes: s.values.scopes,
        expiryPreset: s.values.expiryPreset,
      })}
    >
      {(v) => {
        const parts = selectedScopes(v.scopes);
        const scopeText = parts.length > 0 ? parts.join(", ") : "no scopes";
        const expText =
          v.expiryPreset === "none"
            ? "no expiration"
            : v.expiryPreset === "custom"
              ? "a custom expiration date"
              : `expiration in ${v.expiryPreset} days`;
        return (
          <p className="text-[14px] text-muted-foreground" data-testid="pat-classic-summary">
            {`This token will have ${scopeText}, with ${expText}.` as string}
          </p>
        );
      }}
    </form.Subscribe>

    <p
      role="alert"
      className={formError ? "text-[16px] text-destructive" : "hidden"}
      data-testid="pat-classic-form-error"
    >
      {(formError || "") as string}
    </p>

    <form.Subscribe selector={(s) => selectedScopes(s.values.scopes).length}>
      {(count) => (
        <Button type="submit" className="h-11 w-fit" disabled={pending || count === 0}>
          {(pending ? "Generating…" : "Generate token") as string}
        </Button>
      )}
    </form.Subscribe>
  </form>;
}
