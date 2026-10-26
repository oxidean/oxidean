# Bun

Bun TypeScript HTTP server (`bun init` + hot reload).

## Getting started

```bash
bun install
bun run dev
```

Open http://localhost:3000

## Scripts

| Command | Description |
|---------|-------------|
| `bun run dev` | Hot-reload server |
| `bun run start` | Run without watch |
| `bun test` | Bun test runner |

## Docker

```bash
docker build -t app .
docker run --rm -p 3000:3000 app
```

Open http://localhost:3000
