import { Check, ChevronDown, Code, Copy } from "@octanejs/lucide";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PatHowTo } from "@/components/repo/pat-how-to";
import { SshHowTo } from "@/components/repo/ssh-how-to";
import {
  httpsCloneUrl,
  resolveSshAdvertiseHost,
  resolveSshAdvertisePort,
  sshCloneUrl,
  sshNeedsPortHint,
} from "@/lib/public-origin";
import { useRepoStore } from "@/lib/repo-store";

type CloneBoxProps = {
  owner: string;
  repo: string;
  /** Current treeish used for archive download links. */
  refName: string;
  /** When true, ZIP / tar.gz items are disabled (GIT-07 empty edge). */
  empty: boolean;
  /** Optional override for tests; otherwise SSR layout store origin. */
  publicOrigin?: string;
  /** Optional advertised SSH host override (tests / loader). */
  sshHost?: string;
  /** Optional advertised SSH port override (tests / loader). */
  sshPort?: number;
};

/** Code clone / download dropdown (D-22 / D-29 / E12 / D-SSH-02). */
export function CloneBox({
  owner,
  repo,
  refName,
  empty,
  publicOrigin: publicOriginProp,
  sshHost: sshHostProp,
  sshPort: sshPortProp,
}: CloneBoxProps) @{
  const storeOrigin = useRepoStore((s) => s.publicOrigin);
  const storeSshHost = useRepoStore((s) => s.sshHost);
  const storeSshPort = useRepoStore((s) => s.sshPort);
  const origin = publicOriginProp || storeOrigin;
  const httpsUrl = httpsCloneUrl(origin, owner, repo);
  // Meta-injected advertise values — browser code must never read
  // `process.env` (the bundle has no `process`), so the page head meta tags
  // written by `oxidean-web` are the fallback.
  const sshHost = sshHostProp ?? (storeSshHost || resolveSshAdvertiseHost(origin));
  const sshPort = sshPortProp ?? (storeSshPort > 0 ? storeSshPort : resolveSshAdvertisePort());
  const sshUrl = sshCloneUrl(sshHost, sshPort, owner, repo);
  const showPortHint = sshNeedsPortHint(sshPort);
  const zipHref = `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/archive/${encodeURIComponent(refName)}.zip`;
  const tarHref = `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/archive/${encodeURIComponent(refName)}.tar.gz`;
  const [proto, setProto] = useState<"https" | "ssh">("https");
  const [copyHttps, setCopyHttps] = useState<"idle" | "copied" | "error">("idle");
  const [copySsh, setCopySsh] = useState<"idle" | "copied" | "error">("idle");

  async function copyText(text: string, setState: (s: "idle" | "copied" | "error") => void) {
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("clipboard unavailable");
      }
      await navigator.clipboard.writeText(text);
      setState("copied");
      window.setTimeout(() => setState("idle"), 2000);
    } catch {
      setState("error");
    }
  }

  <DropdownMenu>
    <DropdownMenuTrigger
      className="inline-flex h-11 items-center gap-2 rounded-md border border-border bg-card px-4 text-[14px] font-semibold text-foreground outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
      aria-label="Clone or download"
    >
      <Code aria-hidden size={16} className="text-foreground" />
      Code
      <ChevronDown aria-hidden size={16} className="text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" className="w-[min(100vw-2rem,32rem)] p-3">
      <div
        role="tablist"
        aria-label="Clone protocol"
        className="mx-1 flex gap-4 border-b border-border"
      >
        <button
          type="button"
          role="tab"
          aria-selected={proto === "https"}
          className={
            (proto === "https"
              ? "-mb-px border-b-2 border-primary px-1 pb-2 text-[14px] font-semibold text-foreground"
              : "-mb-px border-b-2 border-transparent px-1 pb-2 text-[14px] text-muted-foreground hover:text-foreground") as string
          }
          onClick={() => setProto("https")}
        >
          HTTPS
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={proto === "ssh"}
          className={
            (proto === "ssh"
              ? "-mb-px border-b-2 border-primary px-1 pb-2 text-[14px] font-semibold text-foreground"
              : "-mb-px border-b-2 border-transparent px-1 pb-2 text-[14px] text-muted-foreground hover:text-foreground") as string
          }
          onClick={() => setProto("ssh")}
        >
          SSH
        </button>
      </div>

      @if (proto === "https") {
        <div className="px-1 py-1">
          <p className="mt-1 text-[13px] text-muted-foreground">
            Use this URL from any Git client.
          </p>
          <div className="mt-3 flex items-stretch gap-2">
            <input
              type="text"
              readOnly
              value={httpsUrl as string}
              aria-label="HTTPS clone URL"
              title={httpsUrl as string}
              onFocus={(e) => {
                const el = e.currentTarget;
                el.select();
                requestAnimationFrame(() => {
                  el.scrollLeft = 0;
                });
              }}
              className="min-w-0 flex-1 rounded-md border border-border bg-muted/40 px-2.5 py-2 font-mono text-[13px] leading-none text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-auto shrink-0 border border-border hover:bg-muted"
              aria-label="Copy HTTPS URL"
              title={(copyHttps === "copied" ? "Copied" : "Copy HTTPS URL") as string}
              onClick={() => void copyText(httpsUrl, setCopyHttps)}
            >
              <Copy
                aria-hidden
                size={16}
                className={(copyHttps === "copied" ? "hidden" : "text-foreground") as string}
              />
              <Check
                aria-hidden
                size={16}
                className={(copyHttps === "copied" ? "text-foreground" : "hidden") as string}
              />
            </Button>
          </div>
          @if (copyHttps === "error") {
            <p role="alert" className="mt-2 text-[14px] text-destructive">
              Couldn’t copy. Select the URL and copy manually.
            </p>;
          }

          <PatHowTo httpsUrl={httpsUrl as string} compact />
        </div>;
      } @else {
        <div className="px-1 py-1">
          <p className="mt-1 text-[13px] text-muted-foreground">
            Use a registered SSH public key.
          </p>
          <div className="mt-3 flex items-stretch gap-2">
            <input
              type="text"
              readOnly
              value={sshUrl as string}
              aria-label="SSH clone URL"
              title={sshUrl as string}
              onFocus={(e) => {
                const el = e.currentTarget;
                el.select();
                requestAnimationFrame(() => {
                  el.scrollLeft = 0;
                });
              }}
              className="min-w-0 flex-1 rounded-md border border-border bg-muted/40 px-2.5 py-2 font-mono text-[13px] leading-none text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-auto shrink-0 border border-border hover:bg-muted"
              aria-label="Copy SSH URL"
              title={(copySsh === "copied" ? "Copied" : "Copy SSH URL") as string}
              onClick={() => void copyText(sshUrl, setCopySsh)}
            >
              <Copy
                aria-hidden
                size={16}
                className={(copySsh === "copied" ? "hidden" : "text-foreground") as string}
              />
              <Check
                aria-hidden
                size={16}
                className={(copySsh === "copied" ? "text-foreground" : "hidden") as string}
              />
            </Button>
          </div>
          @if (copySsh === "error") {
            <p role="alert" className="mt-2 text-[14px] text-destructive">
              Couldn’t copy. Select the URL and copy manually.
            </p>;
          }
          @if (showPortHint) {
            <p className="mt-2 text-[13px] text-muted-foreground">
              {
                `Host ${sshHost} · Port ${sshPort} — add Port ${sshPort} under Host ${sshHost} in ~/.ssh/config (scp-style URL stays without ssh://).` as string
              }
            </p>;
          }
          <SshHowTo compact />
        </div>;
      }

      <DropdownMenuSeparator />

      @if (empty) {
        <>
          <DropdownMenuItem disabled aria-disabled="true">
            Download ZIP
          </DropdownMenuItem>
          <DropdownMenuItem disabled aria-disabled="true">
            Download tar.gz
          </DropdownMenuItem>
        </>;
      } @else {
        <>
          <DropdownMenuItem
            onClick={() => {
              window.location.assign(zipHref);
            }}
          >
            Download ZIP
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => {
              window.location.assign(tarHref);
            }}
          >
            Download tar.gz
          </DropdownMenuItem>
        </>;
      }
    </DropdownMenuContent>
  </DropdownMenu>;
}
