<?php

declare(strict_types=1);

use Bootstrap\App;

require dirname(__DIR__) . '/vendor/autoload.php';

$app = App::boot();
echo $app->handle($_SERVER['REQUEST_METHOD'] ?? 'GET', parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/');
