# Multifrost PHP Quick Examples

## Caller Example

```php
<?php

require __DIR__ . '/vendor/autoload.php';

use Multifrost\connect;

$connection = connect('math-service');
$handle = $connection->handle();
$handle->start();

try {
    $result = $handle->add(10, 20);
    echo "10 + 20 = {$result}\n";

    $result = $handle->multiply(6, 7);
    echo "6 * 7 = {$result}\n";
} finally {
    $handle->stop();
}
```

## Service Example

```php
<?php

require __DIR__ . '/vendor/autoload.php';

use Multifrost\ServiceContext;
use Multifrost\ServiceWorker;
use Multifrost\runService;

$worker = new class implements ServiceWorker {
    public function handleCall(string $function, array $args): mixed
    {
        return match ($function) {
            'add' => $args[0] + $args[1],
            'multiply' => $args[0] * $args[1],
            default => throw new \RuntimeException("unknown: {$function}"),
        };
    }
};

runService($worker, new ServiceContext(peerId: 'math-service'));
```

## Spawn Example

```php
<?php

require __DIR__ . '/vendor/autoload.php';

use Multifrost\spawn;

$process = spawn(__DIR__ . '/math_service.php', [
    'MULTIFROST_PEER_ID' => 'my-service',
]);

// ... service runs independently ...

$process->stop();
```

## With Validation

```php
<?php

require __DIR__ . '/vendor/autoload.php';

use Multifrost\connect;
use Multifrost\ConnectOptions;

$connection = connect('math-service', new ConnectOptions(
    validateTarget: true, // verify service exists before allowing calls
));
$handle = $connection->handle();
$handle->start();

$result = $handle->add(1, 2);
$handle->stop();
```
