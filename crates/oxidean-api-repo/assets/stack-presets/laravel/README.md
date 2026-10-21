# Laravel-style PHP

Lean Laravel-shaped tree: `composer.json`, `artisan`, `public/index.php`, routes, views, and bootstrap.

This pack stays embeddable (no full Illuminate `vendor/` tree). For the complete framework:

```bash
composer create-project laravel/laravel .
```

## Getting started

```bash
composer install
composer serve
# or
php -S localhost:8000 -t public
```

Open http://localhost:8000

Copy `.env.example` to `.env` when you expand toward a full Laravel install.
