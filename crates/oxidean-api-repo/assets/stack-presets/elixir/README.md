# Elixir

Mix application (`mix new --sup` shape).

## Getting started

```bash
mix deps.get
mix run --no-halt
mix test
```

## Docker

```bash
docker build -t app .
docker run --rm -p 4000:4000 app
```

Open http://localhost:4000
