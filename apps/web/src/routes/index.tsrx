import { createFileRoute, Link, useLoaderData } from "@octanejs/tanstack-router";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { buttonVariants } from "@/components/ui/button";
import { OxideanMark } from "@/components/oxidean-mark";
import { SignedInHomeSkeleton } from "@/components/page-skeletons";
import { SignedInHome } from "@/components/signed-in-home";
import { fetchProviderConfig, fetchRepoListMine, fetchSessionMe } from "@/lib/ssr-auth";
import { hasSessionPresenceHint, syncSessionPresenceHint } from "@/lib/session-hint";
import { cn } from "@/lib/utils";

/** D-20 home tree priority after shared root access gate. */
export type HomeTree = "setup" | "signed-in" | "marketing";

export function selectHomeTree(input: { needs_setup: boolean; hasSession: boolean }): HomeTree {
  if (input.needs_setup) return "setup";
  if (input.hasSession) return "signed-in";
  return "marketing";
}

export type HomeLoaderData = {
  tree: Exclude<HomeTree, "setup">;
  user: UserPublic | null;
  /** Present when tree === "signed-in". */
  repos: RepoPublic[];
  reposError: string | null;
  /** Marketing CTA — SSR'd so signup button paints without client fetch. */
  allowSignup: boolean;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export const Route = createFileRoute("/")({
  component: HomePage,
  head: ({ loaderData }) => ({
    meta: [
      {
        title:
          (loaderData as HomeLoaderData | undefined)?.tree === "signed-in"
            ? "Repositories · Oxidean"
            : "Oxidean",
      },
    ],
  }),
  loader: async (): Promise<HomeLoaderData> => {
    // needs_setup / must_change are owned by root loader — do not re-lock here.
    let user: UserPublic | null = null;
    try {
      const me = await fetchSessionMe();
      if (me.ok) user = me.data;
    } catch {
      /* anonymous */
    }

    const tree = selectHomeTree({
      needs_setup: false,
      hasSession: user != null,
    }) as Exclude<HomeTree, "setup">;

    let repos: RepoPublic[] = [];
    let reposError: string | null = null;
    let allowSignup = false;

    if (tree === "signed-in") {
      try {
        const list = await fetchRepoListMine();
        if (list.ok) {
          repos = list.data.repos;
        } else {
          reposError = list.error.message || "Could not load repositories.";
        }
      } catch {
        reposError = NETWORK_ERROR;
      }
    } else {
      try {
        const cfg = await fetchProviderConfig();
        if (cfg.ok) allowSignup = cfg.data.allow_signup === true;
      } catch {
        allowSignup = false;
      }
    }

    return { tree, user, repos, reposError, allowSignup };
  },
});

function revealExploreTarget() {
  const el = document.getElementById("explore");
  if (!el) return;
  // Finish translateY before hash scroll so middle/wheel landing doesn’t fight the reveal.
  el.classList.add("oct-reveal-in");
  el.style.transition = "none";
  // Re-enable transitions for later (no-op if already revealed).
  requestAnimationFrame(() => {
    el.style.transition = "";
  });
}

function useScrollReveal(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    const nodes = Array.from(document.querySelectorAll<HTMLElement>(".oct-reveal"));
    if (typeof IntersectionObserver === "undefined") {
      nodes.forEach((n) => n.classList.add("oct-reveal-in"));
      return;
    }

    if (location.hash === "#explore") revealExploreTarget();

    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          entry.target.classList.add("oct-reveal-in");
          io.unobserve(entry.target);
        }
      },
      { threshold: 0, rootMargin: "0px 0px 20% 0px" },
    );
    nodes.forEach((n) => io.observe(n));
    return () => io.disconnect();
  }, [enabled]);
}

function HomePage() {
  const data = useLoaderData({ from: "/" }) as HomeLoaderData | undefined;
  // Cookie is only readable after mount — reading during render mismatches SSR.
  const [presenceHint, setPresenceHint] = useState(false);

  useEffect(() => {
    setPresenceHint(hasSessionPresenceHint());
  }, []);

  useEffect(() => {
    if (!data) return;
    syncSessionPresenceHint(data.tree === "signed-in");
  }, [data]);

  const tree = data?.tree;
  const user = data?.user;

  useScrollReveal(tree === "marketing" || (tree == null && !presenceHint));

  // Rare PE: presence cookie says signed-in before loader hydrates.
  if (tree == null && presenceHint) {
    return <SignedInHomeSkeleton />;
  }

  if (tree === "signed-in" && user) {
    return (
      <SignedInHome user={user} repos={data?.repos ?? []} reposError={data?.reposError ?? null} />
    );
  }

  return <AnonLanding allowSignup={data?.allowSignup === true} />;
}

