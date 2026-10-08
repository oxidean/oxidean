import { useQuery } from "@octanejs/tanstack-query";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { useEffect } from "octane";
import { AppPage } from "@/components/app-page";
import { AppLink } from "@/components/ui/app-link";
import { buttonVariants } from "@/components/ui/button";
import { OxideanMark } from "@/components/oxidean-mark";
import { SignedInHomeSkeleton } from "@/components/page-skeletons";
import { SignedInHome } from "@/components/signed-in-home";
import { hasSessionPresenceHint, syncSessionPresenceHint } from "@/lib/session-hint";
import {
  authProviderConfigQueryOptions,
  authSessionQueryOptions,
  repoListMineQueryOptions,
} from "@/lib/session-queries";
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

export function HomePage() @{
  <AppPage>
    <HomeContent />
  </AppPage>;
}

function HomeContent() {
  // needs_setup / must_change are owned by AppAccessGate — do not re-lock here.
  const meQ = useQuery(authSessionQueryOptions());
  const signedIn = meQ.data != null;
  const reposQ = useQuery({ ...repoListMineQueryOptions(), enabled: signedIn });
  const cfgQ = useQuery(authProviderConfigQueryOptions());

  const tree: HomeTree | null = meQ.isPending ? null : signedIn ? "signed-in" : "marketing";
  const user = meQ.data ?? null;

  useEffect(() => {
    if (tree != null) syncSessionPresenceHint(tree === "signed-in");
  }, [tree]);

  // Cookie is only readable after mount — reading it for reveal gating feeds an
  // effect arg, not SSR markup, so it's safe client-side.
  const sessionHint = typeof document !== "undefined" ? hasSessionPresenceHint() : false;
  useScrollReveal(tree === "marketing" || (tree == null && !sessionHint));

  // Pending: SSR cannot see the session, so emit both candidates — the
  // `data-oxidean-session` stamp on <html> (set per-request by the web tier)
  // selects via CSS which one paints before `auth.me` resolves.
  if (tree == null) {
    return (
      <>
        <div data-anon-landing>
          <AnonLanding allowSignup={cfgQ.data?.allow_signup === true} />
        </div>
        <div data-home-skeleton>
          <SignedInHomeSkeleton />
        </div>
      </>
    );
  }

  if (tree === "signed-in" && user) {
    return (
      <SignedInHome
        user={user}
        repos={reposQ.data ?? []}
        reposError={reposQ.isError ? (reposQ.error?.message ?? NETWORK_ERROR) : null}
      />
    );
  }

  return <AnonLanding allowSignup={cfgQ.data?.allow_signup === true} />;
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
              <AppLink href="/signup" className={cn(buttonVariants())}>
                Get started
              </AppLink>;
            }
            <AppLink href="/explore" className={cn(buttonVariants({ variant: "secondary" }))}>
              Explore
            </AppLink>
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
            <AppLink href="/signup" className={cn(buttonVariants())}>
              Get started
            </AppLink>;
          }
          <AppLink href="/explore" className={cn(buttonVariants({ variant: "secondary" }))}>
            Explore
          </AppLink>
        </div>
      </div>
    </section>
  </>;
}
