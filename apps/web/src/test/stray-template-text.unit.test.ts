import { describe, expect, it } from "vitest";
import { findStrayTemplateText } from "./stray-template-text";

describe("findStrayTemplateText (issue #165)", () => {
  it("flags `;` between sibling elements in a layout container", () => {
    // The shipped shape: </a>; between the hero CTAs inside a flex div.
    const html = `<div class="flex gap-3"><a href="/signup">Get started</a><!--]-->;<!--[--><a href="/explore">Explore</a></div>`;
    const hits = findStrayTemplateText(html);
    expect(hits).toHaveLength(1);
    expect(hits[0]!.parent).toBe("div");
  });

  it("flags `;` as the last child of a container and at document level", () => {
    expect(findStrayTemplateText(`<div><a>x</a>;</div>`)).toHaveLength(1);
    expect(findStrayTemplateText(`<main>;;</main>`)).toHaveLength(1);
    expect(findStrayTemplateText(`<div></div>;`)).toHaveLength(1);
  });

  it("flags `;` on its own line (formatted shape)", () => {
    const html = `<div>\n  <a href="/a">x</a>\n  ;\n  <a href="/b">y</a>\n</div>`;
    expect(findStrayTemplateText(html)).toHaveLength(1);
  });

  it("ignores `;` inside text carriers, code, scripts, and comments", () => {
    expect(
      findStrayTemplateText(
        `<div><p>;</p><span>;</span><td>;</td><a href="x">;</a><h2>;</h2></div>`,
      ),
    ).toHaveLength(0);
    // Syntax-highlighted code tokens live in pre/code.
    expect(
      findStrayTemplateText(
        `<pre class="shiki"><code><span>for (;;) {</span><span>;</span></code></pre>`,
      ),
    ).toHaveLength(0);
    expect(findStrayTemplateText(`<script>if(a){b();}</script><div>ok</div>`)).toHaveLength(0);
    expect(findStrayTemplateText(`<style>.x{content:";"}</style><div>ok</div>`)).toHaveLength(0);
    expect(findStrayTemplateText(`<div><!-- ; --><a>x</a></div>`)).toHaveLength(0);
  });

  it("ignores non-stray text and entities", () => {
    expect(
      findStrayTemplateText(`<div><a>Step 1; Step 2</a> &hellip; <a>next</a></div>`),
    ).toHaveLength(0);
    // `&#59;` is the entity opt-out — it is not a `;`-only text node.
    expect(findStrayTemplateText(`<div><a>x</a>&#59;<a>y</a></div>`)).toHaveLength(0);
  });

  it("flags multiple strays and reports their parents", () => {
    const html = `<div>;<section><a>x</a>;<a>y</a></section></div>`;
    const hits = findStrayTemplateText(html);
    expect(hits.map((h) => h.parent).sort()).toEqual(["div", "section"]);
  });
});
