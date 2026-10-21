const server = Bun.serve({
  port: Number(process.env.PORT ?? 3000),
  fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === "/") {
      return new Response("Hello from Bun!\n", {
        headers: { "Content-Type": "text/plain; charset=utf-8" },
      });
    }
    if (url.pathname === "/json") {
      return Response.json({ runtime: "bun", ok: true });
    }
    return new Response("Not Found", { status: 404 });
  },
});

console.log(`Listening on http://localhost:${server.port}`);
