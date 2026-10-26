# syntax=docker/dockerfile:1

FROM ruby:3.4-alpine
WORKDIR /app
COPY Gemfile ./
RUN bundle install
COPY . .
ENV PORT=4567
EXPOSE 4567
CMD ["bundle", "exec", "ruby", "bin/server"]
