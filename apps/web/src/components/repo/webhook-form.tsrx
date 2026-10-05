import type { WebhookPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const EVENT_OPTIONS = [
  { id: "push", label: "push" },
  { id: "pull_request", label: "pull_request" },
  { id: "issues", label: "issues" },
  { id: "issue_comment", label: "issue_comment" },
  { id: "release", label: "release" },
  { id: "star", label: "star" },
  { id: "fork", label: "fork" },
  { id: "create", label: "create" },
  { id: "delete", label: "delete" },
  { id: "workflow_run", label: "workflow_run" },
  { id: "registry_package", label: "registry_package" },
] as const;

type EventId = (typeof EVENT_OPTIONS)[number]["id"];

function SecretReveal(props: { secret: string; onDone: () => void }) @{
  const secret = props.secret;
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  async function copySecret() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
      await navigator.clipboard.writeText(secret);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 2000);
    } catch {
      setCopyState("error");
    }
  }

  <div
    className="rounded-md border border-destructive/40 bg-card p-4"
    data-testid="webhook-secret-reveal"
  >
    <h4 className="text-[15px] font-semibold text-foreground">Copy your webhook secret now</h4>
    <p className="mt-1 text-[14px] text-muted-foreground">
      You won’t be able to see it again. Store it with your receiver to verify signatures.
    </p>
    <div className="mt-4 flex flex-col gap-3 sm:flex-row sm:items-stretch">
      <Input
        readOnly
        value={secret as string}
        aria-label="Webhook secret"
        className="min-w-0 flex-1 font-mono text-[13px]"
        onFocus={(e) => {
          const el = e.currentTarget as HTMLInputElement;
          el.select();
        }}
      />
      <Button
        type="button"
        variant="secondary"
        className="shrink-0"
        onClick={() => void copySecret()}
      >
        {(copyState === "copied" ? "Copied" : "Copy secret") as string}
      </Button>
    </div>
    @if (copyState === "error") {
      <p role="alert" className="mt-2 text-[14px] text-destructive">
        Couldn’t copy. Select the secret and copy manually.
      </p>;
    }
    <Button type="button" className="mt-4 w-full sm:w-auto" onClick={props.onDone}>
      Done
    </Button>
  </div>;
}

