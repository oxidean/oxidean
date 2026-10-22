import { createServer } from "node:http";
import { greet } from "./greet.js";

const port = Number(process.env.PORT ?? 3000);

const server = createServer((req, res) => {
  const name = new URL(req.url ?? "/", `http://${req.headers.host}`).searchParams.get(
    "name",
  );
  res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
  res.end(greet(name ?? "world") + "\n");
});

server.listen(port, () => {
  console.log(`Server listening on http://localhost:${port}`);
});
