import { useForm, useStore } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { PatExpiryFields } from "@/components/settings/pat-expiry-field";
import {
  resolveExpiresAt,
  tomorrowDateValue,
  type PatExpiryPreset,
} from "@/components/settings/pat-expiry";
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
      repoChecked: true,
      packageRead: false,
      packageWrite: false,
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
      if (!value.repoChecked) {
        setScopeError("Full control of private repositories is required.");
        return;
      }
      setPending(true);
      try {
        const scopes: string[] = ["repo"];
        if (value.packageWrite) {
          scopes.push("package:write");
        } else if (value.packageRead) {
          scopes.push("package:read");
        }
        const res = await apiClient.pat.createClassic({
          name: trimmed,
          scopes: scopes as ("repo" | "package:read" | "package:write")[],
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

  // packageWrite must be read reactively at component scope — a form.Subscribe
  // nested under a Field render-prop crashes under octane 0.10 (function
  // children get invoked eagerly by the children resolver).
  const packageWrite = useStore(form.store, (s) => s.values.packageWrite);

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

    <div className="flex flex-col gap-3" data-testid="pat-classic-scopes">
      <p className="text-[14px] font-medium text-foreground">Scopes</p>

      <div className="flex flex-col gap-2">
        <p className="text-[13px] font-medium uppercase tracking-wide text-muted-foreground">
          Repository
        </p>
        <form.Field name="repoChecked">
          {(field) => (
            <label className="flex cursor-pointer items-start gap-3">
              <Checkbox
                checked={field.state.value}
                disabled={pending}
                aria-label="Full control of private repositories"
                data-testid="scope-repo"
                onCheckedChange={(checked) => field.handleChange(checked === true)}
              />
              <span className="flex flex-col gap-0.5">
                <span className="text-[14px] text-foreground">
                  repo — Full control of private repositories
                </span>
                <span className="text-[13px] text-muted-foreground">
                  Required for git clone, fetch, and push over HTTPS.
                </span>
              </span>
            </label>
          )}
        </form.Field>
      </div>

      <div className="flex flex-col gap-2">
        <p className="text-[13px] font-medium uppercase tracking-wide text-muted-foreground">
          Packages
        </p>
        <form.Field name="packageRead">
          {(field) => (
            <label className="flex cursor-pointer items-start gap-3">
              <Checkbox
                checked={field.state.value || packageWrite}
                disabled={pending || packageWrite}
                aria-label="package:read"
                data-testid="scope-package-read"
                onCheckedChange={(checked) => field.handleChange(checked === true)}
              />
              <span className="flex flex-col gap-0.5">
                <span className="text-[14px] text-foreground">
                  package:read — Download packages
                </span>
                <span className="text-[13px] text-muted-foreground">
                  Registry download only; does not include repository access by itself.
                </span>
              </span>
            </label>
          )}
        </form.Field>
        <form.Field name="packageWrite">
          {(field) => (
            <label className="flex cursor-pointer items-start gap-3">
              <Checkbox
                checked={field.state.value}
                disabled={pending}
                aria-label="package:write"
                data-testid="scope-package-write"
                onCheckedChange={(checked) => {
                  const on = checked === true;
                  field.handleChange(on);
                  if (on) form.setFieldValue("packageRead", true);
                }}
              />
              <span className="flex flex-col gap-0.5">
                <span className="text-[14px] text-foreground">
                  package:write — Publish and manage packages
                </span>
                <span className="text-[13px] text-muted-foreground">
                  Implies package:read. Publish, overwrite, and delete package versions.
                </span>
              </span>
            </label>
          )}
        </form.Field>
      </div>

      <p
        role="alert"
        className={scopeError ? "text-[14px] text-destructive" : "hidden"}
        data-testid="pat-classic-scope-error"
      >
        {(scopeError || "") as string}
      </p>
    </div>

    <PatExpiryFields form={form} disabled={pending} />

    <form.Subscribe
      selector={(s) => ({
        repo: s.values.repoChecked,
        packageRead: s.values.packageRead,
        packageWrite: s.values.packageWrite,
        expiryPreset: s.values.expiryPreset,
      })}
    >
      {(v) => {
        const parts: string[] = [];
        if (v.repo) parts.push("repo");
        if (v.packageWrite) parts.push("package:write");
        else if (v.packageRead) parts.push("package:read");
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

    <form.Subscribe selector={(s) => s.values.repoChecked}>
      {(repoChecked) => (
        <Button type="submit" className="h-11 w-fit" disabled={pending || !repoChecked}>
          {(pending ? "Generating…" : "Generate token") as string}
        </Button>
      )}
    </form.Subscribe>
  </form>;
}
