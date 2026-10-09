import { Badge } from "@/components/ui/badge";

type CommitVerifiedBadgeProps = {
  status?: string | null;
  kind?: string | null;
  className?: string;
};

/** Small Verified / Unverified badge for commit signature_status. */
export function CommitVerifiedBadge(props: CommitVerifiedBadgeProps) @{
  const status = (props.status ?? "none").toLowerCase();
  const title =
    status === "valid"
      ? props.kind === "ssh" || props.kind === "gpg"
        ? `Verified (${props.kind})`
        : "Verified"
      : status === "invalid"
        ? "Invalid signature"
        : "";

  @if (status === "valid") {
    <span data-testid="commit-verified-badge" title={title as string} className={props.className}>
      <Badge variant="success">Verified</Badge>
    </span>;
  } @else {
    @if (status === "invalid") {
      <span
        data-testid="commit-unverified-badge"
        title={title as string}
        className={props.className}
      >
        <Badge variant="destructive">Unverified</Badge>
      </span>;
    } @else {
      <></>;
    }
  }
}