/** Create / edit outbound webhook (HOOK-01 / D-HOOK-03 / D-HOOK-16 / D-HOOK-19). */
export function WebhookForm(props: {
  owner: string;
  name: string;
  /** When set, form updates this hook; otherwise creates. */
  initial?: WebhookPublic | null;
  onCancel: () => void;
  onSaved: (hook: WebhookPublic) => void;
}) @{
  const owner = props.owner;
  const name = props.name;
  const initial = props.initial ?? null;
  const isEdit = !!initial;

  const [url, setUrl] = useState(initial?.url ?? "");
  const [secret, setSecret] = useState("");
  const [description, setDescription] = useState(initial?.name ?? "");
  const [active, setActive] = useState(initial?.active ?? true);
  const [events, setEvents] = useState<Record<EventId, boolean>>(() => {
    const set = new Set(initial?.events ?? ["push", "pull_request", "issues", "issue_comment"]);
    return {
      push: set.has("push"),
      pull_request: set.has("pull_request"),
      issues: set.has("issues"),
      issue_comment: set.has("issue_comment"),
      release: set.has("release"),
      star: set.has("star"),
      fork: set.has("fork"),
      create: set.has("create"),
      delete: set.has("delete"),
      workflow_run: set.has("workflow_run"),
      registry_package: set.has("registry_package"),
    };
  });
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [revealedSecret, setRevealedSecret] = useState<string | null>(null);
  const [savedHook, setSavedHook] = useState<WebhookPublic | null>(null);

  function toggleEvent(id: EventId, on: boolean) {
    setEvents((prev) => ({ ...prev, [id]: on }));
  }

  function selectedEvents(): string[] {
    return EVENT_OPTIONS.filter((o) => events[o.id]).map((o) => o.id);
  }

  async function submit() {
    setError("");
    const trimmedUrl = url.trim();
    if (!trimmedUrl) {
      setError("Payload URL is required.");
      return;
    }
    const ev = selectedEvents();
    if (ev.length === 0) {
      setError("Select at least one event.");
      return;
    }
    if (!isEdit && !secret.trim()) {
      setError("Secret is required.");
      return;
    }

    setPending(true);
    try {
      if (isEdit && initial) {
        const res = await apiClient.webhook.update({
          owner,
          name,
          id: initial.id,
          url: trimmedUrl,
          secret: secret.trim() ? secret.trim() : null,
          events: ev,
          active,
          description: description.trim() ? description.trim() : null,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not update webhook.");
          setPending(false);
          return;
        }
        toastSuccess("Webhook updated.");
        if (res.data.secret) {
          setSavedHook(res.data);
          setRevealedSecret(res.data.secret);
          setPending(false);
          return;
        }
        setPending(false);
        props.onSaved(res.data);
      } else {
        const res = await apiClient.webhook.create({
          owner,
          name,
          url: trimmedUrl,
          secret: secret.trim(),
          events: ev,
          active,
          description: description.trim() ? description.trim() : null,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not create webhook.");
          setPending(false);
          return;
        }
        toastSuccess("Webhook created.");
        if (res.data.secret) {
          setSavedHook(res.data);
          setRevealedSecret(res.data.secret);
          setPending(false);
          return;
        }
        setPending(false);
        props.onSaved(res.data);
      }
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  @if (revealedSecret && savedHook) {
    <SecretReveal
      secret={revealedSecret}
      onDone={() => {
        const hook = savedHook;
        setRevealedSecret(null);
        setSavedHook(null);
        props.onSaved(hook);
      }}
    />;
  } @else {
    <form
      method="post"
      action="#"
      className="flex flex-col gap-4 rounded-md border border-border p-4"
      data-testid="webhook-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h3 className="text-[16px] font-semibold text-foreground">
        {(isEdit ? "Edit webhook" : "Add webhook") as string}
      </h3>

      <div className="flex flex-col gap-2">
        <Label htmlFor="webhook-url">Payload URL</Label>
        <Input
          id="webhook-url"
          type="url"
          value={url}
          onInput={(e) => setUrl((e.currentTarget as HTMLInputElement).value)}
          placeholder="https://example.com/hooks/oxidean"
          autoComplete="off"
          disabled={pending}
          required
        />
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="webhook-secret">
          {(isEdit ? "Secret (leave blank to keep)" : "Secret") as string}
        </Label>
        <Input
          id="webhook-secret"
          type="password"
          value={secret}
          onInput={(e) => setSecret((e.currentTarget as HTMLInputElement).value)}
          autoComplete="new-password"
          disabled={pending}
          placeholder={isEdit ? "••••••••" : undefined}
        />
        <p className="text-[13px] text-muted-foreground">
          Used for HMAC-SHA256 signing (X-Hub-Signature-256). Required on create; rotate by entering
          a new value.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="webhook-description">Description (optional)</Label>
        <Input
          id="webhook-description"
          value={description}
          onInput={(e) => setDescription((e.currentTarget as HTMLInputElement).value)}
          autoComplete="off"
          disabled={pending}
        />
      </div>

      <fieldset className="flex flex-col gap-2">
        <legend className="text-[14px] font-medium text-foreground">Events</legend>
        @for (const opt of EVENT_OPTIONS; key opt.id) {
          <label className="flex cursor-pointer items-center gap-3">
            <Checkbox
              checked={events[opt.id]}
              disabled={pending}
              aria-label={opt.label}
              onCheckedChange={(checked) => toggleEvent(opt.id, checked === true)}
            />
            <span className="text-[14px] text-foreground">{opt.label as string}</span>
          </label>;
        }
      </fieldset>

      <div className="flex items-center gap-3">
        <Switch
          id="webhook-active"
          checked={active}
          disabled={pending}
          onCheckedChange={(v) => setActive(!!v)}
        />
        <Label htmlFor="webhook-active">Active</Label>
      </div>

      @if (error) {
        <p role="alert" className="text-[14px] text-destructive">
          {error as string}
        </p>;
      }

      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={pending}>
          {(pending ? "Saving…" : isEdit ? "Save webhook" : "Create webhook") as string}
        </Button>
        <Button type="button" variant="secondary" disabled={pending} onClick={props.onCancel}>
          Cancel
        </Button>
      </div>
    </form>;
  }
}
