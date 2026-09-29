import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import type { MemberBasePermission } from "@oxidean/api-client";
import { Check, ChevronDown } from "@octanejs/lucide";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectList,
  SelectPopup,
  SelectPortal,
  SelectPositioner,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { apiClient } from "@/lib/api-client";
import { toastError, toastSuccess } from "@/lib/toast";
import { cn } from "@/lib/utils";
import type { OrgSettingsLoaderData } from "./$owner.settings";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need organization admin access to manage settings.";
const MEMBER_BASE_HELPER =
  "Applies to Members on private organization repositories. Owners and Admins always have admin access. Public repositories remain readable.";

const MEMBER_BASE_OPTIONS: {
  value: MemberBasePermission;
  label: string;
}[] = [
  { value: "none", label: "None" },
  { value: "read", label: "Read" },
  { value: "write", label: "Write" },
];

export const Route = createFileRoute("/$owner/settings/")({
  component: OrgSettingsPage,
  head: ({ loaderData, params }) => {
    // Parent layout owns the default Settings title; keep General explicit for clarity.
    const data = loaderData as OrgSettingsLoaderData | undefined;
    const owner = String((params as { owner?: string }).owner ?? "");
    const name = data?.org?.display_name || data?.org?.slug || owner;
    return { meta: [{ title: `General · ${name} · Oxidean` }] };
  },
});

/** Exported for org settings tests (D-ORG-02b). */
export function OrgSettingsPage() @{
  const params = useParams({ from: "/$owner/settings/" }) as { owner?: string };
  const data = useLoaderData({ from: "/$owner/settings" }) as OrgSettingsLoaderData | undefined;

  const org = data?.org;
  const slug = org?.slug ?? params.owner ?? "";

  const [pending, setPending] = useState(false);

  const form = useForm({
    defaultValues: {
      display_name: org?.display_name ?? "",
      member_base: (org?.member_base_permission ?? "none") as MemberBasePermission,
    },
    onSubmit: async ({ value }) => {
      setPending(true);
      try {
        const res = await apiClient.org.updateSettings({
          slug,
          display_name: value.display_name.trim() || null,
          member_base_permission: value.member_base,
        });
        if (!res.ok) {
          if (res.error.code === "org.forbidden") {
            toastError(FORBIDDEN);
          } else {
            toastError(res.error.message || NETWORK_ERROR);
          }
          setPending(false);
          return;
        }
        form.setFieldValue("display_name", res.data.display_name ?? "");
        form.setFieldValue("member_base", res.data.member_base_permission);
        toastSuccess("Settings saved.");
        setPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  @if (!org) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else {
    <div data-testid="org-settings-general">
      <h2 className="text-[18px] font-semibold text-foreground">General</h2>
      <p className="mt-1 text-[14px] text-muted-foreground">
        Organization display name and default member repository access.
      </p>

      <form
        className="mt-6 flex flex-col gap-6"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        <form.Field name="display_name">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="org-display-name">Display name</Label>
              <Input
                id="org-display-name"
                name="display_name"
                autoComplete="organization"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              />
            </div>
          )}
        </form.Field>

        <form.Subscribe selector={(s) => s.values.member_base}>
          {(memberBase) => {
            const baseLabel =
              MEMBER_BASE_OPTIONS.find((o) => o.value === memberBase)?.label ?? "None";
            return (
              <div className="flex flex-col gap-2">
                <Label htmlFor="org-member-base">Base permission for Members</Label>
                <SelectRoot
                  value={memberBase}
                  onValueChange={(next) => {
                    if (typeof next === "string") {
                      form.setFieldValue("member_base", next as MemberBasePermission);
                    }
                  }}
                >
                  <SelectTrigger
                    id="org-member-base"
                    className="w-full min-w-[12rem]"
                    aria-label="Base permission for Members"
                  >
                    <SelectValue>{baseLabel as string}</SelectValue>
                    <SelectIcon>
                      <ChevronDown aria-hidden size={16} />
                    </SelectIcon>
                  </SelectTrigger>
                  {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
                      escape hatch; popup is anchor-scoped under the Select root. */}
                  <SelectPortal>
                    <SelectPositioner sideOffset={4}>
                      <SelectPopup>
                        <SelectList>
                          {/* octane-dom-race-ok: static Select options list */}
                          @for (const opt of MEMBER_BASE_OPTIONS; key opt.value) {
                            <SelectItem value={opt.value}>
                              <SelectItemIndicator>
                                <Check aria-hidden size={16} className="text-primary" />
                              </SelectItemIndicator>
                              <SelectItemText>{opt.label as string}</SelectItemText>
                            </SelectItem>;
                          }
                        </SelectList>
                      </SelectPopup>
                    </SelectPositioner>
                  </SelectPortal>
                </SelectRoot>
                <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                  {MEMBER_BASE_HELPER}
                </p>
              </div>
            );
          }}
        </form.Subscribe>

        <Button type="submit" disabled={pending} className={cn("h-11")}>
          @if (pending) {
            <>Saving…</>;
          } @else {
            <>Save settings</>;
          }
        </Button>
      </form>
    </div>;
  }
}
