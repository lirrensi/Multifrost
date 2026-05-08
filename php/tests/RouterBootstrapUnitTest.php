<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\RouterBootstrap;
use Multifrost\Protocol;

final class RouterBootstrapUnitTest extends TestCase
{
    private string $originalPortEnv;

    protected function setUp(): void
    {
        // Save original env value
        $this->originalPortEnv = \getenv(Protocol::ROUTER_PORT_ENV) ?: '';
    }

    protected function tearDown(): void
    {
        // Restore env
        if ($this->originalPortEnv !== '') {
            \putenv(Protocol::ROUTER_PORT_ENV . '=' . $this->originalPortEnv);
        } else {
            \putenv(Protocol::ROUTER_PORT_ENV);
        }
    }

    // ── routerPortFromEnv ──────────────────────────────────────────

    public function testPortFromEnvReturnsDefaultWhenUnset(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV);
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    public function testPortFromEnvReturnsDefaultWhenEmpty(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    public function testPortFromEnvReturnsSetValue(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=7777');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(7777, $port);
    }

    public function testPortFromEnvRejectsPortZero(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=0');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    public function testPortFromEnvRejectsNegativePort(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=-1');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    public function testPortFromEnvRejectsPortTooHigh(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=65536');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    public function testPortFromEnvAcceptsUpperBound(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=65535');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(65535, $port);
    }

    public function testPortFromEnvAcceptsLowerBound(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=1');
        $port = RouterBootstrap::routerPortFromEnv();
        $this->assertSame(1, $port);
    }

    public function testPortFromEnvRejectsNonNumericString(): void
    {
        \putenv(Protocol::ROUTER_PORT_ENV . '=abc');
        $port = RouterBootstrap::routerPortFromEnv();
        // (int) 'abc' === 0, which is rejected
        $this->assertSame(Protocol::DEFAULT_ROUTER_PORT, $port);
    }

    // ── routerEndpoint ─────────────────────────────────────────────

    public function testRouterEndpointFormat(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint(9981);
        $this->assertSame('ws://127.0.0.1:9981', $endpoint);
    }

    public function testRouterEndpointCustomPort(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint(7777);
        $this->assertSame('ws://127.0.0.1:7777', $endpoint);
    }

    public function testRouterEndpointAlwaysLocalhost(): void
    {
        for ($port = 1; $port <= 5; $port++) {
            $endpoint = RouterBootstrap::routerEndpoint($port);
            $this->assertStringStartsWith('ws://127.0.0.1:', $endpoint);
        }
    }

    // ── routerReachable (returns false when no router) ──────────────

    public function testRouterReachableReturnsFalseWhenNoRouter(): void
    {
        // Should not throw, just return false
        $reachable = RouterBootstrap::routerReachable('ws://127.0.0.1:1');
        $this->assertFalse($reachable);
    }

    public function testRouterReachableReturnsFalseForInvalidEndpoint(): void
    {
        // Should not throw, just return false for unreachable ports
        $reachable = RouterBootstrap::routerReachable('ws://127.0.0.1:65432');
        $this->assertFalse($reachable);
    }

    public function testRouterReachableWithBadScheme(): void
    {
        // Passing an invalid URI scheme should not crash
        $reachable = RouterBootstrap::routerReachable('http://127.0.0.1:9981');
        // May throw or return false depending on how phrity/websocket handles it
        // Just verify no PHP error/warning
        $this->assertFalse($reachable);
    }

    // ── isProcessAlive ─────────────────────────────────────────────

    public function testIsProcessAliveReturnsTrueForCurrentProcess(): void
    {
        $pid = \getmypid();
        $this->assertTrue(RouterBootstrap::isProcessAlive($pid));
    }

    public function testIsProcessAliveReturnsFalseForNonExistentPid(): void
    {
        // Use a PID that is extremely unlikely to exist on any system
        $this->assertFalse(RouterBootstrap::isProcessAlive(999999999));
    }

    // ── evaluateExistingLock ────────────────────────────────────────

    public function testEvaluateExistingLockValidLockReturnsWait(): void
    {
        $pid = \getmypid();
        $data = [
            'format' => 'v1',
            'pid' => $pid,
            'router_pid' => null,
            'port' => 9981,
            'created_at_unix' => \microtime(true),
            'expires_at_unix' => \microtime(true) + 3600.0, // 1 hour from now
            'status' => 'starting',
            'language' => 'php',
        ];
        $path = \tempnam(\sys_get_temp_dir(), 'mf_lock_test_');
        \file_put_contents($path, \json_encode($data, \JSON_UNESCAPED_SLASHES));

        $result = RouterBootstrap::evaluateExistingLock($path, 9981);
        $this->assertSame('wait', $result);

        @\unlink($path);
    }

    public function testEvaluateExistingLockExpiredReturnsReclaim(): void
    {
        $pid = \getmypid();
        $data = [
            'format' => 'v1',
            'pid' => $pid,
            'router_pid' => null,
            'port' => 9981,
            'created_at_unix' => \microtime(true) - 3600.0,
            'expires_at_unix' => \microtime(true) - 1800.0, // 30 minutes ago
            'status' => 'starting',
            'language' => 'php',
        ];
        $path = \tempnam(\sys_get_temp_dir(), 'mf_lock_test_');
        \file_put_contents($path, \json_encode($data, \JSON_UNESCAPED_SLASHES));

        $result = RouterBootstrap::evaluateExistingLock($path, 9981);
        $this->assertSame('reclaim', $result);

        @\unlink($path);
    }

    public function testEvaluateExistingLockGarbageContentReturnsReclaim(): void
    {
        $path = \tempnam(\sys_get_temp_dir(), 'mf_lock_test_');
        \file_put_contents($path, 'this is not valid json at all');

        $result = RouterBootstrap::evaluateExistingLock($path, 9981);
        $this->assertSame('reclaim', $result);

        @\unlink($path);
    }

    public function testEvaluateExistingLockFailedStatusReturnsReclaim(): void
    {
        $pid = \getmypid();
        $data = [
            'format' => 'v1',
            'pid' => $pid,
            'router_pid' => null,
            'port' => 9981,
            'created_at_unix' => \microtime(true),
            'expires_at_unix' => \microtime(true) + 3600.0,
            'status' => 'failed',
            'language' => 'php',
        ];
        $path = \tempnam(\sys_get_temp_dir(), 'mf_lock_test_');
        \file_put_contents($path, \json_encode($data, \JSON_UNESCAPED_SLASHES));

        $result = RouterBootstrap::evaluateExistingLock($path, 9981);
        $this->assertSame('reclaim', $result);

        @\unlink($path);
    }

    // ── ensureRouter (with real router binary) ─────────────────────
    // This is covered in BootstrapIntegrationTest.php
}
