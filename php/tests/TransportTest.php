<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\PeerTransport;
use Multifrost\PeerClass;
use Multifrost\Protocol;
use Multifrost\RouterBootstrap;
use Multifrost\TransportError;
use Multifrost\RegistrationError;
use Multifrost\RouterError;
use Multifrost\RemoteCallError;

/**
 * Transport tests require a running router.
 * Skip if router is not available.
 */
final class TransportTest extends TestCase
{
    private ?int $routerPort = null;

    protected function setUp(): void
    {
        $port = RouterBootstrap::routerPortFromEnv();
        $endpoint = RouterBootstrap::routerEndpoint($port);

        if (!RouterBootstrap::routerReachable($endpoint)) {
            $this->markTestSkipped('Router not reachable — start the router first');
        }

        $this->routerPort = $port;
    }

    public function testCallerRegistrationSucceeds(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint($this->routerPort);
        $peerId = 'test-caller-' . Protocol::newMsgId();

        $transport = PeerTransport::dial($endpoint, $peerId, PeerClass::Caller);

        $this->assertFalse($transport->isClosed());
        $transport->disconnect();
        $transport->close();
    }

    public function testDuplicatePeerIdRejection(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint($this->routerPort);
        $peerId = 'test-dup-' . Protocol::newMsgId();

        $t1 = PeerTransport::dial($endpoint, $peerId, PeerClass::Caller);

        $this->expectException(RegistrationError::class);

        try {
            PeerTransport::dial($endpoint, $peerId, PeerClass::Caller);
        } finally {
            $t1->disconnect();
            $t1->close();
        }
    }

    public function testQueryPeerExists(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint($this->routerPort);
        $serviceId = 'test-svc-' . Protocol::newMsgId();

        $caller = PeerTransport::dial($endpoint, 'test-caller', PeerClass::Caller);

        // Service not yet registered
        $result = $caller->queryExists($serviceId);
        $this->assertFalse($result->exists);

        // Register a service
        $service = PeerTransport::dial($endpoint, $serviceId, PeerClass::Service);

        // Now it should exist
        $result = $caller->queryExists($serviceId);
        $this->assertTrue($result->exists);
        $this->assertSame(PeerClass::Service, $result->class);
        $this->assertTrue($result->connected);

        $caller->disconnect();
        $caller->close();
        $service->disconnect();
        $service->close();
    }

    public function testCallerCannotBeCalled(): void
    {
        $endpoint = RouterBootstrap::routerEndpoint($this->routerPort);

        // Register a caller
        $targetCaller = PeerTransport::dial($endpoint, 'test-target-caller', PeerClass::Caller);

        // Register another caller to attempt the call
        $attacker = PeerTransport::dial($endpoint, 'test-attacker', PeerClass::Caller);

        // Try to call the caller peer (should fail — not a service)
        $this->expectException(RouterError::class);
        $attacker->call('test-target-caller', 'echo', ['should fail']);

        $targetCaller->disconnect();
        $targetCaller->close();
        $attacker->disconnect();
        $attacker->close();
    }
}
