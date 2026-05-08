# Multifrost PHP Agent Guide

## Build And Test

```bash
cd php
composer install
vendor/bin/phpunit
vendor/bin/phpstan analyse src tests --level=6
```

## Architecture Rules

- The public caller API is `connect`, `ConnectOptions`, `Connection`, `Handle`.
- The public service API is `spawn`, `ServiceProcess`, `ServiceWorker`, `ServiceContext`, `runService`.
- PHP is blocking/synchronous by design — no async layer, no event loop.
- `Handle` uses `__call` magic for ergonomic calling (`$handle->add(1, 2)`).
- `ServiceWorker` is only a method host; it does not own transport or process lifecycle.
- The live transport is WebSocket only via `phrity/websocket`.
- The router is bootstrapped lazily and shared across callers and services.

## Coding Rules

- Use `declare(strict_types=1)` in every file.
- Use PHP 8.2+ features: readonly classes, enums, named arguments.
- Follow PSR-12 formatting.
- Prefer explicit `throw` over `assert` for runtime validation.
- Type hint all public API methods.

## Example Patterns

```php
use Multifrost\connect;

$connection = connect('math-service');
$handle = $connection->handle();
$handle->start();
try {
    $result = $handle->add(1, 2);
} finally {
    $handle->stop();
}
```

```php
use Multifrost\ServiceContext;
use Multifrost\ServiceWorker;
use Multifrost\runService;

$worker = new class implements ServiceWorker {
    public function handleCall(string $function, array $args): mixed {
        return match ($function) {
            'add' => $args[0] + $args[1],
            default => throw new \RuntimeException("unknown: {$function}"),
        };
    }
};

runService($worker, new ServiceContext(peerId: 'math-service'));
```

## Non-Goals

- No ZeroMQ
- No async/event-loop API
- No parent/child naming
- No `ParentWorker` / `ChildWorker`
