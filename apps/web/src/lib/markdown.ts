import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkGithub from "remark-github";
import remarkRehype from "remark-rehype";
import rehypeSanitize from "rehype-sanitize";
import rehypeStringify from "rehype-stringify";
import type { Element, Root } from "hast";

export type RenderGfmOptions = {
  /** Default owner for bare `#N` refs (ISS-04 / D-ISS-13). */
  owner?: string;
  /** Default repo for bare `#N` refs (ISS-04 / D-ISS-13). */
  repo?: string;
};

/** True for paths that should default to rendered GFM (blob preview / README). */
export function isMarkdownPath(path: string): boolean {
  const base = path.split("/").pop()?.toLowerCase() ?? "";
  return (
    base.endsWith(".md") ||
    base.endsWith(".markdown") ||
    base.endsWith(".mdown") ||
    base.endsWith(".mkd") ||
    base === "readme"
  );
}

/**
 * Post-sanitize pass: the GitHub sanitize schema strips `loading`/`decoding`,
 * so re-attach them to `img` after sanitizing. Values are fixed literals —
 * no user input reaches the attributes.
 */
function rehypeLazyImages() {
  return (tree: Root) => {
    const walk = (node: Root | Element) => {
      if (node.type === "element" && node.tagName === "img") {
        node.properties.loading = "lazy";
        node.properties.decoding = "async";
      }
      for (const child of node.children ?? []) walk(child as Element);
    };
    walk(tree);
  };
}

/**
 * Render GitHub-flavored Markdown to HTML with rehype-sanitize last (D-18 / D-ISS-10).
 * Never passes raw HTML through — XSS vectors from README/issue blobs are stripped.
 *
 * With owner/repo context, `remark-github` autolinks `#N` and `owner/repo#N` to
 * `/{owner}/{repo}/issues/{n}`. Mentions and commits are not linked (Q1 / D-ISS-13).
 */
export async function renderGfm(markdown: string, opts?: RenderGfmOptions): Promise<string> {
  const owner = opts?.owner?.trim() || "owner";
  const repo = opts?.repo?.trim() || "repo";

  const file = await unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkGithub, {
      repository: `${owner}/${repo}`,
      buildUrl(values) {
        if (values.type === "issue") {
          return `/${values.user}/${values.project}/issues/${values.no}`;
        }
        // Mentions / commits / compare: disabled until a later social phase (Q1).
        return false;
      },
    })
    .use(remarkRehype)
    .use(rehypeSanitize)
    .use(rehypeLazyImages)
    .use(rehypeStringify)
    .process(markdown);
  return String(file);
}
