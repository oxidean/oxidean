/**
 * Navigation chrome for ClientRouter swaps — an inline `<head>` boot script
 * (same channel as EARLY_NAV_BOOT_SCRIPT), so it is live before any island
 * hydrates and survives every swap (it only touches `documentElement`).
 *
 * Two jobs:
 *
 * 1. Top progress bar — `astro:before-preparation` arms a delayed class on
 *    `<html>`; `astro:page-load` completes it. The delay (~150ms) keeps
 *    prefetched/instant swaps flicker-free; a slow fetch gets a trickle bar
 *    so the click doesn't read as dead. The bar is `html::before` — it lives
 *    outside the swapped `<body>`, so mid-swap morphs can't drop it.
 *
 * 2. Focus restoration — after a swap, if the previously focused element was
 *    morphed away (`activeElement` falls back to body), move focus to the
 *    `<main>` landmark so keyboard users resume at page content instead of
 *    re-tabbing through the persisted chrome. Programmatic focus doesn't
 *    trigger `:focus-visible`, so mouse users see nothing.
 */

/** Show the bar only when preparation+fetch exceeds this — instant swaps stay clean. */
const SHOW_DELAY_MS = 150;
/** Time the completed 100% bar lingers before fading out. */
const DONE_LINGER_MS = 300;

export const NAV_PROGRESS_BOOT_SCRIPT = `(function(){var SHOW_DELAY=${SHOW_DELAY_MS},DONE_LINGER=${DONE_LINGER_MS};var doc=document.documentElement,timer=0,doneTimer=0;function start(){clearTimeout(timer);timer=setTimeout(function(){doc.classList.remove("nav-done");doc.classList.add("nav-pending");},SHOW_DELAY);}function stop(){clearTimeout(timer);clearTimeout(doneTimer);if(doc.classList.contains("nav-pending")){doc.classList.remove("nav-pending");doc.classList.add("nav-done");}doneTimer=setTimeout(function(){doc.classList.remove("nav-done");},DONE_LINGER);}document.addEventListener("astro:before-preparation",start);document.addEventListener("astro:page-load",stop);document.addEventListener("astro:after-swap",function(){var ae=document.activeElement;if(ae&&ae!==document.body&&ae!==document.documentElement)return;var main=document.querySelector("main");if(main instanceof HTMLElement){if(!main.hasAttribute("tabindex"))main.setAttribute("tabindex","-1");main.focus({preventScroll:true});}});})();`;
