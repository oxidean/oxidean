<?php

declare(strict_types=1);

require dirname(__DIR__) . '/vendor/autoload.php';

use App\Http\Router;

$router = new Router();
$router->get('/', static function (): string {
    return '<!doctype html><h1>Hello from PHP!</h1><p><a href="/health">/health</a></p>';
});
$router->get('/health', static function (): string {
    header('Content-Type: application/json');
    return json_encode(['ok' => true, 'runtime' => 'php'], JSON_THROW_ON_ERROR);
});

echo $router->dispatch($_SERVER['REQUEST_METHOD'] ?? 'GET', parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/');
