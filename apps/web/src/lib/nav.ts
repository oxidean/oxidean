/**
 * Programmatic navigation — post-router replacement for `useNavigate`.
 *
 * Rides Astro's ClientRouter (view transitions) by dispatching through a
 * real anchor click the router's delegated listener intercepts. Falls back
 * to a full load if the router is absent.
 */
export function navigate(to: string) {
  if (typeof document === "undefined") return;
  const a = document.createElement("a");
  a.href = to;
  a.style.display = "none";
  a.setAttribute("data-oxidean-nav", "");
  document.body.appendChild(a);
  try {
    a.click();
  } finally {
    a.remove();
  }
}
