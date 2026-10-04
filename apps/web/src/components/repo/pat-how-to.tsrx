import { AppLink } from "@/components/ui/app-link";

type PatHowToProps = {
  /** HTTPS clone URL for the example `git clone` block (D-13 / D-19). */
  httpsUrl: string;
  /** Compact layout inside CloneBox dropdown. */
  compact?: boolean;
};

/**
 * Shared HTTPS PAT credential how-to (D-13 / GIT-02).
 * Username aliases: account username / git / token / oauth2; password is the PAT.
 * AppLink CTA renders a plain `<a>` without a RouterProvider, so
 * CloneBox/QuickSetup keep working in tests;
 * unauthenticated users are redirected by the tokens settings route.
 */
export function PatHowTo({ httpsUrl, compact }: PatHowToProps) @{
  const cloneExample = `git clone ${httpsUrl}`;
  const pad = compact ? "mt-3 px-1" : "mt-0";

  <div className={pad as string} data-slot="pat-how-to">
    <h3
      className={
        (compact
          ? "text-[14px] font-semibold text-foreground"
          : "text-[14px] font-medium text-foreground") as string
      }
    >
      Authenticate with a personal access token
    </h3>
    <ul className="mt-2 flex flex-col gap-1.5 text-[13px] leading-relaxed text-muted-foreground">
      <li>Username: your Oxidean username, or `git`, `token`, or `oauth2`.</li>
      <li>Password: a personal access token — not your account password.</li>
    </ul>
    <p className="mt-3">
      <AppLink
        href="/settings/tokens"
        className="text-[14px] font-semibold text-primary underline-offset-4 hover:underline"
      >
        Create a personal access token
      </AppLink>
    </p>
    <p className="mt-3 text-[13px] text-muted-foreground">
      When Git asks for a password, paste your token.
    </p>
    <code className="mt-2 block overflow-x-auto whitespace-pre-wrap break-all rounded-md border border-border bg-muted/50 px-3 py-2.5 font-mono text-[13px] text-foreground">
      {cloneExample as string}
    </code>
  </div>;
}
