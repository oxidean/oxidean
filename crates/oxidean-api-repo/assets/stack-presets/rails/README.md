# Rails

Trimmed Rails 8-style app skeleton (Gemfile, config, controllers, views, SQLite).

## Getting started

```bash
bundle install
bundle exec rails db:prepare
bundle exec rails server
```

Open http://localhost:3000

This pack is intentionally lean (no full `bin/` stubs or Propshaft assets) so it stays
small enough to embed, while matching a real Rails layout.

## Docker

```bash
docker build -t app .
docker run --rm -p 3000:3000 app
```

Open http://localhost:3000