function AnonLanding({ allowSignup }: { allowSignup: boolean }) @{
  <>
    <section className="relative min-h-[32rem] overflow-hidden md:min-h-[36rem] lg:min-h-[40rem]">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_12%_18%,color-mix(in_srgb,var(--primary)_42%,transparent),transparent_52%),radial-gradient(ellipse_at_88%_22%,color-mix(in_srgb,var(--secondary)_34%,transparent),transparent_48%),radial-gradient(ellipse_at_50%_100%,color-mix(in_srgb,var(--card)_80%,transparent),transparent_55%),linear-gradient(165deg,var(--background),color-mix(in_srgb,var(--card)_70%,var(--background)))]"
      />
      <div className="relative mx-auto flex min-h-[32rem] max-w-6xl flex-col justify-center px-4 py-14 sm:py-16 md:min-h-[36rem] lg:min-h-[40rem]">
        <div className="oct-rise" style={{ animationDelay: "0ms" }}>
          <OxideanMark size={96} />
        </div>
        <h1
          className="oct-rise mt-6 max-w-3xl font-[family-name:var(--font-display)] text-[clamp(1.75rem,5vw,2.5rem)] font-semibold leading-[1.15] text-foreground"
          style={{ animationDelay: "80ms" }}
        >
          Where repositories nest — cloud or yours
        </h1>
        <div className="oct-rise" style={{ animationDelay: "160ms" }}>
          <p className="mt-4 max-w-2xl text-[16px] text-muted-foreground">
            Social coding forge: host git, collaborate, and ship — one product for Oxidean Cloud and
            self-host
          </p>
          <div className="mt-8 flex flex-wrap gap-3">
            @if (allowSignup) {
              <Link to="/signup" preload="intent" className={cn(buttonVariants())}>
                Get started
              </Link>;
            }
            <Link
              to="/explore"
              preload="intent"
              className={cn(buttonVariants({ variant: "secondary" }))}
            >
              Explore
            </Link>
          </div>
        </div>
      </div>
    </section>

    <section id="explore" className="scroll-mt-32 py-24 oct-reveal md:scroll-mt-20">
      <div className="mx-auto grid max-w-6xl gap-12 px-4 md:grid-cols-2">
        <div className="border-l-[3px] border-primary pl-6">
          <h2 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Oxidean Cloud
          </h2>
          <p className="mt-4 text-[16px] text-muted-foreground">
            Hosted forge with the same product you run yourself — one release train.
          </p>
        </div>
        <div className="border-l-[3px] border-secondary pl-6">
          <h2 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Self-host
          </h2>
          <p className="mt-4 text-[16px] text-muted-foreground">
            Run Oxidean on your machines with Docker Compose — your data, your network.
          </p>
        </div>
      </div>
    </section>

    <section className="py-24 oct-reveal">
      <div className="mx-auto grid max-w-6xl gap-12 px-4 md:grid-cols-3">
        <div>
          <h3 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Host git
          </h3>
          <p className="mt-4 text-[16px] text-muted-foreground">
            Browse repos and history in the browser.
          </p>
        </div>
        <div>
          <h3 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Collaborate
          </h3>
          <p className="mt-4 text-[16px] text-muted-foreground">
            Issues and pull requests for teams.
          </p>
        </div>
        <div>
          <h3 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Ship
          </h3>
          <p className="mt-4 text-[16px] text-muted-foreground">
            Releases and CI when you're ready to ship.
          </p>
        </div>
      </div>
    </section>

    <section className="border-t border-border bg-card/50 py-24">
      <div className="mx-auto max-w-6xl px-4">
        <h2 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
          One forge. Cloud or yours.
        </h2>
        <div className="mt-8 flex flex-wrap gap-3">
          @if (allowSignup) {
            <Link to="/signup" preload="intent" className={cn(buttonVariants())}>
              Get started
            </Link>;
          }
          <Link
            to="/explore"
            preload="intent"
            className={cn(buttonVariants({ variant: "secondary" }))}
          >
            Explore
          </Link>
        </div>
      </div>
    </section>
  </>;
}
