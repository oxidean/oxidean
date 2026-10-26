# syntax=docker/dockerfile:1

FROM composer:2 AS deps
WORKDIR /app
COPY composer.json ./
RUN composer install --no-dev --prefer-dist --no-interaction

FROM php:cli-alpine
WORKDIR /app
COPY --from=deps /app/vendor ./vendor
COPY . .
EXPOSE 8080
CMD ["php", "-S", "0.0.0.0:8080", "-t", "public"]
