import type { RepoTreeEntry } from "@oxidean/api-client";

/** Directories (and gitlink commits) before blobs; then localeCompare (D-15). */
export function sortTreeEntries(entries: RepoTreeEntry[]): RepoTreeEntry[] {
  return [...entries].sort((a, b) => {
    const rank = (e: RepoTreeEntry) => (e.kind === "tree" || e.kind === "commit" ? 0 : 1);
    const d = rank(a) - rank(b);
    if (d !== 0) return d;
    return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  });
}

/** Parse `/tree/{ref}/…` or `/blob/{ref}/…` splat into ref + relative path (D-17 / WR-03).
 *
 * When `knownRefs` is provided (short branch/tag names), pick the longest matching
 * prefix of splat segments; otherwise fall back to first-segment split.
 */
export function parseRefAndPath(
  splat: string | undefined | null,
  knownRefs?: readonly string[] | null,
): {
  ref: string;
  path: string;
} {
  const parts = (splat ?? "")
    .split("/")
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0) return { ref: "", path: "" };

  const known = (knownRefs ?? []).map((r) => r.trim()).filter(Boolean);
  if (known.length > 0) {
    const knownSet = new Set(known);
    let best: { ref: string; pathSegs: number } | null = null;
    for (let i = parts.length; i >= 1; i--) {
      const candidate = parts.slice(0, i).join("/");
      if (knownSet.has(candidate)) {
        best = { ref: candidate, pathSegs: i };
        break; // longest first
      }
    }
    if (best) {
      return {
        ref: best.ref,
        path: parts.slice(best.pathSegs).join("/"),
      };
    }
  }

  return { ref: parts[0]!, path: parts.slice(1).join("/") };
}

/** Prefer short branch/tag names for URLs and Select (D-17). */
export function shortRefName(full: string): string {
  const s = full.trim();
  if (s.startsWith("refs/heads/")) return s.slice("refs/heads/".length);
  if (s.startsWith("refs/tags/")) return s.slice("refs/tags/".length);
  return s;
}

export function joinRepoPath(...parts: string[]): string {
  return parts
    .map((p) => p.replace(/^\/+|\/+$/g, ""))
    .filter(Boolean)
    .join("/");
}

/** Parent directory path for tree `..` navigation (empty = repo root). */
export function parentRepoPath(path: string): string {
  const parts = path
    .replace(/^\/+|\/+$/g, "")
    .split("/")
    .filter(Boolean);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join("/");
}

/** Crumb segments for tree/blob path chrome (D-17 / UI long-path backstop). */
export type PathCrumb = {
  seg: string;
  prefix: string;
  last: boolean;
};

export function pathBreadcrumbCrumbs(path: string): PathCrumb[] {
  const segments = path.split("/").filter(Boolean);
  return segments.map((seg, i) => ({
    seg,
    prefix: segments.slice(0, i + 1).join("/"),
    last: i === segments.length - 1,
  }));
}

export function treeHref(owner: string, repo: string, ref: string, path = ""): string {
  const base = `/${owner}/${repo}/tree/${encodeURIComponent(ref)}`;
  const rel = path.replace(/^\/+/, "");
  return rel ? `${base}/${rel.split("/").map(encodeURIComponent).join("/")}` : base;
}

export function blobHref(owner: string, repo: string, ref: string, path: string): string {
  const rel = path.replace(/^\/+/, "");
  return `/${owner}/${repo}/blob/${encodeURIComponent(ref)}/${rel
    .split("/")
    .map(encodeURIComponent)
    .join("/")}`;
}

function relSuffix(path: string): string {
  const rel = path.replace(/^\/+/, "");
  return rel ? `/${rel.split("/").map(encodeURIComponent).join("/")}` : "";
}

/** Web file-editing routes (GIT-19) — `/edit|new|upload|mkdir|delete/<ref>/<path>`. */
export function editHref(owner: string, repo: string, ref: string, path: string): string {
  return `/${owner}/${repo}/edit/${encodeURIComponent(ref)}${relSuffix(path)}`;
}

export function newFileHref(owner: string, repo: string, ref: string, dir = ""): string {
  return `/${owner}/${repo}/new/${encodeURIComponent(ref)}${relSuffix(dir)}`;
}

export function uploadHref(owner: string, repo: string, ref: string, dir = ""): string {
  return `/${owner}/${repo}/upload/${encodeURIComponent(ref)}${relSuffix(dir)}`;
}

export function mkdirHref(owner: string, repo: string, ref: string, dir = ""): string {
  return `/${owner}/${repo}/mkdir/${encodeURIComponent(ref)}${relSuffix(dir)}`;
}

