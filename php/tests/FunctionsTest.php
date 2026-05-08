<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\Protocol;
use Multifrost\ServiceContext;
use Multifrost\ServiceWorker;
use Multifrost\Envelope;
use Multifrost\Frame;
use Multifrost\PeerClass;
use Multifrost\PeerTransport;
use Multifrost\ErrorOrigin;
use Multifrost\BootstrapError;
use function Multifrost\resolveServicePeerId;
use function Multifrost\sendServiceError;
use function Multifrost\handleServiceFrame;

final class FunctionsTest extends TestCase
{
    private string $origServer;

    protected function setUp(): void
    {
        $this->origServer = $_SERVER['SCRIPT_FILENAME'] ?? '';
    }

    protected function tearDown(): void
    {
        if ($this->origServer !== '') {
            $_SERVER['SCRIPT_FILENAME'] = $this->origServer;
        } else {
            unset($_SERVER['SCRIPT_FILENAME']);
        }
        \putenv(Protocol::ENTRYPOINT_PATH_ENV);
    }

    // ── resolveServicePeerId ────────────────────────────────────────

    public function testResolvePeerIdUsesExplicitPeerId(): void
    {
        $context = new ServiceContext(peerId: 'my-explicit-service');
        $result = resolveServicePeerId($context);
        $this->assertSame('my-explicit-service', $result);
    }

    public function testResolvePeerIdUsesEntrypointEnv(): void
    {
        $testFile = __FILE__;
        \putenv(Protocol::ENTRYPOINT_PATH_ENV . '=' . $testFile);

        $context = new ServiceContext(peerId: null);
        $result = resolveServicePeerId($context);

        $expected = \realpath($testFile);
        $this->assertSame($expected, $result);
    }

    public function testResolvePeerIdFallsBackToScriptFilename(): void
    {
        $_SERVER['SCRIPT_FILENAME'] = __FILE__;

        $context = new ServiceContext(peerId: null);
        $result = resolveServicePeerId($context);

        $expected = \realpath(__FILE__);
        $this->assertSame($expected, $result);
    }

    public function testResolvePeerIdFallsBackToPhpSelf(): void
    {
        unset($_SERVER['SCRIPT_FILENAME']);
        $_SERVER['PHP_SELF'] = __FILE__;

        $context = new ServiceContext(peerId: null);
        $result = resolveServicePeerId($context);

        $expected = \realpath(__FILE__);
        $this->assertSame($expected, $result);
    }

    public function testResolvePeerIdReturnsNullWhenAllEmpty(): void
    {
        unset($_SERVER['SCRIPT_FILENAME'], $_SERVER['PHP_SELF']);
        \putenv(Protocol::ENTRYPOINT_PATH_ENV);

        $context = new ServiceContext(peerId: null);
        $result = resolveServicePeerId($context);

        $this->assertNull($result);
    }

    public function testResolvePeerIdExplicitBeatsEnv(): void
    {
        \putenv(Protocol::ENTRYPOINT_PATH_ENV . '=/some/other/file.php');

        $context = new ServiceContext(peerId: 'explicit-wins');
        $result = resolveServicePeerId($context);

        $this->assertSame('explicit-wins', $result);
    }

    public function testResolveWithEmptyStringPeerIdFallsThrough(): void
    {
        $testFile = __FILE__;
        \putenv(Protocol::ENTRYPOINT_PATH_ENV . '=' . $testFile);

        $context = new ServiceContext(peerId: '');
        $result = resolveServicePeerId($context);

        $expected = \realpath($testFile);
        $this->assertSame($expected, $result);
    }

    // ── handleServiceFrame (using mocks) ────────────────────────────

    public function testHandleServiceFrameDisconnectClosesTransport(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->expects($this->once())->method('close');

        $worker = $this->createEchoWorker();
        $frame = new Frame(
            envelope: new Envelope(5, Protocol::KIND_DISCONNECT, 'msg-1', 'caller-1', 'svc-1', 1.0),
            bodyBytes: '',
        );

        handleServiceFrame($transport, $worker, 'svc-1', $frame);
    }

    public function testHandleServiceFrameCallDispatchesToWorker(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->expects($this->once())->method('send');

        $worker = $this->createEchoWorker();
        $codec = new \Multifrost\FrameCodec();
        $bodyBytes = $codec->encode(['function' => 'echo', 'args' => ['hello']]);

        $frame = new Frame(
            envelope: new Envelope(5, Protocol::KIND_CALL, 'msg-1', 'caller-1', 'svc-1', 1.0),
            bodyBytes: $bodyBytes,
        );

        handleServiceFrame($transport, $worker, 'svc-1', $frame);
    }

    public function testHandleServiceFrameHeartbeatIsIgnored(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->expects($this->never())->method('close');
        $transport->expects($this->never())->method('send');

        $worker = $this->createEchoWorker();
        $frame = new Frame(
            envelope: new Envelope(5, Protocol::KIND_HEARTBEAT, 'msg-1', 'caller-1', 'svc-1', 1.0),
            bodyBytes: '',
        );

        handleServiceFrame($transport, $worker, 'svc-1', $frame);
    }

    public function testHandleServiceFrameUnknownKindIsIgnored(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->expects($this->never())->method('close');
        $transport->expects($this->never())->method('send');

        $worker = $this->createEchoWorker();
        $frame = new Frame(
            envelope: new Envelope(5, 'register', 'msg-1', 'caller-1', 'svc-1', 1.0),
            bodyBytes: '',
        );

        handleServiceFrame($transport, $worker, 'svc-1', $frame);
    }

    // ── sendServiceError ────────────────────────────────────────────

    public function testSendServiceErrorSendsErrorFrame(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->expects($this->once())->method('send');

        $requestEnvelope = new Envelope(5, Protocol::KIND_CALL, 'req-1', 'caller-1', 'svc-1', 1.0);

        sendServiceError(
            $transport,
            'svc-1',
            $requestEnvelope,
            'division_by_zero',
            'Cannot divide by zero',
            ErrorOrigin::Application,
        );
    }

    public function testSendServiceErrorClosesTransportOnSendFailure(): void
    {
        $transport = $this->createPeerTransportMock();
        $transport->method('send')->willThrowException(new \RuntimeException('send failed'));
        $transport->expects($this->once())->method('close');

        $requestEnvelope = new Envelope(5, Protocol::KIND_CALL, 'req-1', 'caller-1', 'svc-1', 1.0);

        sendServiceError(
            $transport,
            'svc-1',
            $requestEnvelope,
            'error',
            'msg',
            ErrorOrigin::Application,
        );
    }

    // ── Test helpers ────────────────────────────────────────────────

    /**
     * @return PeerTransport&\PHPUnit\Framework\MockObject\MockObject
     */
    private function createPeerTransportMock(): PeerTransport
    {
        $mock = $this->createMock(PeerTransport::class);
        // Default: stub methods as no-ops (void return type — no willReturn needed)
        $mock->method('send')->willReturnCallback(function (): void {});
        $mock->method('close')->willReturnCallback(function (): void {});
        $mock->method('isClosed')->willReturn(false);
        return $mock;
    }

    private function createEchoWorker(): ServiceWorker
    {
        return new class implements ServiceWorker {
            public function handleCall(string $function, array $args): mixed
            {
                return $args[0] ?? null;
            }
        };
    }
}
