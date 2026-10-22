<?php

declare(strict_types=1);

namespace App\Http;

final class Kernel
{
    /** @param array<string, array<string, callable(): string>> $routes */
    public function __construct(private array $routes)
    {
    }

    public function handle(string $method, string $path): string
    {
        $handler = $this->routes[$method][$path] ?? null;
        if ($handler === null) {
            http_response_code(404);
            return 'Not Found';
        }
        return $handler();
    }
}
