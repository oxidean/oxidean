/** SPDX license picker options from `spdx-license-list` (D-04). */

import type { RepoTemplateOption } from "@oxidean/api-client";
import spdxLicenseList from "spdx-license-list";

/** Featured licenses shown as descriptive cards before searching the full SPDX set. */
const POPULAR: {
  id: string;
  label: string;
  description: string;
}[] = [
  {
    id: "MIT",
    label: "MIT",
    description:
      "Permissive — keep the copyright notice. Very common for open source apps and libraries.",
  },
  {
    id: "Apache-2.0",
    label: "Apache 2.0",
    description:
      "Permissive with an express patent grant. Popular for larger projects and companies.",
  },
  {
    id: "BSD-3-Clause",
    label: "BSD 3-Clause",
    description: "Permissive with a no-endorsement clause. Simple and widely understood.",
  },
  {
    id: "BSD-2-Clause",
    label: "BSD 2-Clause",
    description: "Simplified BSD — permissive with minimal conditions.",
  },
  {
    id: "GPL-3.0-only",
    label: "GPL 3.0",
    description: "Strong copyleft — derivatives must stay GPL. Common for community tools.",
  },
  {
    id: "LGPL-3.0-only",
    label: "LGPL 3.0",
    description:
      "Library copyleft — linking is freer than GPL; modifications to the library stay LGPL.",
  },
  {
    id: "AGPL-3.0-only",
    label: "AGPL 3.0",
    description:
      "Network copyleft — using the software over a network triggers source obligations.",
  },
  {
    id: "MPL-2.0",
    label: "MPL 2.0",
    description:
      "File-level copyleft — change MPL files stay open; other files can use other licenses.",
  },
  {
    id: "ISC",
    label: "ISC",
    description: "Short permissive license functionally similar to MIT/BSD.",
  },
  {
    id: "0BSD",
    label: "0BSD",
    description: "Public-domain equivalent — no attribution required.",
  },
  {
    id: "Unlicense",
    label: "Unlicense",
    description: "Public-domain dedication — waive copyright to the extent allowed.",
  },
  {
    id: "CC0-1.0",
    label: "CC0 1.0",
    description: "Creative Commons public-domain dedication — often used for data and content.",
  },
];

let cachedPicker: RepoTemplateOption[] | null = null;

/**
 * License options for the TemplatePicker modal.
 * Popular licenses carry rich descriptions; the rest are searchable under “All SPDX”.
 */
export function listLicensePickerOptions(): RepoTemplateOption[] {
  if (cachedPicker) return cachedPicker;
  const popularIds = new Set(POPULAR.map((p) => p.id));
  const popular: RepoTemplateOption[] = POPULAR.map((p) => ({
    id: p.id,
    label: p.label,
    group: "Popular",
    description: p.description,
  }));
  const rest = Object.keys(spdxLicenseList)
    .filter((id) => !popularIds.has(id))
    .sort((a, b) => a.localeCompare(b))
    .map((id) => {
      const meta = spdxLicenseList[id];
      const name = meta?.name ?? id;
      const osi = meta?.osiApproved ? "OSI-approved. " : "";
      return {
        id,
        label: id,
        group: "All SPDX",
        description: `${osi}${name}`.trim(),
      } satisfies RepoTemplateOption;
    });
  cachedPicker = [...popular, ...rest];
  return cachedPicker;
}
