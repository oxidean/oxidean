import type { OxpilotPairingView, OxpilotTargetView } from "@oxidean/api-client";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useRef, useState } from "octane";
import { CheckCircle2 } from "@octanejs/lucide";
import { CopyCommand } from "@/components/settings/copy-command";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import {
  invalidateOxpilotQueries,
  oxpilotPairingCreateMutation,
  oxpilotTargetListQuery,
} from "@/lib/oxpilot-queries";

/** Step-2 lifecycle — the wait state machine. */
type WaitPhase = "waiting" | "success" | "timeout";

/** The pinned capability vocabulary — wire strings, not UI labels. */
const CAPS = [
  { id: "shell.exec", wire: "exec_shell", label: "Run shell commands (shell.exec)" },
  { id: "fs.read", wire: "fs_read", label: "Read files (fs.read)" },
  { id: "fs.write", wire: "fs_write", label: "Write files (fs.write)" },
] as const;

/**
 * M4 two-step connect dialog.
 *
 * Step 1 — `target_name` + capability checkboxes (`exec_shell`, `fs_read`,
 * `fs_write`; GUI is disabled with a note — no gui cap exists on the wire)
 * + the `understood` acknowledgement. `[Continue]` stays disabled until
 * name + ≥1 capability + acknowledgement, then `pairing_create` mints the
 * one-shot code.
 *
 * Step 2 — renders ONLY `ox pilot pair <code> --caps <wires>` via
 * `CopyCommand` — the CLI defaults a bare `pair` to the FULL capability
 * set, so the narrowed grant must travel on the command line too. The
 * `pairing_code` never leaves this component: no transcript, live region,
 * storage, or logs (SEC-7). `target_list` polls every 2s bounded by the
 * server `expires_at`; a matching `pending_consent` target is surfaced
 * for explicit review — identity + requested capabilities — and its
 * pending `connect` grant is answered (`grant_respond`) only after the
 * user picks Allow or Deny. `connected` ends the wait. Esc/close on
 * step 2 abandons the wait and `reset()`s the mutations — the code dies
 * with the dialog.
 */
