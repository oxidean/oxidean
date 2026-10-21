#!/usr/bin/env bun
/**
 * A bare `;` line in a .tsrx template is a literal semicolon text node in the
 * rendered page — Rivet treats `X;` inside @{ }/@if/@for arms as a statement
 * terminator, but the same `;` inside element children is emitted as text
 * ("Get started ; Explore" shipped on the landing page; the repo settings
 * page showed stray `;` between panels). In plain TS a bare `;` is a dead
 * empty statement — pointless either way, so there is no opt-out.
 *
 * oxfmt normalizes `<X />;` in children position onto its own line, so
 * scanning for `;`-only lines catches the whole class after formatting.
 * Comments and string/template-literal contents are excluded.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src");

type Hit = { file: string; line: number };

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (/\.tsx?$/.test(name) || name.endsWith(".tsrx")) out.push(p);
  }
  return out;
}

/** Strip comments/strings from a line (state carried across lines). */
function codeOnly(
  line: string,
  st: { block: boolean; tmpl: boolean; tmplDepth: number[] },
): string {
  let out = "";
  let quote: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i]!;
    const n = line[i + 1] ?? "";
    if (st.block) {
      if (c === "*" && n === "/") {
        st.block = false;
        i++;
      }
      continue;
    }
    if (st.tmpl) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === "`") st.tmpl = false;
      else if (c === "$" && n === "{") {
        st.tmplDepth.push(0);
        st.tmpl = false;
        i++;
      }
      continue;
    }
    if (quote) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === "/" && n === "/") break;
    if (c === "/" && n === "*") {
      st.block = true;
      i++;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      continue;
    }
    if (c === "`") {
      st.tmpl = true;
      continue;
    }
    if (st.tmplDepth.length > 0) {
      if (c === "{") st.tmplDepth[st.tmplDepth.length - 1]!++;
      else if (c === "}") {
        const d = st.tmplDepth[st.tmplDepth.length - 1]!;
        if (d === 0) {
          st.tmplDepth.pop();
          st.tmpl = true;
          continue;
        }
        st.tmplDepth[st.tmplDepth.length - 1] = d - 1;
      }
    }
    out += c;
  }
  return out;
}

const hits: Hit[] = [];
for (const file of walk(ROOT)) {
  const rel = relative(ROOT, file);
  const st = { block: false, tmpl: false, tmplDepth: [] as number[] };
  const lines = readFileSync(file, "utf-8").split("\n");
  // `;` on its own line is only rendered text when it follows a JSX tag close
  // (`</X>` or `/>`) in children position. A `;` line after a string/`)`/`}`
  // terminator is the tail of a multi-line JS statement — not a text node.
  let prevTail = "";
  for (let i = 0; i < lines.length; i++) {
    const code = codeOnly(lines[i]!, st).trim();
    if (code === ";") {
      if (prevTail === ">") hits.push({ file: rel, line: i + 1 });
      continue;
    }
    if (code !== "") prevTail = code[code.length - 1]!;
  }
}

if (hits.length === 0) {
  console.log("check-template-semicolons: ok");
  process.exit(0);
}

console.error("check-template-semicolons: bare `;` lines found:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}`);
}
console.error(
  "\nA bare `;` inside JSX children renders as a literal semicolon on the page. " +
    "Remove it — statement terminators belong inside @{ }/@if/@for arms, not element children.",
);
process.exit(1);
