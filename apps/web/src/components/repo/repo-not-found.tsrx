import { Link } from "@octanejs/tanstack-router";

/** Identical copy for missing and private/no-access (D-25). */
export function RepoNotFound() @{
  <div className="mx-auto max-w-lg px-4 py-16 text-center">
    <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
      Page not found
    </h1>
    <p className="mt-3 text-[16px] text-muted-foreground">We couldn't find that page.</p>
    <p className="mt-8">
      <Link
        to="/"
        preload="intent"
        className="text-[14px] font-semibold text-primary underline-offset-4 hover:underline"
      >
        Back to home
      </Link>
    </p>
  </div>;
}
