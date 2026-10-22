const port = Number(Deno.env.get("PORT") ?? 8000);

Deno.serve({ port }, (req) => {
  const url = new URL(req.url);
  if (url.pathname === "/") {
    return new Response("Hello from Deno!\n", {
      headers: { "Content-Type": "text/plain; charset=utf-8" },
    });
  }
  if (url.pathname === "/json") {
    return Response.json({ runtime: "deno", ok: true });
  }
  return new Response("Not Found", { status: 404 });
});

console.log(`Listening on http://localhost:${port}`);
