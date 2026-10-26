# Ruby

Gemfile + WEBrick hello server + Minitest.

## Getting started

```bash
bundle install
bundle exec ruby bin/server
bundle exec ruby -Ilib:test test/greeter_test.rb
```

Open http://localhost:4567

## Docker

```bash
docker build -t app .
docker run --rm -p 4567:4567 app
```

Open http://localhost:4567