export function deleteHref(owner: string, repo: string, ref: string, path: string): string {
  return `/${owner}/${repo}/delete/${encodeURIComponent(ref)}${relSuffix(path)}`;
}

/** Repository activity feed (`/:owner/:repo/activity`). */
export function activityHref(owner: string, repo: string): string {
  return `/${owner}/${repo}/activity`;
}

export function commitHref(owner: string, repo: string, sha: string): string {
  return `/${owner}/${repo}/commit/${encodeURIComponent(sha)}`;
}

export function compareHref(owner: string, repo: string, base: string, head: string): string {
  return `/${owner}/${repo}/compare/${encodeURIComponent(base)}...${encodeURIComponent(head)}`;
}

export function blameHref(owner: string, repo: string, ref: string, path: string): string {
  const rel = path.replace(/^\/+/, "");
  return `/${owner}/${repo}/blame/${encodeURIComponent(ref)}/${rel
    .split("/")
    .map(encodeURIComponent)
    .join("/")}`;
}

export function rawBlobUrl(owner: string, repo: string, ref: string, path: string): string {
  // Same-origin relative path — SSR-safe (no window / Host needed).
  const rel = path.replace(/^\/+/, "");
  return `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/raw/${encodeURIComponent(ref)}/${rel
    .split("/")
    .map(encodeURIComponent)
    .join("/")}`;
}

/** Human-readable byte size for blob headers. */
export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 Bytes";
  if (bytes < 1024) return `${Math.round(bytes)} Bytes`;
  if (bytes < 1024 * 1024) {
    const kb = bytes / 1024;
    const rounded = kb < 10 ? Math.round(kb * 10) / 10 : Math.round(kb);
    return `${rounded} KB`;
  }
  const mb = bytes / (1024 * 1024);
  const rounded = mb < 10 ? Math.round(mb * 10) / 10 : Math.round(mb);
  return `${rounded} MB`;
}

export function isImagePath(filePath: string): boolean {
  return /\.(png|jpe?g|gif|webp|svg|ico|bmp)$/i.test(filePath);
}

export function imageMimeForPath(filePath: string): string {
  const lower = filePath.toLowerCase();
  if (lower.endsWith(".png")) return "image/png";
  if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) return "image/jpeg";
  if (lower.endsWith(".gif")) return "image/gif";
  if (lower.endsWith(".webp")) return "image/webp";
  if (lower.endsWith(".svg")) return "image/svg+xml";
  if (lower.endsWith(".ico")) return "image/x-icon";
  if (lower.endsWith(".bmp")) return "image/bmp";
  return "application/octet-stream";
}

