import { memo } from "octane";
import { cn } from "@/lib/utils";

/**
 * Layout placeholder — Base UI does not ship Skeleton yet; this matches
 * Oxidean chrome (muted surface + pulse) and reserves space to avoid jumps.
 */
function SkeletonImpl({
  className,
  ...rest
}: {
  className?: string;
  style?: Record<string, string | number>;
  [dataAttr: `data-${string}`]: string | boolean | undefined;
}) @{
  <div
    aria-hidden
    className={cn("oct-skeleton animate-pulse rounded-md bg-muted", className)}
    {...rest}
  />;
}
export const Skeleton = memo(SkeletonImpl);
