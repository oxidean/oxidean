# PHP

Composer PSR-4 router + built-in server.

## Getting started

```bash
composer install
php -S localhost:8080 -t public
```

Open http://localhost:8080

## Docker

```bash
docker build -t app .
docker run --rm -p 8080:8080 app
```

Open http://localhost:8080
