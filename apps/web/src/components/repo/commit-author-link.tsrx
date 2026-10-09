import { AppLink } from "@/components/ui/app-link";

type CommitAuthorLinkProps = {
  name: string;
  username?: string | null;
  avatarUrl?: string | null;
  className?: string;
};

/** Link author to /{username} with optional avatar when resolved. */
export function CommitAuthorLink(props: CommitAuthorLinkProps) @{
  const name = props.name as string;
  const username = props.username?.trim() || "";
  const avatar = props.avatarUrl?.trim() || "";

  @if (username) {
    <AppLink
      href={`/${username}` as string}
      className={
        ("inline-flex max-w-full items-center gap-1.5 text-foreground no-underline hover:text-primary hover:underline " +
          (props.className ?? "")) as string
      }
      title={username as string}
    >
      @if (avatar) {
        <img
          src={avatar as string}
          alt=""
          width={16}
          height={16}
          className="h-4 w-4 shrink-0 rounded-full object-cover"
        />;
      }
      <span className="truncate font-medium">{name}</span>
    </AppLink>;
  } @else {
    <span
      className={
        ("inline-flex max-w-full items-center gap-1.5 text-muted-foreground " +
          (props.className ?? "")) as string
      }
    >
      <span className="truncate">{name}</span>
    </span>;
  }
}
