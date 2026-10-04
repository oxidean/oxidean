import type { OAuthGrantPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { KeyRound } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

const SCOPE_LABELS: Record<string, string> = {
  "read:user": "Read your profile",
  "user:email": "View your email address",
  repo: "Read and write repositories over HTTPS",
  "package:read": "Download packages",
  "package:write": "Publish and delete packages",
};

type OAuthGrantsProps = {
  grants: OAuthGrantPublic[];
  onRevoked?: () => void;
};

function relShort(iso: string): string {
  return formatRelative(iso);
}

/** Authorized (granted) OAuth applications with per-grant revoke (API-03). */
export function OAuthGrants({ grants, onRevoked }: OAuthGrantsProps) @{
  const [confirming, setConfirming] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState("");

  async function revoke(grant: OAuthGrantPublic) {
    if (pending) return;
    setError("");
    setPending(grant.application_id);
    try {
      const res = await apiClient.oauthApp.revoke({ id: grant.application_id });
      if (!res.ok) {
        setError(res.error.message || "Couldn't revoke access.");
        return;
      }
      onRevoked?.();
    } catch {
      setError("Couldn't revoke access. Check your connection and try again.");
    } finally {
      setPending(null);
      setConfirming(null);
    }
  }

  @if (grants.length === 0) {
    <EmptyState
      className="mt-4"
      icon={<KeyRound aria-hidden size={22} />}
      title="No authorized applications"
      description="Third-party apps you approve from the consent screen show up here."
    />;
  } @else {
    <>
      @if (error) {
        <p role="alert" className="mt-4 text-[14px] text-destructive">
          {error as string}
        </p>;
      }
      <List className="mt-4">
        @for (const grant of grants; key grant.application_id) {
          <ListRow className="flex items-start justify-between gap-3 p-4">
            <div className="min-w-0 flex-1">
              <p
                className="truncate text-[14px] font-semibold text-foreground"
                title={grant.app_name as string}
              >
                {grant.app_name as string}
              </p>
              <div className="mt-2 flex flex-wrap gap-1.5">
                @for (const scope of grant.scopes; key scope) {
                  <Badge variant="outline">
                    {(SCOPE_LABELS[scope] ?? scope) as string}
                  </Badge>;
                }
              </div>
              <p className="mt-2 text-[13px] text-muted-foreground">
                {"Authorized " + relShort(grant.granted_at)}
                {grant.last_used_at ? " · last used " + relShort(grant.last_used_at) : ""}
              </p>
            </div>
            <div className="shrink-0">
              @if (confirming === grant.application_id) {
                <Button
                  type="button"
                  variant="destructive"
                  disabled={pending !== null}
                  onClick={() => void revoke(grant)}
                >
                  {
                    (pending === grant.application_id
                      ? "Revoking…"
                      : "Confirm revoke") as string
                  }
                </Button>;
              } @else {
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setConfirming(grant.application_id)}
                >
                  Revoke
                </Button>;
              }
            </div>
          </ListRow>;
        }
      </List>
    </>;
  }
}