export function ConnectDialog(props: { open: boolean; onClose: () => void }) @{
  const qc = useQueryClient();
  const [step, setStep] = useState<1 | 2>(1);
  const [name, setName] = useState("");
  const [caps, setCaps] = useState<Set<string>>(new Set());
  const [understood, setUnderstood] = useState(false);
  const [pairing, setPairing] = useState<OxpilotPairingView | null>(null);
  const [phase, setPhase] = useState<WaitPhase>("waiting");
  const [formError, setFormError] = useState<string | null>(null);
  const [consentError, setConsentError] = useState<string | null>(null);
  const [pendingTarget, setPendingTarget] = useState<OxpilotTargetView | null>(null);
  const consentShown = useRef<Set<string>>(new Set());

  const pairingM = useMutation({
    ...oxpilotPairingCreateMutation(),
    onSuccess: (view) => {
      setFormError(null);
      setConsentError(null);
      setPendingTarget(null);
      consentShown.current = new Set();
      setPairing(view);
      setPhase("waiting");
      setStep(2);
    },
    onError: (e) => {
      setFormError(e instanceof Error ? e.message : "Could not create the pairing code.");
    },
  });

  /**
   * `pending_consent` → answer the pending `connect` grant for the
   * surfaced target with the user's explicit Allow/Deny decision —
   * pairing alone never grants consent.
   */
  const consentM = useMutation({
    mutationFn: async ({ targetId, allow }: { targetId: string; allow: boolean }) => {
      const listRes = await apiClient.oxpilot.grantList({ target_id: targetId });
      if (!listRes.ok) {
        throw new Error(`${listRes.error.code}: ${listRes.error.message}`);
      }
      const grant = listRes.data.grants.find((g) => g.kind === "connect" && g.state === "pending");
      if (!grant) throw new Error("oxpilot.not_found: no pending connect grant");
      const res = await apiClient.oxpilot.grantRespond({
        grant_id: grant.id,
        allow,
      });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    onSuccess: () => {
      setConsentError(null);
      setPendingTarget(null);
      invalidateOxpilotQueries(qc);
    },
    onError: (e) => {
      setConsentError(e instanceof Error ? e.message : "Could not answer the connection request.");
    },
  });

  // Step-2 poll — `target_list` every 2s while the wait is live.
  const waiting = step === 2 && pairing !== null && phase === "waiting";
  const targetsQ = useQuery({
    ...oxpilotTargetListQuery(waiting),
    refetchInterval: waiting ? 2000 : false,
    refetchIntervalInBackground: false,
  });

  // Expiry bound — the server `expires_at` stops both the poll and the wait.
  useEffect(() => {
    if (!waiting || !pairing) return;
    const ms = Date.parse(pairing.expires_at) - Date.now();
    const t = window.setTimeout(
      () => setPhase((p) => (p === "waiting" ? "timeout" : p)),
      Math.max(ms, 0),
    );
    return () => window.clearTimeout(t);
  }, [waiting, pairing]);

  // Match the minted `pairing_id` — connected ⇒ success; pending_consent
  // ⇒ surface the target for explicit Allow/Deny review (once per id).
  useEffect(() => {
    if (!waiting || !pairing) return;
    const match = (targetsQ.data ?? []).find(
      (t: OxpilotTargetView) => t.pairing_id === pairing.pairing_id,
    );
    if (!match) return;
    if (match.lifecycle === "connected") {
      setPendingTarget(null);
      setPhase("success");
      invalidateOxpilotQueries(qc);
      return;
    }
    if (match.lifecycle === "pending_consent" && !consentShown.current.has(match.id)) {
      consentShown.current.add(match.id);
      setPendingTarget(match);
    }
  }, [targetsQ.data, waiting, pairing]);

  function toggleCap(wire: string, checked: boolean) {
    setCaps((prev) => {
      const next = new Set(prev);
      if (checked) next.add(wire);
      else next.delete(wire);
      return next;
    });
  }

  function startPairing() {
    setFormError(null);
    const trimmed = name.trim();
    if (!trimmed || caps.size === 0 || !understood) return;
    pairingM.mutate({ target_name: trimmed, caps: [...caps] });
  }

  /** Abandon the wait / close — every pairing artifact resets. */
  function close() {
    pairingM.reset();
    consentM.reset();
    setStep(1);
    setPairing(null);
    setPhase("waiting");
    setFormError(null);
    setConsentError(null);
    setPendingTarget(null);
    props.onClose();
  }

  const canContinue = name.trim().length > 0 && caps.size > 0 && understood && !pairingM.isPending;

  <Dialog
    open={props.open}
    onOpenChange={(next) => {
      if (pairingM.isPending) return;
      if (!next) close();
    }}
  >
    <DialogContent data-testid="oxpilot-connect" className="max-w-lg">
      @if (step === 1) {
        <section aria-label="Connect a system">
          <DialogHeader>
            <DialogTitle>Connect a system</DialogTitle>
          </DialogHeader>
          <div className="mt-2 flex flex-col gap-4">
            <p className="text-[13px] text-muted-foreground">
              Step 1 of 2 — name the machine and pick the capabilities its connector may use.
            </p>
            <div className="flex flex-col gap-2">
              <Label htmlFor="oxpilot-target-name">Target name</Label>
              <Input
                id="oxpilot-target-name"
                placeholder="My laptop"
                value={name}
                onInput={(e) => setName((e.target as HTMLInputElement).value)}
              />
            </div>
            <fieldset className="flex flex-col gap-2">
              <legend className="text-[14px] font-normal leading-[1.4] text-foreground">
                Capabilities
              </legend>
              @for (const cap of CAPS; key cap.wire) {
                <label className="flex cursor-pointer items-start gap-3">
                  <Checkbox
                    className="mt-0.5"
                    checked={caps.has(cap.wire)}
                    aria-label={cap.id}
                    onCheckedChange={(c) => toggleCap(cap.wire, c === true)}
                  />
                  <span className="text-[14px] text-foreground">
                    {cap.label as string}
                  </span>
                </label>;
              }
              <label className="flex cursor-not-allowed items-start gap-3 opacity-60">
                <Checkbox className="mt-0.5" checked={false} disabled aria-label="gui" />
                <span className="text-[14px] text-foreground">
                  Graphical apps (gui)
                  <span className="block text-[13px] text-muted-foreground">
                    Not available — the connector protocol has no gui capability yet.
                  </span>
                </span>
              </label>
            </fieldset>
            <label className="flex cursor-pointer items-start gap-3">
              <Checkbox
                className="mt-0.5"
                checked={understood}
                aria-label="I understand the granted capabilities"
                onCheckedChange={(c) => setUnderstood(c === true)}
              />
              <span className="text-[14px] text-foreground">
                I understand — sessions on this Oxpilot can use the capabilities I grant on
                that machine.
              </span>
            </label>
            @if (formError) {
              <AuthErrorBanner message={formError} />;
            }
            <DialogFooter>
              <Button
                type="button"
                variant="secondary"
                disabled={pairingM.isPending}
                onClick={() => close()}
              >
                Cancel
              </Button>
              <Button type="button" disabled={!canContinue} onClick={() => startPairing()}>
                {(pairingM.isPending ? "Creating…" : "Continue") as string}
              </Button>
            </DialogFooter>
          </div>
        </section>;
      } @else {
        <section aria-label="Pair the connector">
          <DialogHeader>
            <DialogTitle>Run the pair command</DialogTitle>
          </DialogHeader>
          <div className="mt-2 flex flex-col gap-4">
            <p className="text-[13px] text-muted-foreground">
              {
                `Step 2 of 2 — run this on ${name.trim() || "the target machine"} to install and pair the connector. The code is shown once and expires.` as string
              }
            </p>
            @if (pairing) {
              <CopyCommand
                command={`ox pilot pair ${pairing.pairing_code} --caps ${CAPS.filter(
                  (c) => caps.has(c.wire),
                )
                  .map((c) => c.wire)
                  .join(",")}`}
                ariaLabel="Pairing command"
              />;
            }
            @if (phase === "waiting") {
              <p className="flex items-center gap-2 text-[13px] text-muted-foreground">
                <span
                  aria-hidden
                  className="inline-block size-2 animate-pulse rounded-full bg-primary"
                />
                Waiting for the connector to check in…
              </p>;
            } @else {
              @if (phase === "success") {
                <p className="flex items-center gap-2 text-[14px] text-foreground">
                  <CheckCircle2 size={16} aria-hidden className="text-primary" />
                  {
                    `Connected — ${name.trim() || "the system"} is ready for consent-gated work.` as string
                  }
                </p>;
              } @else {
                <p className="text-[14px] text-foreground">
                  The pairing code expired before a connector checked in.
                </p>;
              }
            }
            @if (phase === "waiting" && pendingTarget) {
              <div className="flex flex-col gap-2 rounded-md border border-border p-3">
                <p className="text-[13px] font-medium text-foreground">
                  Review connection request
                </p>
                <p className="text-[13px] text-muted-foreground">
                  {`Target: ${pendingTarget.label}` as string}
                </p>
                <p className="text-[13px] text-muted-foreground">
                  {
                    `Requested capabilities: ${pendingTarget.granted_caps.join(", ")}` as string
                  }
                </p>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    disabled={consentM.isPending}
                    onClick={() =>
                      consentM.mutate({ targetId: pendingTarget.id, allow: true })
                    }
                  >
                    Allow connection
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={consentM.isPending}
                    onClick={() =>
                      consentM.mutate({ targetId: pendingTarget.id, allow: false })
                    }
                  >
                    Deny
                  </Button>
                </div>
              </div>;
            }
            @if (consentError) {
              <AuthErrorBanner message={consentError} />;
            }
            <DialogFooter>
              @if (phase === "timeout") {
                <Button
                  type="button"
                  variant="outline"
                  disabled={pairingM.isPending}
                  onClick={() => startPairing()}
                >
                  Get a new code
                </Button>;
              }
              <Button
                type="button"
                disabled={pairingM.isPending || consentM.isPending}
                onClick={() => close()}
              >
                {(phase === "success" ? "Done" : "Close") as string}
              </Button>
            </DialogFooter>
          </div>
        </section>;
      }
    </DialogContent>
  </Dialog>;
}
