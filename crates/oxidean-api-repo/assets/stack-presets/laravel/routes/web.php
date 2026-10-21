<?php

declare(strict_types=1);

return [
    'GET' => [
        '/' => static function (): string {
            return (string) file_get_contents(dirname(__DIR__) . '/resources/views/welcome.php');
        },
        '/up' => static function (): string {
            header('Content-Type: application/json');
            return json_encode(['status' => 'ok'], JSON_THROW_ON_ERROR);
        },
    ],
];
