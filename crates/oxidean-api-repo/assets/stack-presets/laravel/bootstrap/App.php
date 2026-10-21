<?php

declare(strict_types=1);

namespace Bootstrap;

use App\Http\Kernel;

final class App
{
    public static function boot(): Kernel
    {
        $routes = require dirname(__DIR__) . '/routes/web.php';
        return new Kernel($routes);
    }
}
