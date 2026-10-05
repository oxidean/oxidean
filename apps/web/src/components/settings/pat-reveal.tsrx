import { useState } from "octane";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

type PatRevealProps = {
  token: string;
};

/** One-time plaintext PAT reveal (D-15 / T-08-01). */
export function PatReveal({ token }: PatRevealProps) @{
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  async function copyToken() {
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("clipboard unavailable");
      }
      await navigator.clipboard.writeText(token);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 2000);
    } catch {
      setCopyState("error");
    }
  }

  <div className="rounded-md border border-destructive/40 bg-card p-6">
    <h2 className="font-[family-name:var(--font-display)] text-[20px] font-semibold leading-[1.2] text-foreground">
      Make sure to copy your personal access token now
    </h2>
    <p className="mt-2 text-[16px] text-muted-foreground">You won’t be able to see it again.</p>
    <div className="mt-6 flex flex-col gap-3 sm:flex-row sm:items-stretch">
      <Input
        readOnly
        value={token as string}
        aria-label="Personal access token"
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
        aria-label="Copy token"
        onClick={() => void copyToken()}
      >
        {(copyState === "copied" ? "Copied" : "Copy token") as string}
      </Button>
    </div>
    @if (copyState === "error") {
      <p role="alert" className="mt-2 text-[14px] text-destructive">
        Couldn’t copy. Select the token and copy manually.
      </p>;
    }
    <AppLink
      href="/settings/tokens"
      className={cn(
        buttonVariants({ variant: "default" }),
        "mt-6 inline-flex h-11 w-fit items-center",
      )}
    >
      Back to tokens
    </AppLink>
  </div>;
}
