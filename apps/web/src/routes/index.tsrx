import { useQuery } from "@octanejs/tanstack-query";
import { memo, useEffect } from "octane";
import { AppPage } from "@/components/app-page";
import { AppLink } from "@/components/ui/app-link";
import { buttonVariants } from "@/components/ui/button";
import { OxideanMark } from "@/components/oxidean-mark";
import { SignedInHomeSkeleton } from "@/components/page-skeletons";
import { SignedInHome } from "@/components/signed-in-home";
import { hasSessionPresenceHint, syncSessionPresenceHint } from "@/lib/session-hint";
import { authSessionQueryOptions, repoListMineQueryOptions } from "@/lib/session-queries";
import { useMounted } from "@/lib/use-mounted.tsrx";
import { cn } from "@/lib/utils";

/** D-20 home tree priority after shared root access gate. */
export type HomeTree = "setup" | "signed-in" | "marketing";

export function selectHomeTree(input: { needs_setup: boolean; hasSession: boolean }): HomeTree {
  if (input.needs_setup) return "setup";
  if (input.hasSession) return "signed-in";
  return "marketing";
}

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

function useScrollReveal(enabled: boolean, epoch: unknown) {
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
    // epoch re-runs the query when the rendered tree shape settles (pending →
    // marketing keeps the same .oct-reveal nodes in place, so the re-run is a
    // cheap re-observe; it is still the safety net if a future change does
    // replace the nodes, e.g. the marketing → signed-in swap).
  }, [enabled, epoch]);
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

  const tree: HomeTree | null = meQ.isPending ? null : signedIn ? "signed-in" : "marketing";
  const user = meQ.data ?? null;

  useEffect(() => {
    if (tree != null) syncSessionPresenceHint(tree === "signed-in");
  }, [tree]);

  // Cookie is only readable after mount — reading it for reveal gating feeds an
  // effect arg, not SSR markup, so it's safe client-side.
  const sessionHint = typeof document !== "undefined" ? hasSessionPresenceHint() : false;
  // Prerender always emits the pending pair; a warm auth.me cache (persisted
  // chrome resolves it before this island hydrates) would otherwise paint the
  // resolved tree over that DOM and roll the island back. renderedTree pins the
  // SSR branch until mounted and feeds the reveal effect's epoch so its
  // observer rebinds when the tree settles.
  const mounted = useMounted();
  const renderedTree = mounted ? tree : null;
  useScrollReveal(
    renderedTree === "marketing" || (renderedTree == null && !sessionHint),
    renderedTree,
  );

  // Pending: SSR cannot see the session, so emit both candidates — the
  // `data-oxidean-session` stamp on <html> (set per-request by the web tier)
  // selects via CSS which one paints before `auth.me` resolves.
  //
  // The anonymous branch always renders the SAME fragment shape (landing
  // first, skeleton second) in both states — only the skeleton slot appears /
  // drops. A fragment→single-node root change makes Octane replace the whole
  // root and remount the hero (verified in Chromium); keeping the fragment
  // shape lets the landing subtree stay in place across the settle (issue
  // #143).
  if (renderedTree == null || renderedTree === "marketing") {
    return (
      <>
        <div data-anon-landing>
          <AnonLanding />
        </div>
        {renderedTree == null ? (
          <div data-home-skeleton>
            <SignedInHomeSkeleton />
          </div>
        ) : null}
      </>
    );
  }

  if (renderedTree === "signed-in" && user) {
    // `repo.listMine` only starts once `auth.me` resolves signed-in
    // (`enabled: signedIn`), so pending here means "loading" — keep the
    // skeleton up instead of flashing the empty state (issue #120).
    if (reposQ.isPending) {
      return <SignedInHomeSkeleton />;
    }
    return (
      <SignedInHome
        user={user}
        repos={reposQ.data ?? []}
        reposError={reposQ.isError ? (reposQ.error?.message ?? NETWORK_ERROR) : null}
      />
    );
  }

  // (Signed-in tree above replaces the fragment wholesale — expected churn.)

  // Wrapper retained (pending branch renders the same shape) so Octane keeps
  // the landing subtree in place — no hero remount or `oct-rise` restart on
  // the auth.me settle (issue #143).
  return (
    <div data-anon-landing>
      <AnonLanding />
    </div>
  );
}

/**
 * Zero-prop on purpose: `memo` bails out of every `HomeContent` re-render
 * (auth.me settle, chrome query settles) so the hero DOM never churns. The
 * "Get started" CTA is always rendered and gated in CSS by the
 * `data-oxidean-signup` stamp on <html> (web tier + client stamp sync), so
 * signup open/closed changes mutate <html> — not this subtree.
 */
function AnonLandingImpl() @{
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
            <AppLink href="/signup" data-anon-signup-cta className={cn(buttonVariants())}>
              Get started
            </AppLink>
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
        <div class="mt-8 flex flex-wrap gap-3">
          <AppLink href="/signup" data-anon-signup-cta className={cn(buttonVariants())}>
            Get started
          </AppLink>
          <AppLink href="/explore" className={cn(buttonVariants({ variant: "secondary" }))}>
            Explore
          </AppLink>
        </div>
      </div>
    </section>
  </>;
}

export const AnonLanding = memo(AnonLandingImpl);
