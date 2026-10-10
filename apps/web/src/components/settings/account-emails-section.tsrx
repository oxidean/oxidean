import {
  emailAddMutationOptions,
  emailListQueryOptions,
  emailRemoveMutationOptions,
  emailResendVerifyMutationOptions,
  emailSetPrimaryMutationOptions,
  type EmailListItem,
} from "@oxidean/api-client";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { Mail } from "@octanejs/lucide";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { InputOtp } from "@/components/ui/input-otp";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { AppLink } from "@/components/ui/app-link";

const LIST_ERROR = "Couldn't load email addresses. Check your connection and try again.";
const VERIFY_INVALID = "That code is invalid or expired. Resend verification and try again.";
const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type AccountEmailsSectionProps = {
  initialEmails?: EmailListItem[];
};

/** Email addresses section on /settings/profile (Account). */
export function AccountEmailsSection(props: AccountEmailsSectionProps) @{
  const queryClient = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<EmailListItem | null>(null);
  const [formError, setFormError] = useState("");
  const [actionError, setActionError] = useState("");
  const [verifyTargetId, setVerifyTargetId] = useState<string | null>(null);
  const [verifyCode, setVerifyCode] = useState("");
  const [verifyError, setVerifyError] = useState("");
  const [verifyPending, setVerifyPending] = useState(false);

  const list = useQuery({
    ...emailListQueryOptions(apiClient),
    ...(props.initialEmails !== undefined ? { initialData: props.initialEmails } : {}),
  });

  function openVerifyPanel(id: string) {
    setVerifyTargetId(id);
    setVerifyCode("");
    setVerifyError("");
  }

  function closeVerifyPanel() {
    setVerifyTargetId(null);
    setVerifyCode("");
    setVerifyError("");
  }

  async function submitVerifyCode(code: string) {
    const trimmed = code.trim();
    if (trimmed.length !== 8 || !/^\d{8}$/.test(trimmed) || verifyPending) return;
    setVerifyPending(true);
    setVerifyError("");
    try {
      const res = await apiClient.auth.verify({ code: trimmed });
      if (!res.ok) {
        setVerifyError(
          res.error.code === "auth.invalid_token"
            ? VERIFY_INVALID
            : res.error.message || VERIFY_INVALID,
        );
        setVerifyPending(false);
        return;
      }
      closeVerifyPanel();
      await queryClient.invalidateQueries({ queryKey: ["email", "list"] });
      await queryClient.invalidateQueries({ queryKey: ["auth", "me"] });
      setVerifyPending(false);
    } catch {
      setVerifyError(NETWORK_ERROR);
      setVerifyPending(false);
    }
  }

  const form = useForm({
    defaultValues: { email: "" },
    onSubmit: ({ value }) => {
      if (add.isPending) return;
      setFormError("");
      const e = value.email.trim();
      if (!e || !e.includes("@")) {
        setFormError("Enter a valid email address.");
        return;
      }
      add.mutate({ email: e });
    },
  });

  const add = useMutation({
    ...emailAddMutationOptions(apiClient),
    onSuccess: async (item) => {
      form.reset();
      setFormError("");
      setShowAdd(false);
      await queryClient.invalidateQueries({ queryKey: ["email", "list"] });
      openVerifyPanel(item.id);
    },
    onError: (err: Error) => {
      setFormError(err.message || "Couldn't add email.");
    },
  });

  const setPrimary = useMutation({
    ...emailSetPrimaryMutationOptions(apiClient),
    onSuccess: async () => {
      setActionError("");
      await queryClient.invalidateQueries({ queryKey: ["email", "list"] });
      await queryClient.invalidateQueries({ queryKey: ["auth", "me"] });
    },
    onError: (err: Error) => {
      setActionError(err.message || "Couldn't set primary email.");
    },
  });

  const resend = useMutation({
    ...emailResendVerifyMutationOptions(apiClient),
    onSuccess: (_data, vars) => {
      setActionError("");
      openVerifyPanel(vars.id);
    },
    onError: (err: Error) => {
      setActionError(err.message || "Couldn't resend verification.");
    },
  });

  const removeMut = useMutation({
    ...emailRemoveMutationOptions(apiClient),
    onSuccess: async () => {
      setRemoveTarget(null);
      setActionError("");
      await queryClient.invalidateQueries({ queryKey: ["email", "list"] });
    },
    onError: (err: Error) => {
      setActionError(err.message || "Couldn't remove email.");
    },
  });

  const rows = list.data ?? props.initialEmails ?? [];
  const atCap = rows.length >= 10;
  const primaryEmail = rows.find((e) => e.is_primary)?.email;
  const sortedRows = [...rows].sort((a, b) => {
    if (a.is_primary === b.is_primary) return 0;
    return a.is_primary ? -1 : 1;
  });
  const verifyTarget = sortedRows.find((e) => e.id === verifyTargetId);

  <section className="mt-12" data-testid="settings-account-emails">
    <div className="flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h2 className="text-[18px] font-semibold text-foreground">Email addresses</h2>
        <p className="mt-1 text-[14px] text-muted-foreground">
          Add and verify addresses for login and commit Verified badges. Primary is used for
          notifications. Use a verified address (or forge noreply) as your git committer email.
        </p>
        @if (primaryEmail) {
          <p className="mt-1 text-[14px] text-foreground">
            <span className="text-muted-foreground">Primary email · </span>
            <span className="font-medium">{primaryEmail as string}</span>
          </p>;
        }
        <p className="mt-1 text-[14px] text-muted-foreground">
          Signing keys are managed under{" "}
          <AppLink href="/settings/ssh-keys" className="underline underline-offset-2">
            SSH and GPG keys
          </AppLink>
          .
        </p>
      </div>
      <Button
        type="button"
        disabled={atCap}
        onClick={() => {
          setShowAdd(true);
          setFormError("");
        }}
      >
        Add email address
      </Button>
    </div>
    @if (atCap) {
      <p className="mt-3 text-[14px] text-muted-foreground">Maximum of 10 email addresses</p>;
    }
    @if (actionError) {
      <p role="alert" className="mt-3 text-[14px] text-destructive">
        {actionError as string}
      </p>;
    }

    @if (showAdd && !atCap) {
      <form
        method="post"
        action="#"
        className="mt-6 flex flex-col gap-4 rounded-md border border-border bg-card p-4"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        <form.Field name="email">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="account-email-add">Email address</Label>
              <Input
                id="account-email-add"
                name="email"
                type="email"
                value={field.state.value}
                disabled={add.isPending}
                onBlur={field.handleBlur}
                onInput={(e) =>
                  field.handleChange((e.currentTarget as HTMLInputElement).value)
                }
                autoComplete="email"
              />
            </div>
          )}
        </form.Field>
        @if (formError) {
          <p role="alert" className="text-[14px] text-destructive">
            {formError as string}
          </p>;
        }
        <div className="flex flex-wrap gap-2">
          <Button type="submit" disabled={add.isPending}>
            {(add.isPending ? "Adding…" : "Add email address") as string}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={add.isPending}
            onClick={() => {
              setShowAdd(false);
              setFormError("");
              form.reset();
            }}
          >
            Cancel
          </Button>
        </div>
      </form>;
    }

    @if (verifyTarget && !verifyTarget.verified) {
      <div
        className="mt-6 flex flex-col gap-4 rounded-md border border-border bg-card p-4"
        data-testid="settings-email-verify-code"
      >
        <div className="min-w-0">
          <p className="text-[14px] font-semibold text-foreground">Enter verification code</p>
          <p className="mt-1 text-[14px] text-muted-foreground">
            We sent an 8-digit code to{" "}
            <span className="font-medium text-foreground">
              {verifyTarget.email as string}
            </span>
            . You can also open the link in that email.
          </p>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="account-email-verify-code">Verification code</Label>
          <InputOtp
            id="account-email-verify-code"
            name="verify-code"
            value={verifyCode}
            onChange={(next) => {
              setVerifyCode(next);
              if (verifyError) setVerifyError("");
            }}
            onComplete={(next) => {
              void submitVerifyCode(next);
            }}
            disabled={verifyPending}
            aria-label="Verification code"
            aria-invalid={Boolean(verifyError)}
          />
        </div>
        @if (verifyError) {
          <p role="alert" className="text-[14px] text-destructive">
            {verifyError as string}
          </p>;
        }
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            disabled={verifyPending || verifyCode.length !== 8}
            onClick={() => void submitVerifyCode(verifyCode)}
          >
            {(verifyPending ? "Verifying…" : "Verify email") as string}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={verifyPending}
            onClick={() => closeVerifyPanel()}
          >
            Cancel
          </Button>
        </div>
      </div>;
    }

    @if (list.isPending && props.initialEmails === undefined) {
      <div
        className="mt-8 flex flex-col gap-3"
        aria-busy="true"
        aria-label="Loading email addresses"
      >
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>;
    } @else {
      @if (list.isError) {
        <div className="mt-8 flex flex-col gap-3">
          <p role="alert" className="text-[16px] text-destructive">
            {LIST_ERROR}
          </p>
          <Button
            type="button"
            variant="outline"
            className="w-fit"
            onClick={() => void list.refetch()}
          >
            Retry
          </Button>
        </div>;
      } @else {
        @if (sortedRows.length === 0) {
          <EmptyState
            className="mt-8"
            icon={<Mail aria-hidden size={22} />}
            title="No email addresses"
            description="Add an email address to verify for login and signed commits."
          />;
        } @else {
          <List className="mt-8">
            @for (const item of sortedRows; key item.id) {
              <ListRow
                className={
                  (item.is_primary
                    ? "justify-between gap-3 border-l-2 border-l-primary bg-muted/30 p-4"
                    : "justify-between gap-3 p-4") as string
                }
                aria-current={item.is_primary ? "true" : undefined}
              >
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                  <p className="truncate text-[14px] font-semibold leading-none text-foreground">
                    {item.email as string}
                  </p>
                  @if (item.is_primary) {
                    <span data-testid="email-primary-badge">
                      <Badge variant="success">Primary</Badge>
                    </span>;
                  }
                  @if (item.verified) {
                    <Badge variant="outline">Verified</Badge>;
                  } @else {
                    <Badge variant="outline">Unverified</Badge>;
                  }
                </div>
                <div className="flex shrink-0 flex-wrap items-center gap-2">
                  @if (!item.verified) {
                    <>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={resend.isPending}
                        onClick={() => openVerifyPanel(item.id)}
                      >
                        Enter code
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={resend.isPending}
                        onClick={() => resend.mutate({ id: item.id })}
                      >
                        Resend verify
                      </Button>
                    </>;
                  }
                  @if (item.is_primary) {
                    <span className="text-[13px] text-muted-foreground">
                      Primary address
                    </span>;
                  } @else {
                    <>
                      @if (item.verified) {
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={setPrimary.isPending}
                          onClick={() => setPrimary.mutate({ id: item.id })}
                        >
                          Make primary
                        </Button>;
                      }
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="border-destructive/50 text-destructive hover:bg-destructive/10 hover:text-destructive"
                        onClick={() => setRemoveTarget(item)}
                      >
                        Remove
                      </Button>
                    </>;
                  }
                </div>
              </ListRow>;
            }
          </List>;
        }
      }
    }

    <AlertDialog
      open={removeTarget !== null}
      onOpenChange={(open) => {
        if (!open) setRemoveTarget(null);
      }}
    >
      <AlertDialogContent className="max-h-[min(90vh,32rem)] overflow-y-auto">
        <AlertDialogHeader>
          <AlertDialogTitle>Remove email address?</AlertDialogTitle>
          <AlertDialogDescription className="break-words whitespace-normal">
            {(removeTarget ? `Remove ${removeTarget.email} from your account?` : "") as string}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={removeMut.isPending}>Keep email</AlertDialogCancel>
          <Button
            type="button"
            variant="destructive"
            disabled={removeMut.isPending || !removeTarget}
            onClick={() => {
              if (removeTarget) removeMut.mutate({ id: removeTarget.id });
            }}
          >
            {(removeMut.isPending ? "Removing…" : "Remove email") as string}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </section>;
}