/** Parse `#L10` or `#L10-L20` line permalinks (D-20). */
export function parseLineHash(hash: string): { start: number; end: number } | null {
  const m = hash.match(/^#?L(\d+)(?:-L?(\d+))?$/i);
  if (!m) return null;
  const start = Number(m[1]);
  const end = m[2] ? Number(m[2]) : start;
  if (!Number.isFinite(start) || start < 1) return null;
  if (!Number.isFinite(end) || end < start) return { start, end: start };
  return { start, end };
}

export function findReadmeName(entries: RepoTreeEntry[]): string | null {
  const names = entries.filter((e) => e.kind === "blob").map((e) => e.name);
  const preferred = ["README.md", "README.MD", "Readme.md", "readme.md", "README"];
  for (const p of preferred) {
    if (names.includes(p)) return p;
  }
  return names.find((n) => /^readme(\.|$)/i.test(n)) ?? null;
}

/** Root LICENSE / COPYING blob name when present (About sidebar). */
export function findLicenseName(entries: RepoTreeEntry[]): string | null {
  const names = entries.filter((e) => e.kind === "blob").map((e) => e.name);
  const preferred = [
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "LICENCE",
    "LICENCE.md",
    "LICENCE.txt",
    "COPYING",
    "COPYING.md",
  ];
  const byLower = new Map(names.map((n) => [n.toLowerCase(), n]));
  for (const p of preferred) {
    const hit = byLower.get(p.toLowerCase());
    if (hit) return hit;
  }
  return names.find((n) => /^(license|licence|copying)(\.|$|-)/i.test(n)) ?? null;
}

/** Root CONTRIBUTING blob name when present (About sidebar). */
export function findContributingName(entries: RepoTreeEntry[]): string | null {
  const names = entries.filter((e) => e.kind === "blob").map((e) => e.name);
  const preferred = ["CONTRIBUTING.md", "CONTRIBUTING", "CONTRIBUTING.txt"];
  const byLower = new Map(names.map((n) => [n.toLowerCase(), n]));
  for (const p of preferred) {
    const hit = byLower.get(p.toLowerCase());
    if (hit) return hit;
  }
  return names.find((n) => /^contributing(\.|$)/i.test(n)) ?? null;
}

/** About-rail license label, e.g. "MIT license", from SPDX stub / first line / filename. */
export function licenseSidebarLabel(fileName: string, content?: string | null): string {
  const fromContent = content ? licenseIdFromContent(content) : null;
  if (fromContent) return formatLicenseSidebarLabel(fromContent);
  const fromName = licenseIdFromFileName(fileName);
  if (fromName) return formatLicenseSidebarLabel(fromName);
  return "License";
}

/** Detect root LICENSE / CONTRIBUTING for the About sidebar; optionally read LICENSE text for SPDX label. */
export async function resolveAboutRootFiles(
  entries: RepoTreeEntry[],
  fetchText: (path: string) => Promise<string | null>,
): Promise<{
  licenseFile: string | null;
  licenseLabel: string | null;
  contributingFile: string | null;
}> {
  const licenseFile = findLicenseName(entries);
  const contributingFile = findContributingName(entries);
  if (!licenseFile) {
    return { licenseFile: null, licenseLabel: null, contributingFile };
  }
  const content = await fetchText(licenseFile);
  return {
    licenseFile,
    licenseLabel: licenseSidebarLabel(licenseFile, content),
    contributingFile,
  };
}

function formatLicenseSidebarLabel(id: string): string {
  return `${id} license`;
}

function licenseIdFromContent(content: string): string | null {
  const spdx = content.match(/^\s*SPDX-License-Identifier:\s*([A-Za-z0-9.+-]+)/m);
  if (spdx?.[1]) return spdx[1];
  const first =
    content
      .split(/\r?\n/)
      .find((l) => l.trim())
      ?.trim() ?? "";
  if (!first || first.length > 80) return null;
  const mit = first.match(/^MIT(?:\s+License)?$/i);
  if (mit) return "MIT";
  const apache = first.match(/^Apache(?:\s+License)?(?:\s*,?\s*Version\s*([\d.]+))?$/i);
  if (apache) return apache[1] ? `Apache-${apache[1]}` : "Apache-2.0";
  const bsd3 = first.match(/^BSD\s+3-Clause(?:\s+License)?$/i);
  if (bsd3) return "BSD-3-Clause";
  const bsd2 = first.match(/^BSD\s+2-Clause(?:\s+License)?$/i);
  if (bsd2) return "BSD-2-Clause";
  const isc = first.match(/^ISC(?:\s+License)?$/i);
  if (isc) return "ISC";
  const mpl = first.match(/^Mozilla\s+Public\s+License\s+Version\s*([\d.]+)/i);
  if (mpl?.[1]) return `MPL-${mpl[1]}`;
  if (/^Unlicense$/i.test(first)) return "Unlicense";
  // Generic "Foo License" / "Foo licence" first line → keep token before License.
  const generic = first.match(/^(.+?)\s+[Ll]icen[cs]e$/);
  if (generic?.[1] && !/\s/.test(generic[1].trim()) && generic[1].length <= 32) {
    return generic[1].trim();
  }
  return null;
}

function licenseIdFromFileName(fileName: string): string | null {
  const base = fileName.replace(/^.*\//, "");
  const m = base.match(
    /(?:^|[._-])(MIT|Apache-2\.0|BSD-3-Clause|BSD-2-Clause|ISC|MPL-2\.0|GPL-3\.0(?:-only)?|LGPL-3\.0(?:-only)?|AGPL-3\.0(?:-only)?|0BSD|Unlicense|CC0-1\.0)(?:[._-]|$)/i,
  );
  return m?.[1] ?? null;
}

/** Short tip SHA for branch/tag lists (first 7 hex chars). */
export function shortOid(oid: string): string {
  const s = oid.trim();
  return s.length > 7 ? s.slice(0, 7) : s;
}

export function isBranchRef(fullName: string): boolean {
  return fullName.startsWith("refs/heads/");
}

export function isTagRef(fullName: string): boolean {
  return fullName.startsWith("refs/tags/");
}

export function archiveZipUrl(owner: string, repo: string, refName: string): string {
  return `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/archive/${encodeURIComponent(refName)}.zip`;
}

export function archiveTarGzUrl(owner: string, repo: string, refName: string): string {
  return `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/archive/${encodeURIComponent(refName)}.tar.gz`;
}
