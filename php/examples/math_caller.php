<?php

declare(strict_types=1);

/**
 * Multifrost PHP caller example.
 * Calls a math service and prints the result.
 *
 * Usage:
 *   # Start the router first, then a math service, then:
 *   php examples/math_caller.php
 */

require __DIR__ . '/../vendor/autoload.php';

use function Multifrost\connect;

$serviceId = $argv[1] ?? 'math-service';

$connection = connect($serviceId);
$handle = $connection->handle();

echo "Connecting to service '{$serviceId}'...\n";

try {
    $handle->start();

    // Ergonomic calling via __call magic
    $result = $handle->add(1, 2);
    echo "add(1, 2) = {$result}\n";

    $result = $handle->multiply(3, 4);
    echo "multiply(3, 4) = {$result}\n";

    // Or explicit call()
    $result = $handle->call('echo', ['Hello from PHP!']);
    echo 'echo(...) = ' . \json_encode($result) . "\n";

} catch (\Multifrost\MultifrostException $e) {
    echo "Error: {$e->getMessage()}\n";
    exit(1);
} finally {
    $handle->stop();
    echo "Disconnected.\n";
}
