import { Outlet, createFileRoute } from "@octanejs/tanstack-router";
import { requireSessionRedirect } from "@/lib/ssr-auth";

/**
 * Layout for `/settings/tokens/*`.
 * List lives on the index route so `/new` and fine-grained can render.
 * The single beforeLoad gate covers every child route.
 */
export const Route = createFileRoute("/settings/tokens")({
  component: TokensLayout,
  head: () => ({ meta: [{ title: "Personal access tokens · Oxidean" }] }),
  beforeLoad: ({ location }) => requireSessionRedirect(location.pathname as string),
});

function TokensLayout() @{
  <Outlet />;
}
