import { cn } from "@/lib/utils";
import { useEffect, useState } from "octane";

type RepoSettingsSection = {
  /** Anchor id on the wrapping <section> in the settings page. */
  id: string;
  label: string;
};

/** Section order mirrors the settings page's render order. */
const REPO_SETTINGS_SECTIONS: RepoSettingsSection[] = [
  { id: "repo-settings-general", label: "General" },
  { id: "repo-settings-visibility", label: "Visibility" },
  { id: "repo-settings-collaborators", label: "Collaborators" },
  { id: "repo-settings-branch-protection", label: "Branch protection" },
  { id: "repo-settings-deploy-keys", label: "Deploy keys" },
  { id: "repo-settings-storage", label: "Storage" },
  { id: "repo-settings-lfs", label: "Git LFS" },
  { id: "repo-settings-template", label: "Template repository" },
  { id: "repo-settings-features", label: "Features" },
  { id: "repo-settings-actions", label: "Actions" },
  { id: "repo-settings-merge", label: "Pull request merges" },
  { id: "repo-settings-webhooks", label: "Webhooks" },
  { id: "repo-settings-mirror", label: "Two-way mirror" },
  { id: "repo-settings-lfs-objects", label: "LFS objects" },
  { id: "repo-settings-danger", label: "Danger zone" },
];

function linkClass(isActive: boolean, mobile: boolean): string {
  return cn(
    mobile
      ? "inline-flex shrink-0 items-center whitespace-nowrap rounded-full border px-3 py-1 text-[13px] transition-colors"
      : "block rounded-md px-3 py-2 text-[14px] no-underline transition-colors",
    isActive
      ? mobile
        ? "border-primary/40 bg-primary/10 font-medium text-foreground"
        : "bg-muted font-semibold text-foreground"
      : mobile
        ? "border-border text-muted-foreground hover:text-foreground"
        : "font-normal text-muted-foreground hover:bg-muted/60 hover:text-foreground",
  );
}

function scrollToSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  history.replaceState(null, "", `#${id}`);
}

/**
 * In-page section nav for the repo settings monolith: sticky sidebar on
 * desktop, horizontally scrollable chips on mobile. Scroll-spy highlights the
 * section currently in view.
 */
export function RepoSettingsNav(props: { mobile?: boolean }) @{
  const [active, setActive] = useState(REPO_SETTINGS_SECTIONS[0]!.id);

  useEffect(() => {
    // happy-dom (integration tests) has no IntersectionObserver — skip
    // scroll-spy there; click-driven `active` still works without it.
    if (typeof IntersectionObserver === "undefined") return;
    const obs = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) setActive(e.target.id);
        }
      },
      // Highlight a section once its top clears the sticky chrome and until
      // the next section takes over.
      { rootMargin: "-96px 0px -70% 0px" },
    );
    for (const s of REPO_SETTINGS_SECTIONS) {
      const el = document.getElementById(s.id);
      if (el) obs.observe(el);
    }
    return () => obs.disconnect();
  }, []);

  // preventDefault: hash links would otherwise be swallowed by the view-
  // transition router instead of scrolling.
  const click = (id: string) => (e: Event) => {
    e.preventDefault();
    setActive(id);
    scrollToSection(id);
  };

  @if (props.mobile) {
    <nav
      aria-label="Repository settings sections"
      className="flex gap-2 overflow-x-auto pb-1 [scrollbar-width:thin]"
    >
      @for (const s of REPO_SETTINGS_SECTIONS; key s.id) {
        {
          /* raw-anchor-ok — same-page #section hash anchors */
        }
        <a
          href={`#${s.id}`}
          className={linkClass(active === s.id, true) as string}
          onClick={click(s.id)}
        >
          {s.label as string}
        </a>;
      }
    </nav>;
  } @else {
    <nav
      aria-label="Repository settings sections"
      className="flex max-h-[calc(100vh-9rem)] flex-col gap-1 overflow-y-auto"
    >
      <p className="mb-2 px-3 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground">
        Settings
      </p>
      @for (const s of REPO_SETTINGS_SECTIONS; key s.id) {
        {
          /* raw-anchor-ok — same-page #section hash anchors */
        }
        <a
          href={`#${s.id}`}
          className={linkClass(active === s.id, false) as string}
          aria-current={active === s.id ? "location" : undefined}
          onClick={click(s.id)}
        >
          {s.label as string}
        </a>;
      }
    </nav>;
  }
}
