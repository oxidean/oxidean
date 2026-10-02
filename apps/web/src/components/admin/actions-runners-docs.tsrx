import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

type ActionsRunnersDocsProps = {
  className?: string;
  /** Show link to instance admin runners (repo settings). */
  showAdminLink?: boolean;
  /** Compact variant for repo settings panels. */
  compact?: boolean;
};

/**
 * In-UI operator docs for self-hosted Actions runners (ACT-04…ACT-07 / D-ACT-08…10).
 * No managed CI minutes — operators bring compute.
 */
export function ActionsRunnersDocs({
  className,
  showAdminLink = false,
  compact = false,
}: ActionsRunnersDocsProps) @{
  <div
    className={cn(
      "rounded-md border border-border bg-card text-card-foreground",
      compact ? "p-4" : "p-5",
      className,
    )}
    data-testid="actions-runners-docs"
  >
    <h2 className={cn("font-semibold text-foreground", compact ? "text-[15px]" : "text-[18px]")}>
      How runners work
    </h2>
    <p className="mt-2 text-[14px] leading-relaxed text-muted-foreground">
      Oxidean is a control plane: it discovers workflows under{" "}
      <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
        .github/workflows/
      </code>
      , queues jobs on push and pull request, and waits for{" "}
      <strong className="font-medium text-foreground">registered self-hosted runners</strong> to
      claim them. There is no managed CI minutes product and no in-process job executor.
    </p>

    <ol className="mt-4 list-decimal space-y-3 pl-5 text-[14px] leading-relaxed text-muted-foreground">
      <li>
        <span className="font-medium text-foreground">Mint a registration token</span>
        {" — "}
        Instance admins create a one-time token on this page (or set{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          OXIDEAN_RUNNER_REGISTRATION_TOKEN
        </code>{" "}
        for Compose bootstrap). Copy it immediately; it is not shown again.
      </li>
      <li>
        <span className="font-medium text-foreground">Start the official runner image</span>
        {" — "}
        Use the{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          oxidean-runner
        </code>{" "}
        image with your public forge origin — not{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          localhost
        </code>{" "}
        if job containers must check out from the forge.
      </li>
      <li>
        <span className="font-medium text-foreground">Match workflow labels</span>
        {" — "}
        Jobs use{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          runs-on
        </code>
        . Runners declare labels as{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          label[:schema[:args]]
        </code>
        , e.g.{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          ubuntu-latest:docker://node:20-bookworm
        </code>
        . Queued jobs wait until a runner with a matching label calls{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px] text-foreground">
          fetch_task
        </code>
        .
      </li>
      <li>
        <span className="font-medium text-foreground">Enable Actions per repository</span>
        {" — "}
        Repo admins toggle Actions under repository Settings → Actions and store write-only secrets
        for workflows.
      </li>
    </ol>

    @if (!compact) {
      <div className="mt-5 space-y-3">
        <h3 className="text-[15px] font-semibold text-foreground">Quick start (Compose)</h3>
        <pre className="overflow-x-auto rounded-md border border-border bg-muted/50 p-3 font-mono text-[12px] leading-relaxed text-foreground">
          {`export OXIDEAN_RUNNER_REGISTRATION_TOKEN=reg_…
docker compose --profile actions up -d --build runner`}
        </pre>
        <h3 className="text-[15px] font-semibold text-foreground">Standalone</h3>
        <pre className="overflow-x-auto rounded-md border border-border bg-muted/50 p-3 font-mono text-[12px] leading-relaxed text-foreground">
          {`export OXIDEAN_PUBLIC_ORIGIN=https://git.example.com
export OXIDEAN_RUNNER_REGISTRATION_TOKEN=reg_…
export OXIDEAN_RUNNER_NAME=runner-1
export OXIDEAN_RUNNER_LABELS='ubuntu-latest:docker://node:20-bookworm,self-hosted'
docker run --rm -it \\
  -e OXIDEAN_PUBLIC_ORIGIN \\
  -e OXIDEAN_RUNNER_REGISTRATION_TOKEN \\
  -e OXIDEAN_RUNNER_NAME \\
  -e OXIDEAN_RUNNER_LABELS \\
  -v oxidean-runner-data:/data \\
  -v /var/run/docker.sock:/var/run/docker.sock \\
  oxidean-runner`}
        </pre>
        <p className="text-[13px] leading-relaxed text-muted-foreground">
          Mount the Docker socket only on an isolated runner host — not on the API container.
          Rotate compromised registration tokens immediately. Runner bearer tokens (
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-[12px] text-foreground">
            ort_…
          </code>
          ) are returned once at register; session cookies are ignored on{" "}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-[12px] text-foreground">
            /api/actions/*
          </code>
          .
        </p>
      </div>;
    }

    @if (showAdminLink) {
      <p className="mt-4 text-[14px] text-muted-foreground">
        Instance admins register runners under{" "}
        <AppLink
          href="/admin/runners"
          className="font-medium text-primary no-underline hover:underline"
        >
          Admin → Actions runners
        </AppLink>
        .
      </p>;
    }
  </div>;
}
