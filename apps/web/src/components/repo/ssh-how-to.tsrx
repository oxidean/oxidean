import { AppLink } from "@/components/ui/app-link";

type SshHowToProps = {
  /** Compact layout inside CloneBox dropdown. */
  compact?: boolean;
};

/**
 * Compact SSH key how-to (D-SSH-06 / UI-SPEC).
 * Login user is always `git`; identity is the registered key fingerprint.
 */
export function SshHowTo({ compact }: SshHowToProps) @{
  const pad = compact ? "mt-3 px-1" : "mt-0";

  <div className={pad as string} data-slot="ssh-how-to">
    <h3
      className={
        (compact
          ? "text-[14px] font-semibold text-foreground"
          : "text-[14px] font-medium text-foreground") as string
      }
    >
      Authenticate with an SSH key
    </h3>
    <ul className="mt-2 flex flex-col gap-1.5 text-[13px] leading-relaxed text-muted-foreground">
      <li>SSH login user is `git`. Identity comes from your registered key fingerprint.</li>
      <li>Register a public key, then clone with the scp-style URL above.</li>
    </ul>
    <p className="mt-3">
      <AppLink
        href="/settings/ssh-keys"
        className="text-[14px] font-semibold text-primary underline-offset-4 hover:underline"
      >
        Add an SSH key
      </AppLink>
    </p>
  </div>;
}
