# Node + TypeScript (Watt)

Watt + `@platformatic/node` TypeScript application — Platformatic's Node capability template.

Watt project (Platformatic) — `npm run dev` runs `wattpm dev`.

## Getting started

```bash
cp .env.sample .env
npm install
npm run dev
```

Open http://localhost:3042

## Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | `wattpm dev` (watch) |
| `npm run build` | `wattpm build` |
| `npm start` | `wattpm start` (production) |


## Docker

```bash
docker build -t app .
docker run --rm -p 3042:3042 app
```

Open http://localhost:3042
