# Node.js + TypeScript

tsx watch + `tsc` build (common Node TS starter).

## Getting started

```bash
npm install
npm run dev
```

## Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | `tsx watch` |
| `npm run build` | Emit to `dist/` |
| `npm start` | Run compiled JS |
| `npm run typecheck` | `tsc --noEmit` |

## Docker

```bash
docker build -t app .
docker run --rm -p 3000:3000 app
```

Open http://localhost:3000
