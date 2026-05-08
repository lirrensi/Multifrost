<?php

declare(strict_types=1);

/**
 * Standard math worker for Multifrost E2E tests.
 * Used by e2e/v5/tests/test_matrix.py for cross-language testing.
 *
 * Usage:
 *   php e2e/workers/math_worker.php
 */

require __DIR__ . '/../../php/vendor/autoload.php';

use Multifrost\ServiceContext;
use Multifrost\ServiceWorker;
use Multifrost\runService;

$mathWorker = new class implements ServiceWorker {
    public function handleCall(string $function, array $args): mixed
    {
        return match ($function) {
            'add' => $this->toFloat($args[0] ?? 0) + $this->toFloat($args[1] ?? 0),
            'multiply' => $this->toFloat($args[0] ?? 0) * $this->toFloat($args[1] ?? 0),
            'divide' => match (true) {
                ($args[1] ?? 1) == 0 => throw new \RuntimeException('division by zero'),
                default => $this->toFloat($args[0] ?? 0) / $this->toFloat($args[1] ?? 1),
            },
            'factorial' => $this->factorial($this->toInt($args[0] ?? 0)),
            'fibonacci' => $this->fibonacci($this->toInt($args[0] ?? 0)),
            'echo' => $args[0] ?? null,
            'get_info' => [
                'language' => 'php',
                'pid' => \getmypid(),
                'version' => \PHP_VERSION,
            ],
            'throw_error' => throw new \RuntimeException((string) ($args[0] ?: 'boom')),
            'large_data' => $this->largeData($this->toInt($args[0] ?? 0)),
            default => throw new \RuntimeException("unknown function: {$function}"),
        };
    }

    private function toFloat(mixed $v): float
    {
        return \is_int($v) ? (float) $v : (float) ($v ?? 0.0);
    }

    private function toInt(mixed $v): int
    {
        return \is_int($v) ? $v : (int) ($v ?? 0);
    }

    private function factorial(int $n): int
    {
        if ($n < 0) {
            throw new \RuntimeException('factorial not defined for negative numbers');
        }
        $result = 1;
        for ($i = 2; $i <= $n; $i++) {
            $result *= $i;
        }
        return $result;
    }

    private function fibonacci(int $n): int
    {
        if ($n <= 1) return $n;
        $a = 0; $b = 1;
        for ($i = 2; $i <= $n; $i++) {
            $c = $a + $b; $a = $b; $b = $c;
        }
        return $b;
    }

    private function largeData(int $size): array
    {
        if ($size < 0) {
            throw new \RuntimeException('size must be non-negative');
        }
        $data = \range(0, $size - 1);
        return ['data' => $data, 'length' => \count($data)];
    }
};

$peerId = \getenv('MULTIFROST_PEER_ID') ?: 'math-service';

runService($mathWorker, new ServiceContext(peerId: $peerId));
