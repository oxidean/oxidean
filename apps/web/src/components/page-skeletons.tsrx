import { Skeleton } from "@/components/ui/skeleton";
import { SETTINGS_SHELL_MAX } from "@/components/settings/settings-shell";

/** Signed-in home at `/` — mirrors {@link SignedInHome} two-column geometry. */
export function SignedInHomeSkeleton() @{
  <div
    className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:py-10"
    role="status"
    aria-busy="true"
    aria-label="Loading"
  >
    <div className="grid gap-8 lg:grid-cols-[17.5rem_minmax(0,1fr)] lg:gap-12">
      <div className="flex min-w-0 flex-col">
        <div className="mb-3 flex items-center justify-between gap-2">
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-6 w-10" />
        </div>
        <Skeleton className="mb-3 h-9 w-full" />
        <div className="flex flex-col gap-0.5">
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-3/4" />
        </div>
      </div>
      <div className="min-w-0">
        <div className="mb-5 border-b border-border pb-3">
          <Skeleton className="h-8 w-24" />
          <Skeleton className="mt-1 h-5 w-72 max-w-full" />
        </div>
        <Skeleton className="h-40 w-full rounded-lg" />
      </div>
    </div>
  </div>;
}

export function ProfilePageSkeleton() @{
  <div
    className={`mx-auto w-full px-4 py-10 sm:px-6 ${SETTINGS_SHELL_MAX}`}
    role="status"
    aria-busy="true"
    aria-label="Loading profile"
  >
    <Skeleton className="h-8 w-40" />
    <Skeleton className="mt-2 h-5 w-64" />
    <div className="mt-8 flex items-center gap-4">
      <Skeleton className="size-16 rounded-full" />
      <Skeleton className="h-11 w-36" />
    </div>
    <div className="mt-8 flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-11 w-full" />
      </div>
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-20" />
        <Skeleton className="h-11 w-full" />
      </div>
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-12" />
        <Skeleton className="h-24 w-full" />
      </div>
      <Skeleton className="h-11 w-28" />
    </div>
  </div>;
}

export function AdminAuthSkeleton() @{
  <div
    className={`mx-auto w-full px-4 py-10 sm:px-6 ${SETTINGS_SHELL_MAX}`}
    role="status"
    aria-busy="true"
    aria-label="Loading auth settings"
  >
    <Skeleton className="h-8 w-48" />
    <Skeleton className="mt-2 h-5 w-full max-w-lg" />
    <div className="mt-8 flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-11 w-full" />
      </div>
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-11 w-full" />
      </div>
      <Skeleton className="h-24 w-full" />
      <Skeleton className="h-11 w-36" />
    </div>
  </div>;
}

export function StatusPageSkeleton() @{
  <div
    className="mx-auto max-w-3xl px-4 py-16"
    role="status"
    aria-busy="true"
    aria-label="Checking status"
  >
    <Skeleton className="h-8 w-48" />
    <div className="mt-8 rounded-lg border border-border bg-card p-6">
      <Skeleton className="h-5 w-40" />
      <Skeleton className="mt-4 h-4 w-56" />
      <Skeleton className="mt-2 h-4 w-44" />
    </div>
  </div>;
}

/** Form-shaped placeholders inside {@link AuthShell}. */
export function AuthFormSkeleton({ fields = 2 }: { fields?: number }) @{
  const fieldIndexes = Array.from({ length: fields }, (_, i) => i);

  <div className="flex flex-col gap-4" role="status" aria-busy="true" aria-label="Loading form">
    @for (const i of fieldIndexes; key i) {
      <div className="flex flex-col gap-2">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-11 w-full" />
      </div>;
    }
    <Skeleton className="h-11 w-full" />
    <Skeleton className="mx-auto h-4 w-40" />
  </div>;
}
