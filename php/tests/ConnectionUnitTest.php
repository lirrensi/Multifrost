<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\ConnectOptions;
use Multifrost\Connection;
use Multifrost\Handle;

final class ConnectionUnitTest extends TestCase
{
    // ── Connection ──────────────────────────────────────────────────

    public function testConnectionStoresTargetPeerId(): void
    {
        $connection = new Connection('math-service', new ConnectOptions());
        $this->assertSame('math-service', $connection->defaultTargetPeerId);
    }

    public function testConnectionStoresOptions(): void
    {
        $options = new ConnectOptions(peerId: 'custom-caller', routerPort: 9911);
        $connection = new Connection('svc', $options);
        $this->assertSame('custom-caller', $connection->options->peerId);
        $this->assertSame(9911, $connection->options->routerPort);
    }

    public function testConnectionHandleReturnsHandle(): void
    {
        $connection = new Connection('math-service', new ConnectOptions());
        $handle = $connection->handle();
        $this->assertInstanceOf(Handle::class, $handle);
        $this->assertSame('math-service', $handle->defaultTargetPeerId);
    }

    public function testConnectOptionsDefaultValues(): void
    {
        $options = new ConnectOptions();

        $this->assertNull($options->peerId);
        $this->assertSame(0, $options->routerPort);
        $this->assertSame(30.0, $options->requestTimeout);
        $this->assertSame(10.0, $options->bootstrapTimeout);
        $this->assertFalse($options->validateTarget);
    }

    // ── Handle ──────────────────────────────────────────────────────

    public function testHandleDefaultsToNotStarted(): void
    {
        $handle = new Handle('math-service', new ConnectOptions());
        $this->assertFalse($handle->isStarted());
    }

    public function testHandleGetPeerIdReturnsEmptyBeforeStart(): void
    {
        $handle = new Handle('math-service', new ConnectOptions());
        // Before start, peerId is empty (it's generated in start())
        $this->assertSame('', $handle->getPeerId());
    }

    public function testHandleStopBeforeStartIsSafe(): void
    {
        $handle = new Handle('math-service', new ConnectOptions());
        // Should not throw
        $handle->stop();
        $this->assertFalse($handle->isStarted());
    }

    public function testHandleCallBeforeStartThrows(): void
    {
        $this->expectException(\Multifrost\TransportError::class);
        $this->expectExceptionMessage('handle not started');

        $handle = new Handle('math-service', new ConnectOptions());
        $handle->call('add', [1, 2]);
    }

    public function testHandleMagicCallBeforeStartThrows(): void
    {
        $this->expectException(\Multifrost\TransportError::class);
        $this->expectExceptionMessage('handle not started');

        $handle = new Handle('math-service', new ConnectOptions());
        $handle->add(1, 2);
    }

    public function testHandleQueryPeerExistsBeforeStartThrows(): void
    {
        $this->expectException(\Multifrost\TransportError::class);
        $this->expectExceptionMessage('handle not started');

        $handle = new Handle('math-service', new ConnectOptions());
        $handle->queryPeerExists('some-service');
    }

    public function testHandleQueryPeerGetBeforeStartThrows(): void
    {
        $this->expectException(\Multifrost\TransportError::class);
        $this->expectExceptionMessage('handle not started');

        $handle = new Handle('math-service', new ConnectOptions());
        $handle->queryPeerGet('some-service');
    }

    public function testDoubleStopIsIdempotent(): void
    {
        $handle = new Handle('math-service', new ConnectOptions());
        // Stop twice should not throw
        $handle->stop();
        $handle->stop();
        $this->assertFalse($handle->isStarted());
    }

    public function testHandleHoldsConfiguredOptions(): void
    {
        $options = new ConnectOptions(
            peerId: 'my-caller',
            routerPort: 9981,
            requestTimeout: 15.0,
            validateTarget: true,
        );
        $handle = new Handle('math-service', $options);

        $this->assertSame('math-service', $handle->defaultTargetPeerId);
        $this->assertSame('my-caller', $handle->options->peerId);
        $this->assertSame(9981, $handle->options->routerPort);
        $this->assertSame(15.0, $handle->options->requestTimeout);
        $this->assertTrue($handle->options->validateTarget);
    }

    public function testHandleAfterStopCallStillThrows(): void
    {
        $handle = new Handle('math-service', new ConnectOptions());
        $handle->stop();  // Stop before start

        $this->expectException(\Multifrost\TransportError::class);
        $this->expectExceptionMessage('handle not started');

        $handle->call('add', [1, 2]);
    }

    // ── Connect function ────────────────────────────────────────────

    public function testConnectFunctionReturnsConnection(): void
    {
        $connection = \Multifrost\connect('test-service');
        $this->assertInstanceOf(Connection::class, $connection);
        $this->assertSame('test-service', $connection->defaultTargetPeerId);
    }

    public function testConnectFunctionWithOptions(): void
    {
        $options = new ConnectOptions(routerPort: 7777);
        $connection = \Multifrost\connect('test-service', $options);
        $this->assertSame(7777, $connection->options->routerPort);
    }

    public function testConnectFunctionDefaultsOptionsWhenNull(): void
    {
        $connection = \Multifrost\connect('test-service', null);
        $this->assertInstanceOf(ConnectOptions::class, $connection->options);
    }
}
