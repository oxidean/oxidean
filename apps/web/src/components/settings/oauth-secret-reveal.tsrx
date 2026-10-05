import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type OAuthSecretRevealProps = {
  /** One-time plaintext `client_secret` — shown once, never stored server-side. */
  secret: string;
};

/** One-time OAuth client_secret reveal — same posture as the PAT reveal. */
export function OAuthSecretReveal({ secret }: OAuthSecretRevealProps) @{
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  async function copySecret() {
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("clipboard unavailable");
      }
      await navigator.clipboard.writeText(secret);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 2000);
    } catch {
      setCopyState("error");
    }
  }

  <div className="rounded-md border border-destructive/40 bg-card p-6">
    <h3 className="font-[family-name:var(--font-display)] text-[18px] font-semibold leading-[1.2] text-foreground">
      Copy your client secret now
    </h3>
    <p className="mt-2 text-[14px] text-muted-foreground">
      You won’t be able to see it again. Store it somewhere safe — Oxidean only keeps a hash.
    </p>
    <div className="mt-4 flex flex-col gap-3 sm:flex-row sm:items-stretch">
      <Input
        readOnly
        value={secret as string}
        aria-label="Client secret"
        className="min-w-0 flex-1 font-mono text-[13px]"
        onFocus={(e) => {
          const el = e.currentTarget as HTMLInputElement;
          el.select();
        }}
      />
      <Button
        type="button"
        variant="outline"
        className="shrink-0"
        aria-label="Copy client secret"
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
  </div>;
}
