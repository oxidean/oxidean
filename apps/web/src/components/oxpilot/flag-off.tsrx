import { Bot } from "@octanejs/lucide";
import { EmptyState } from "@/components/ui/empty-state";

/** `oxpilot.status.reason` → human-facing cause (M7 copy map). */
function flagOffReason(reason: string | null | undefined): string {
  switch (reason) {
    case "flag_off":
      return "the OXIDEAN_OXPILOT_ENABLED flag is off";
    case "misconfigured":
      return "the provider configuration is incomplete";
    case "no_provider":
      return "no provider is configured";
    default:
      return "the feature is not enabled";
  }
}

/**
 * M7 disabled surface — names the reason + the envs an admin must set.
 * Renders ZERO interactive controls besides the static documentation
 * link (SEC-9/AC5.1.1): the swap is total, not a banner over live
 * controls — every gated proc would still deny server-side.
 */
export function OxpilotFlagOff(props: { reason: string | null | undefined }) @{
  <div className="mt-6">
    <EmptyState
      icon={<Bot size={22} />}
      title="Oxpilot is not enabled on this instance"
      description={
        `This instance reports ${flagOffReason(props.reason)}. An administrator can enable it by ` +
        `setting OXIDEAN_OXPILOT_ENABLED=1 and configuring a provider with ` +
        `OXIDEAN_OXPILOT_PROVIDER (for example \`stub\` for a local deterministic provider). ` +
        `See the configuration guide below for the full OXIDEAN_OXPILOT_* surface.`
      }
      action={
        // raw-anchor-ok — external canonical docs URL (instance hosts no docs route).
        <a
          href="https://github.com/oxidean/oxidean/blob/main/docs/CONFIGURATION.md"
          target="_blank"
          rel="noreferrer"
          className="text-[13px] font-medium text-foreground underline underline-offset-2"
        >
          docs/CONFIGURATION.md
        </a>
      }
    />
  </div>;
}
