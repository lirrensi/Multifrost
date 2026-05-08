<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\Protocol;
use Multifrost\Envelope;

final class ProtocolTest extends TestCase
{
    // ── validateEnvelope ────────────────────────────────────────────

    public function testValidateEnvelopeAcceptsValid(): void
    {
        $env = [
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc-123',
            'from' => 'caller-1',
            'to' => 'service-1',
            'ts' => 1234567890.123,
        ];
        // Must not throw
        Protocol::validateEnvelope($env);
        $this->expectNotToPerformAssertions();
    }

    public function testValidateEnvelopeRejectsWrongVersion(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid protocol version');
        Protocol::validateEnvelope([
            'v' => 4,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsMissingVersion(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid protocol version');
        Protocol::validateEnvelope([
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsNullVersion(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid protocol version');
        Protocol::validateEnvelope([
            'v' => null,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsEmptyKind(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid or missing envelope kind');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => '',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsMissingKind(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid or missing envelope kind');
        Protocol::validateEnvelope([
            'v' => 5,
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsInvalidKindString(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid or missing envelope kind');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'invalid_kind_that_does_not_exist',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsMissingMsgId(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope msg_id');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsEmptyMsgId(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope msg_id');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => '',
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsMissingFrom(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope from');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsEmptyFrom(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope from');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => '',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsMissingTo(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope to');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsEmptyTo(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('missing envelope to');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => '',
            'ts' => 1.0,
        ]);
    }

    public function testValidateEnvelopeRejectsNonFloatNonIntTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts type');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 'not-a-number',
        ]);
    }

    public function testValidateEnvelopeRejectsNanTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts value');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => \NAN,
        ]);
    }

    public function testValidateEnvelopeRejectsInfiniteTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts value');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => \INF,
        ]);
    }

    public function testValidateEnvelopeRejectsNegativeInfiniteTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts value');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => -\INF,
        ]);
    }

    public function testValidateEnvelopeRejectsZeroTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts value');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 0,
        ]);
    }

    public function testValidateEnvelopeRejectsNegativeTs(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('invalid envelope ts value');
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => -1.0,
        ]);
    }

    public function testValidateEnvelopeAcceptsIntTs(): void
    {
        // ts as int (not float) should still pass
        Protocol::validateEnvelope([
            'v' => 5,
            'kind' => 'call',
            'msg_id' => 'abc',
            'from' => 'a',
            'to' => 'b',
            'ts' => 100,
        ]);
        $this->expectNotToPerformAssertions();
    }

    public function testValidateEnvelopeAcceptsAllValidKinds(): void
    {
        $kinds = Protocol::VALID_KINDS;
        foreach ($kinds as $kind) {
            Protocol::validateEnvelope([
                'v' => 5,
                'kind' => $kind,
                'msg_id' => 'abc',
                'from' => 'a',
                'to' => 'b',
                'ts' => 1.0,
            ]);
        }
        $this->expectNotToPerformAssertions();
    }

    // ── newEnvelope ─────────────────────────────────────────────────

    public function testNewEnvelopeGeneratesValidStructure(): void
    {
        $env = Protocol::newEnvelope('call', 'caller-1', 'service-1');

        $this->assertSame(5, $env->v);
        $this->assertSame('call', $env->kind);
        $this->assertSame('caller-1', $env->from);
        $this->assertSame('service-1', $env->to);
        $this->assertNotEmpty($env->msgId);
        $this->assertGreaterThan(0, $env->ts);
    }

    // ── newMsgId ────────────────────────────────────────────────────

    public function testNewMsgIdReturnsValidUuidV4Format(): void
    {
        $uuid = Protocol::newMsgId();
        // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        // where y is 8, 9, A, or B
        $pattern = '/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/';
        $this->assertMatchesRegularExpression($pattern, $uuid);
    }

    public function testNewMsgIdGeneratesUniqueIds(): void
    {
        $ids = [];
        $count = 1000;
        for ($i = 0; $i < $count; $i++) {
            $ids[] = Protocol::newMsgId();
        }
        $unique = \array_unique($ids);
        $this->assertCount($count, $unique, \sprintf('Expected all %d generated IDs to be unique', $count));
    }

    // ── nowTs ───────────────────────────────────────────────────────

    public function testNowTsReturnsPositiveFloat(): void
    {
        $ts = Protocol::nowTs();
        $this->assertGreaterThan(0, $ts);
    }

    public function testNowTsIsIncreasing(): void
    {
        $a = Protocol::nowTs();
        \usleep(1);
        $b = Protocol::nowTs();
        $this->assertGreaterThan($a, $b);
    }

    // ── Constants ───────────────────────────────────────────────────

    public function testProtocolKeyIsCorrect(): void
    {
        $this->assertSame('multifrost_ipc_v5', Protocol::PROTOCOL_KEY);
    }

    public function testProtocolVersionIsFive(): void
    {
        $this->assertSame(5, Protocol::PROTOCOL_VERSION);
    }

    public function testRouterPeerIdIsRouter(): void
    {
        $this->assertSame('router', Protocol::ROUTER_PEER_ID);
    }

    public function testDefaultRouterPortIs9981(): void
    {
        $this->assertSame(9981, Protocol::DEFAULT_ROUTER_PORT);
    }

    public function testValidKindsContainsAllExpectedKinds(): void
    {
        $expected = ['register', 'query', 'call', 'response', 'error', 'heartbeat', 'disconnect'];
        $this->assertSame($expected, Protocol::VALID_KINDS);
    }

    public function testQueryTypesAreCorrect(): void
    {
        $this->assertSame('peer.exists', Protocol::QUERY_PEER_EXISTS);
        $this->assertSame('peer.get', Protocol::QUERY_PEER_GET);
    }

    // ── Envelope::toArray / fromArray round trip ────────────────────

    public function testEnvelopeToArrayFromArrayRoundTrip(): void
    {
        $original = new Envelope(
            v: 5,
            kind: 'error',
            msgId: 'msg-999',
            from: 'svc-1',
            to: 'caller-1',
            ts: 987654321.0,
        );

        $array = $original->toArray();
        $restored = Envelope::fromArray($array);

        $this->assertSame($original->v, $restored->v);
        $this->assertSame($original->kind, $restored->kind);
        $this->assertSame($original->msgId, $restored->msgId);
        $this->assertSame($original->from, $restored->from);
        $this->assertSame($original->to, $restored->to);
        $this->assertSame($original->ts, $restored->ts);
    }

    public function testEnvelopeFromArrayRejectsInvalidInput(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        Envelope::fromArray([
            'v' => 5,
            'kind' => 'call',
            // missing msg_id
            'from' => 'a',
            'to' => 'b',
            'ts' => 1.0,
        ]);
    }
}
