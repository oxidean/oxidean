import { memo } from "octane";
import { cn } from "@/lib/utils";

function OxideanMarkImpl({
  size = 32,
  className,
}: {
  size?: number;
  className?: string;
}) @{
  <img
    src="/oxidean-mark.png"
    alt="Oxidean"
    width={size}
    height={size}
    className={cn("oxidean-squircle block shrink-0", className)}
    style={{ width: `${size}px`, height: `${size}px` }}
  />;
}
export const OxideanMark = memo(OxideanMarkImpl);
