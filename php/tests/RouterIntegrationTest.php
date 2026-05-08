<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\Protocol;
use Multifrost\ConnectOptions;
use Multifrost\TransportError;
use Multifrost\RegistrationError;
use Multifrost\RouterError;
use Multifrost\PeerTransport;
use Multifrost\PeerClass;
use Multifrost\RouterBootstrap;
use function Multifrost\connect;

/**
 * Integration tests against the real Multifrost router.
 *
 * These tests require the router binary (multifrost-router) to be available.
 * The test suite starts the router on a free port, runs the tests, and cleans up.
 *
 * On systems where the router binary is not available, all tests skip gracefully.
 */
final class RouterIntegrationTest extends TestCase
{
    private const ROUTER_READY_TIMEOUT = 10.0;

    /** @var resource|null */
    private static $routerProcess = null;
    private static ?int $routerPort = null;
    private static ?string $routerBin = null;

    public static function setUpBeforeClass(): void
    {
        self::$routerBin = self::findRouterBinary();
        if (self::$routerBin === null) {
            return;
        }

        self::$routerPort = \random_int(30000, 60000);
        self::$routerProcess = self::startRouter(self::$routerBin, self::$routerPort);

        if (self::$routerProcess === null) {
            return;
        }

        // Wait for router to be ready
        $endpoint = RouterBootstrap::routerEndpoint(self::$routerPort);
        $ready = self::waitForRouter($endpoint, self::ROUTER_READY_TIMEOUT);

        if (!$ready) {
            // Router binary exists but didn't become reachable
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
            self::$routerProcess = null;
        }
    }

    protected function setUp(): void
    {
        if (self::$routerProcess === null) {
            $this->markTestSkipped(
                self::$routerBin === null
                    ? 'Router binary not found — build the router first (cargo build in router/)'
                    : 'Router binary started but did not become reachable on this system'
            );
        }

        // Safety: ensure the router is still reachable
        $endpoint = RouterBootstrap::routerEndpoint(self::$routerPort);
        if (!RouterBootstrap::routerReachable($endpoint)) {
            $this->markTestSkipped('Router became unreachable during test run');
        }
    }

    // ── Router Management ──────────────────────────────────────────

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

        // Debug build
        if (\DIRECTORY_SEPARATOR === '\\') {
            $candidate = $root . '\\router\\target\\debug\\multifrost-router.exe';
            if (\file_exists($candidate)) {
                return $candidate;
            }
        } else {
            $candidate = $root . '/router/target/debug/multifrost-router';
            if (\file_exists($candidate)) {
                return $candidate;
            }
        }

        // Release build
        if (\DIRECTORY_SEPARATOR === '\\') {
            $candidate = $root . '\\router\\target\\release\\multifrost-router.exe';
            if (\file_exists($candidate)) {
                return $candidate;
            }
        } else {
            $candidate = $root . '/router/target/release/multifrost-router';
            if (\file_exists($candidate)) {
                return $candidate;
            }
        }

