<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\RouterBootstrap;
use Multifrost\Protocol;

/**
 * Tests for router bootstrap behavior against a real router.
 *
 * Requires a working router binary. If the binary is not available
 * or cannot start, all tests skip gracefully.
 */
final class BootstrapIntegrationTest extends TestCase
{
    private static ?int $routerPort = null;
    /** @var resource|string|null */
    private static $routerProcess = null;

    public static function setUpBeforeClass(): void
    {
        // Check if a router is already running externally
        $externalPort = \getenv('MULTIFROST_ROUTER_PORT');
        if ($externalPort !== false && $externalPort !== '') {
            $port = (int) $externalPort;
            if ($port > 0 && $port <= 65535) {
                $endpoint = RouterBootstrap::routerEndpoint($port);
                if (RouterBootstrap::routerReachable($endpoint)) {
                    self::$routerPort = $port;
                    self::$routerProcess = 'external';
                    return;
                }
            }
        }

        $bin = self::findRouterBinary();
        if ($bin === null) {
            return;
        }

        self::$routerPort = \random_int(31000, 60000);

        // Start the router on a random port
        $logPath = \sys_get_temp_dir() . '/multifrost-bootstrap-test.log';
        $logFile = @\fopen($logPath, 'a');

        $descriptorSpec = [
            0 => ['pipe', 'r'],
            1 => $logFile ?: ['pipe', 'w'],
            2 => $logFile ?: ['pipe', 'w'],
        ];

        $env = $_ENV;
        $env['MULTIFROST_ROUTER_PORT'] = (string) self::$routerPort;

        self::$routerProcess = @\proc_open($bin, $descriptorSpec, $pipes, null, $env);

        if (\is_resource($pipes[0] ?? null)) {
            \fclose($pipes[0]);
        }
        if ($logFile === false) {
            if (isset($pipes[1])) {
                \fclose($pipes[1]);
            }
            if (isset($pipes[2])) {
                \fclose($pipes[2]);
            }
        }

        if (!\is_resource(self::$routerProcess)) {
            self::$routerProcess = null;
            return;
        }

        // Wait for router readiness
        $endpoint = RouterBootstrap::routerEndpoint(self::$routerPort);
        $deadline = \microtime(true) + 10.0;
        $ready = false;

        while (\microtime(true) < $deadline) {
            if (RouterBootstrap::routerReachable($endpoint)) {
                $ready = true;
                break;
            }
            \usleep(200_000);
        }

        if (!$ready) {
            @\proc_terminate(self::$routerProcess);
            @\proc_close(self::$routerProcess);
            self::$routerProcess = null;
        }
    }

    public static function tearDownAfterClass(): void
    {
        if (\is_resource(self::$routerProcess)) {
            @\proc_terminate(self::$routerProcess);
            @\proc_close(self::$routerProcess);
        }
        self::$routerProcess = null;
    }

    protected function setUp(): void
    {
        if (self::$routerPort === null) {
            $this->markTestSkipped('Router not available — start one externally and set MULTIFROST_ROUTER_PORT');
        }
        $endpoint = RouterBootstrap::routerEndpoint(self::$routerPort);
        if (!RouterBootstrap::routerReachable($endpoint)) {
            $this->markTestSkipped('Router not reachable');
        }
    }

    private static function findRouterBinary(): ?string
    {
        $bin = \getenv('MULTIFROST_ROUTER_BIN');
        if ($bin !== false && $bin !== '' && \file_exists($bin)) {
            return $bin;
        }

        $root = \realpath(__DIR__ . '/../..');
        if ($root === false) {
            return null;
        }

        if (\DIRECTORY_SEPARATOR === '\\') {
            foreach (['debug', 'release'] as $profile) {
                $candidate = "{$root}\\router\\target\\{$profile}\\multifrost-router.exe";
                if (\file_exists($candidate)) {
                    return $candidate;
                }
            }
        } else {
            foreach (['debug', 'release'] as $profile) {
                $candidate = "{$root}/router/target/{$profile}/multifrost-router";
                if (\file_exists($candidate)) {
                    return $candidate;
                }
            }
        }

        return null;
    }

    public function testEnsureRouterWithAlreadyRunningRouter(): void
    {
        // The router is already running from setUpBeforeClass.
        // ensureRouter should detect it and return quickly.
        $endpoint = RouterBootstrap::ensureRouter(self::$routerPort);
        $this->assertStringEndsWith(':' . self::$routerPort, $endpoint);
    }

    public function testRouterReachableReturnsTrueForRunningRouter(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint(self::$routerPort);
        $this->assertTrue(RouterBootstrap::routerReachable($endpoint));
    }

    public function testRouterPortFromEnvMatches(): void
    {
        // The router is running on self::$routerPort.
        // RouterBootstrap::routerPortFromEnv() returns the env value.
        // But we need to set the env for it to pick up.
        $original = \getenv(Protocol::ROUTER_PORT_ENV) ?: '';
        try {
            \putenv(Protocol::ROUTER_PORT_ENV . '=' . self::$routerPort);
            $port = RouterBootstrap::routerPortFromEnv();
            $this->assertSame(self::$routerPort, $port);
        } finally {
            \putenv(Protocol::ROUTER_PORT_ENV . '=' . $original);
        }
    }
}
