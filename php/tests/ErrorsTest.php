<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\ErrorOrigin;
use Multifrost\MultifrostException;
use Multifrost\TransportError;
use Multifrost\BootstrapError;
use Multifrost\RegistrationError;
use Multifrost\RouterError;
use Multifrost\RemoteCallError;
use Multifrost\ErrorBody;
use function Multifrost\errorFromWire;

final class ErrorsTest extends TestCase
{
    // ── ErrorOrigin enum ────────────────────────────────────────────

    public function testErrorOriginHasAllExpectedCases(): void
    {
        $cases = ErrorOrigin::cases();
        $values = \array_map(fn (ErrorOrigin $e) => $e->value, $cases);

        $this->assertContains('transport', $values);
        $this->assertContains('bootstrap', $values);
        $this->assertContains('registration', $values);
        $this->assertContains('router', $values);
        $this->assertContains('protocol', $values);
        $this->assertContains('remote', $values);
        $this->assertContains('application', $values);
    }

    // ── Exception hierarchy ─────────────────────────────────────────

    public function testAllExceptionsExtendMultifrostException(): void
    {
        $this->assertInstanceOf(MultifrostException::class, new TransportError());
        $this->assertInstanceOf(MultifrostException::class, new BootstrapError());
        $this->assertInstanceOf(MultifrostException::class, new RegistrationError());
        $this->assertInstanceOf(MultifrostException::class, new RouterError('test'));
        $this->assertInstanceOf(MultifrostException::class, new RemoteCallError());
    }

    public function testMultifrostExceptionExtendsRuntimeException(): void
    {
        // Use reflection since MultifrostException is abstract
        $ref = new \ReflectionClass(MultifrostException::class);
        $this->assertTrue($ref->isSubclassOf(\RuntimeException::class));
    }

    // ── TransportError ──────────────────────────────────────────────

    public function testTransportErrorMessageFormat(): void
    {
        $e = new TransportError('connection refused');
        $this->assertStringContainsString('[transport]', $e->getMessage());
        $this->assertStringContainsString('connection refused', $e->getMessage());
    }

    public function testTransportErrorPreservesPrevious(): void
    {
        $prev = new \RuntimeException('underlying error');
        $e = new TransportError('failed', $prev);
        $this->assertSame($prev, $e->getPrevious());
    }

    // ── BootstrapError ──────────────────────────────────────────────

    public function testBootstrapErrorMessageFormat(): void
    {
        $e = new BootstrapError('router not found');
        $this->assertStringContainsString('[bootstrap]', $e->getMessage());
        $this->assertStringContainsString('router not found', $e->getMessage());
    }

    // ── RegistrationError ───────────────────────────────────────────

    public function testRegistrationErrorMessageFormat(): void
    {
        $e = new RegistrationError('duplicate peer_id');
        $this->assertStringContainsString('[registration]', $e->getMessage());
        $this->assertStringContainsString('duplicate peer_id', $e->getMessage());
    }

    // ── RouterError ─────────────────────────────────────────────────

    public function testRouterErrorMessageFormat(): void
    {
        $e = new RouterError('peer_not_found', 'target not found');
        $this->assertSame('peer_not_found', $e->errorCode);
        $this->assertStringContainsString('[router]', $e->getMessage());
        $this->assertStringContainsString('peer_not_found', $e->getMessage());
        $this->assertStringContainsString('target not found', $e->getMessage());
    }

    public function testRouterErrorWithoutMessage(): void
    {
        $e = new RouterError('timeout');
        $this->assertSame('timeout', $e->errorCode);
        // Should still have a message with error code
        $this->assertStringContainsString('timeout', $e->getMessage());
    }

    // ── RemoteCallError ─────────────────────────────────────────────

    public function testRemoteCallErrorMessageFormat(): void
    {
        $e = new RemoteCallError('service crashed');
        $this->assertStringContainsString('[remote]', $e->getMessage());
        $this->assertStringContainsString('service crashed', $e->getMessage());
    }

    // ── errorFromWire ───────────────────────────────────────────────

    public function testErrorFromWireMapsProtocolOriginToRouterError(): void
    {
        $body = new ErrorBody(
            code: 'malformed_call_body',
            message: 'call body must be an object',
            kind: ErrorOrigin::Protocol->value,
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RouterError::class, $e);
        $this->assertSame('malformed_call_body', $e->errorCode);
        $this->assertStringContainsString('call body must be an object', $e->getMessage());
    }

    public function testErrorFromWireMapsRouterOriginToRouterError(): void
    {
        $body = new ErrorBody(
            code: 'unknown_target',
            message: 'target does not exist',
            kind: ErrorOrigin::Router->value,
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RouterError::class, $e);
        $this->assertSame('unknown_target', $e->errorCode);
    }

    public function testErrorFromWireMapsLibraryKindToRouterError(): void
    {
        $body = new ErrorBody(
            code: 'internal_error',
            message: 'something went wrong',
            kind: 'library',
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RouterError::class, $e);
        $this->assertSame('internal_error', $e->errorCode);
    }

    public function testErrorFromWireMapsApplicationOriginToRemoteCallError(): void
    {
        $body = new ErrorBody(
            code: 'division_by_zero',
            message: 'division by zero',
            kind: ErrorOrigin::Application->value,
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RemoteCallError::class, $e);
        $this->assertStringContainsString('division by zero', $e->getMessage());
    }

    public function testErrorFromWireMapsRemoteOriginToRemoteCallError(): void
    {
        $body = new ErrorBody(
            code: 'remote_error',
            message: 'service failed',
            kind: ErrorOrigin::Remote->value,
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RemoteCallError::class, $e);
    }

    public function testErrorFromWireDefaultsToRouterError(): void
    {
        $body = new ErrorBody(
            code: 'unknown',
            message: 'some error',
            kind: 'unrecognized_kind',  // Doesn't match any known origin
        );

        $e = errorFromWire($body);

        $this->assertInstanceOf(RouterError::class, $e);
        $this->assertSame('unknown', $e->errorCode);
    }

    // ── ErrorBody construction ─────────────────────────────────────

    public function testErrorBodyConstructsCorrectly(): void
    {
        $body = new ErrorBody(
            code: 'test_code',
            message: 'test message',
            kind: 'router',
            stack: 'line 42',
            details: ['cause' => 'network'],
        );

        $this->assertSame('test_code', $body->code);
        $this->assertSame('test message', $body->message);
        $this->assertSame('router', $body->kind);
        $this->assertSame('line 42', $body->stack);
        $this->assertSame(['cause' => 'network'], $body->details);
    }

    public function testErrorBodyAllowsNullStackAndDetails(): void
    {
        $body = new ErrorBody(
            code: 'code',
            message: 'msg',
            kind: 'kind',
        );

        $this->assertNull($body->stack);
        $this->assertNull($body->details);
    }

    // ── Exception idempotency ───────────────────────────────────────

    public function testStopIsIdempotent(): void
    {
        // These constructors should never throw
        $e1 = new TransportError('msg');
        $e2 = new BootstrapError('msg');
        $e3 = new RegistrationError('msg');
        $e4 = new RouterError('code', 'msg');
        $e5 = new RemoteCallError('msg');

        $this->assertNotEmpty($e1->getMessage());
        $this->assertNotEmpty($e2->getMessage());
        $this->assertNotEmpty($e3->getMessage());
        $this->assertNotEmpty($e4->getMessage());
        $this->assertNotEmpty($e5->getMessage());
    }
}