        return null;
    }

    /**
     * @return resource|null
     */
    private static function startRouter(string $binary, int $port)
    {
        $logPath = \sys_get_temp_dir() . '/multifrost-router-test.log';
        $logFile = @\fopen($logPath, 'a');

        $descriptorSpec = [
            0 => ['pipe', 'r'],
            1 => $logFile ?: ['pipe', 'w'],
            2 => $logFile ?: ['pipe', 'w'],
        ];

        $env = $_ENV;
        $env['MULTIFROST_ROUTER_PORT'] = (string) $port;

        $process = @\proc_open($binary, $descriptorSpec, $pipes, null, $env);

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

        if (!\is_resource($process)) {
            if (\is_resource($logFile)) {
                \fclose($logFile);
            }
            return null;
        }

        return $process;
    }

    private static function waitForRouter(string $endpoint, float $timeout): bool
    {
        $deadline = \microtime(true) + $timeout;
        while (\microtime(true) < $deadline) {
            if (RouterBootstrap::routerReachable($endpoint)) {
                return true;
            }
            \usleep(200_000);
        }
        return false;
    }

    private function endpoint(): string
    {
        return RouterBootstrap::routerEndpoint(self::$routerPort);
    }

    private function port(): int
    {
        return (int) self::$routerPort;
    }

    // ── Test Cases ──────────────────────────────────────────────────

    public function testCallerRegistrationSucceeds(): void
    {
        $peerId = 'test-caller-' . Protocol::newMsgId();

        $transport = PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Caller);

        $this->assertFalse($transport->isClosed());
        $transport->disconnect();
        $transport->close();
    }

    public function testServiceRegistrationSucceeds(): void
    {
        $peerId = 'test-svc-' . Protocol::newMsgId();

        $transport = PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Service);

        $this->assertFalse($transport->isClosed());
        $transport->disconnect();
        $transport->close();
    }

    public function testQueryPeerExists(): void
    {
        $serviceId = 'test-qsvc-' . Protocol::newMsgId();
        $callerId = 'test-qcaller-' . Protocol::newMsgId();

        // Register a service
        $svc = PeerTransport::dial($this->endpoint(), $serviceId, PeerClass::Service);

        // Register a caller and query
        $caller = PeerTransport::dial($this->endpoint(), $callerId, PeerClass::Caller);

        $result = $caller->queryExists($serviceId);
        $this->assertTrue($result->exists);
        $this->assertSame(PeerClass::Service, $result->class);
        $this->assertTrue($result->connected);

        $caller->disconnect();
        $caller->close();
        $svc->disconnect();
        $svc->close();
    }

    public function testQueryNonExistentPeerReturnsFalse(): void
    {
        $callerId = 'test-qnonex-' . Protocol::newMsgId();

        $caller = PeerTransport::dial($this->endpoint(), $callerId, PeerClass::Caller);

        $result = $caller->queryExists('nonexistent-peer');
        $this->assertFalse($result->exists);

        $result = $caller->queryGet('nonexistent-peer');
        $this->assertFalse($result->exists);

        $caller->disconnect();
        $caller->close();
    }

    public function testDuplicatePeerIdRejected(): void
    {
        $peerId = 'test-dup-' . Protocol::newMsgId();

        $t1 = PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Caller);

        $this->expectException(RegistrationError::class);

        try {
            PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Caller);
        } finally {
            $t1->disconnect();
            $t1->close();
        }
    }

    public function testCallToNonExistentServiceReturnsRouterError(): void
    {
        $handle = connect('nonexistent-service', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        $handle->start();

        $this->expectException(RouterError::class);
        $handle->call('add', [1, 2]);

        $handle->stop();
    }

    public function testHandleQueryAfterStopThrows(): void
    {
        $handle = connect('dummy', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        $handle->stop();

        $this->expectException(TransportError::class);
        $this->expectExceptionMessage('handle not started');
        $handle->queryPeerExists('dummy');
    }

    public function testHandleStartStopStart(): void
    {
        $handle = connect('dummy', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        // Start
        $handle->start();
        $this->assertTrue($handle->isStarted());

        // Stop
        $handle->stop();
        $this->assertFalse($handle->isStarted());

        // Start again
        $handle->start();
        $this->assertTrue($handle->isStarted());

        $handle->stop();
    }

    public function testHandleCallBeforeStartThrows(): void
    {
        $handle = connect('dummy', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        $this->expectException(TransportError::class);
        $this->expectExceptionMessage('handle not started');
        $handle->call('add', [1, 2]);
    }

    public function testHandleGetPeerIdAfterStart(): void
    {
        $handle = connect('dummy', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        $this->assertSame('', $handle->getPeerId());

        $handle->start();
        $this->assertNotEmpty($handle->getPeerId());

        $handle->stop();
    }

    public function testHandleSurvivesCallErrorAndAllowsSubsequentCalls(): void
    {
        $handle = connect('dummy-service', new ConnectOptions(
            routerPort: $this->port(),
        ))->handle();

        $handle->start();

        // First call should fail (target doesn't exist)
        try {
            $handle->call('add', [1, 2]);
        } catch (RouterError) {
            // Expected
        }

        // Handle should still be usable
        try {
            $handle->call('multiply', [3, 4]);
        } catch (RouterError) {
            // Expected — still no such service
        }

        $handle->stop();
        $this->assertFalse($handle->isStarted());
    }

    public function testMultipleCallersCanRegisterConcurrently(): void
    {
        $ids = [];
        $transports = [];

        // Register several callers in sequence
        for ($i = 0; $i < 5; $i++) {
            $peerId = 'test-concurrent-' . $i . '-' . Protocol::newMsgId();
            $ids[] = $peerId;
            $transports[] = PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Caller);
        }

        // All should be registered
        foreach ($ids as $peerId) {
            $t = PeerTransport::dial($this->endpoint(), 'checker-' . Protocol::newMsgId(), PeerClass::Caller);
            $result = $t->queryExists($peerId);
            $this->assertTrue($result->exists);
            $this->assertSame(PeerClass::Caller, $result->class);
            $t->disconnect();
            $t->close();
        }

        // Cleanup
        foreach ($transports as $t) {
            $t->disconnect();
            $t->close();
        }
    }

    public function testDisconnectRemovesPeerFromRegistry(): void
    {
        $peerId = 'test-disco-' . Protocol::newMsgId();

        $transport = PeerTransport::dial($this->endpoint(), $peerId, PeerClass::Caller);

        // Verify we're registered
        $checker = PeerTransport::dial($this->endpoint(), 'checker-' . Protocol::newMsgId(), PeerClass::Caller);
        $result = $checker->queryExists($peerId);
        $this->assertTrue($result->exists);
        $checker->disconnect();
        $checker->close();

        // Disconnect
        $transport->disconnect();
        $transport->close();

        // After disconnect, the peer should be gone
        // (This may take a moment for the router to process)
        \usleep(200_000);

        $checker2 = PeerTransport::dial($this->endpoint(), 'checker2-' . Protocol::newMsgId(), PeerClass::Caller);
        $result = $checker2->queryExists($peerId);
        $this->assertFalse($result->exists);
        $checker2->disconnect();
        $checker2->close();
    }
}
