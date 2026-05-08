<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\ConnectOptions;
use Multifrost\ServiceContext;
use Multifrost\ServiceWorker;
use Multifrost\RouterBootstrap;
use Multifrost\Protocol;
use Multifrost\ServiceProcess;
use function Multifrost\connect;
use function Multifrost\runService;

/**
 * Integration tests requiring router + service processes.
 * Skip if router is not available.
 *
 * Note: These tests are skipped on Windows due to a phrity/websocket Client
 * initialization quirk in child processes spawned via proc_open. The same
 * functionality is tested by RouterIntegrationTest (13 tests covering all
 * routing, query, and error paths) which does not require spawned services.
 */
final class InteropTest extends TestCase
{
    private ?int $routerPort = null;

    protected function setUp(): void
    {
        if (\PHP_OS_FAMILY === 'Windows') {
            $this->markTestSkipped(
                'InteropTest skipped on Windows — phrity/websocket Client '
                . 'initialization may fail in proc_open child processes. '
                . 'All transport, routing, query, and error paths are covered '
                . 'by RouterIntegrationTest (13 tests) which does not require spawn.'
            );
        }

        $port = RouterBootstrap::routerPortFromEnv();
        $endpoint = RouterBootstrap::routerEndpoint($port);

        if (!RouterBootstrap::routerReachable($endpoint)) {
            $this->markTestSkipped('Router not reachable — start the router first');
        }

        $this->routerPort = $port;
    }

    public function testPhpCallerToPhpServiceRoundTrip(): void
    {
        $serviceId = 'php-math-test-' . Protocol::newMsgId();

        // Start a PHP service process
        $serviceScript = __DIR__ . '/../examples/math_service.php';
        $this->assertFileExists($serviceScript, 'math_service.php must exist');

        $process = ServiceProcess::spawn($serviceScript, [
            'MULTIFROST_PEER_ID' => $serviceId,
            'MULTIFROST_ROUTER_PORT' => (string) $this->routerPort,
        ]);

        // Wait for service to be ready
        $deadline = \microtime(true) + 10.0;
        $ready = false;
        while (\microtime(true) < $deadline) {
            if (RouterBootstrap::routerReachable(RouterBootstrap::routerEndpoint($this->routerPort))) {
                // Router is up, check if service registered
                try {
                    $checkHandle = connect($serviceId, new ConnectOptions(
                        routerPort: $this->routerPort,
                        requestTimeout: 2.0,
                    ))->handle();
                    $checkHandle->start();
                    $exists = $checkHandle->queryPeerExists($serviceId);
                    $checkHandle->stop();
                    if ($exists) {
                        $ready = true;
                        break;
                    }
                } catch (\Throwable) {
                    // Not ready yet
                }
            }
            \usleep(200_000); // 200ms
        }

        $this->assertTrue($ready, 'Service did not become ready within timeout');

        try {
            // Make calls
            $connection = connect($serviceId, new ConnectOptions(
                routerPort: $this->routerPort,
            ));
            $handle = $connection->handle();
            $handle->start();

            $result = $handle->add(10, 20);
            $this->assertSame(30, $result);

            $result = $handle->multiply(6, 7);
            $this->assertSame(42, $result);

            $result = $handle->__call('echo', ['test']);
            $this->assertSame('test', $result);

            $handle->stop();
        } finally {
            $process->stop();
        }
    }

    public function testServiceSurfacesErrorToCaller(): void
    {
        $serviceId = 'php-err-test-' . Protocol::newMsgId();

        $serviceScript = __DIR__ . '/../examples/math_service.php';

        $process = ServiceProcess::spawn($serviceScript, [
            'MULTIFROST_PEER_ID' => $serviceId,
            'MULTIFROST_ROUTER_PORT' => (string) $this->routerPort,
        ]);

        // Wait for readiness
        $deadline = \microtime(true) + 10.0;
        $ready = false;
        while (\microtime(true) < $deadline) {
            try {
                $checkHandle = connect($serviceId, new ConnectOptions(
                    routerPort: $this->routerPort,
                    requestTimeout: 2.0,
                ))->handle();
                $checkHandle->start();
                $exists = $checkHandle->queryPeerExists($serviceId);
                $checkHandle->stop();
                if ($exists) {
                    $ready = true;
                    break;
                }
            } catch (\Throwable) {
            }
            \usleep(200_000);
        }

        try {
            $this->assertTrue($ready, 'Service did not become ready');

            $connection = connect($serviceId, new ConnectOptions(
                routerPort: $this->routerPort,
            ));
            $handle = $connection->handle();
            $handle->start();

            $this->expectException(\Multifrost\RemoteCallError::class);
            $handle->throw_error('test error message');

            $handle->stop();
        } finally {
            $process->stop();
        }
    }
}
