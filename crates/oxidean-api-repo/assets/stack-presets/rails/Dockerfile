# syntax=docker/dockerfile:1

# Debian (not alpine): sqlite3/puma native extensions build against the full
# toolchain the default ruby image already ships.
FROM ruby:3.4
WORKDIR /app
COPY Gemfile ./
RUN bundle install
COPY . .
EXPOSE 3000
CMD ["bundle", "exec", "rails", "server", "-b", "0.0.0.0"]
