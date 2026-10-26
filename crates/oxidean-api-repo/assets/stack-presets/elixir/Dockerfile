# syntax=docker/dockerfile:1

FROM elixir:alpine
WORKDIR /app
ENV MIX_ENV=prod
COPY mix.exs ./
RUN mix deps.get
COPY . .
RUN mix compile
EXPOSE 4000
CMD ["mix", "run", "--no-halt"]
