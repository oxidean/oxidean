import { PatHowTo } from "@/components/repo/pat-how-to";
import { AppLink } from "@/components/ui/app-link";
import { httpsCloneUrl } from "@/lib/public-origin";
import { useRepoStore } from "@/lib/repo-store";

type QuickSetupProps = {
  owner: string;
  repo: string;
  /** Optional override for tests; otherwise SSR layout store origin. */
  publicOrigin?: string;
};

/** Empty-repo first-push guide (D-15 / UI-SPEC Quick setup). */
export function QuickSetup({
  owner,
  repo,
  publicOrigin: publicOriginProp,
}: QuickSetupProps) @{
  const storeOrigin = useRepoStore((s) => s.publicOrigin);
  const httpsUrl = httpsCloneUrl(publicOriginProp || storeOrigin, owner, repo);
  const remoteCmd = `git remote add origin ${httpsUrl}`;
  const pushCmd = `git push -u origin HEAD`;
  const setupScript = `echo "# ${repo}" >> README.md
git init
git add README.md
git commit -m "first commit"
${remoteCmd}
${pushCmd}`;

  <section
    className="rounded-lg border border-border bg-card px-6 py-8 shadow-sm"
    aria-labelledby="quick-setup-heading"
  >
    <h2
      id="quick-setup-heading"
      className="font-[family-name:var(--font-display)] text-[20px] font-semibold leading-[1.3] text-foreground"
    >
      Quick setup
    </h2>
    <p className="mt-2 max-w-2xl text-[16px] leading-relaxed text-muted-foreground">
      This repository is empty. Push an existing repository from the command line using the HTTPS
      remote below.
    </p>

    <div className="mt-6 flex flex-col gap-5">
      <div className="flex flex-col gap-2">
        <p className="text-[14px] font-medium text-foreground">HTTPS</p>
        <code className="block overflow-x-auto rounded-md border border-border bg-muted/50 px-3 py-2.5 font-mono text-[13px] text-foreground">
          {httpsUrl as string}
        </code>
      </div>

      <PatHowTo httpsUrl={httpsUrl as string} />

      <div className="flex flex-col gap-2">
        <p className="text-[14px] font-medium text-foreground">
          …or create a new repository on the command line
        </p>
        <pre className="overflow-x-auto rounded-md border border-border bg-muted/50 px-3 py-3.5 font-mono text-[13px] leading-[1.55] text-foreground whitespace-pre-wrap">
          {setupScript as string}
        </pre>
      </div>

      <p className="text-[14px] text-muted-foreground">
        Prefer starting from the form?{" "}
        <AppLink
          href="/new"
          className="font-semibold text-primary underline-offset-4 hover:underline"
        >
          Create another repository
        </AppLink>
      </p>
    </div>
  </section>;
}
