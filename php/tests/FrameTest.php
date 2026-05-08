<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\FrameCodec;
use Multifrost\Envelope;
use Multifrost\Protocol;

final class FrameTest extends TestCase
{
    private FrameCodec $codec;

    protected function setUp(): void
    {
        $this->codec = new FrameCodec();
    }

    public function testFrameEncodeDecodePreservesBodyBytes(): void
    {
        $bodyBytes = '{"hello":"world"}';
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'math-service');

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame($envelope->kind, $decoded->envelope->kind);
        $this->assertSame($envelope->msgId, $decoded->envelope->msgId);
        $this->assertSame($envelope->from, $decoded->envelope->from);
        $this->assertSame($envelope->to, $decoded->envelope->to);
        $this->assertSame($bodyBytes, $decoded->bodyBytes);
    }

    public function testDecodeRejectsTooShortData(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->codec->decodeFrame('ab');
    }

    public function testDecodeRejectsShorterThanDeclaredEnvelopeLength(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Claim envelope is 1000 bytes but only provide 5
        $this->codec->decodeFrame(\pack('N', 1000) . 'short');
    }

    public function testEnvelopeValidationRejectsMissingFields(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        Protocol::validateEnvelope(['v' => 5]);
    }

    public function testEnvelopeFromArrayRoundTrip(): void
    {
        $original = Protocol::newEnvelope(Protocol::KIND_RESPONSE, 'math-service', 'caller-1');
        $array = $original->toArray();
        $restored = Envelope::fromArray($array);

        $this->assertSame($original->kind, $restored->kind);
        $this->assertSame($original->msgId, $restored->msgId);
        $this->assertSame($original->from, $restored->from);
        $this->assertSame($original->to, $restored->to);
        $this->assertEqualsWithDelta($original->ts, $restored->ts, 0.001);
    }

    public function testMsgPackRoundTripPreservesTypes(): void
    {
        $value = [
            'string' => 'hello',
            'int' => 42,
            'float' => 3.14,
            'bool' => true,
            'null' => null,
            'array' => [1, 2, 3],
            'nested' => ['a' => 1, 'b' => 2],
        ];

        $encoded = $this->codec->encode($value);
        $decoded = $this->codec->decode($encoded);

        $this->assertIsArray($decoded);
        $this->assertSame('hello', $decoded['string']);
        $this->assertSame(42, $decoded['int']);
        $this->assertEqualsWithDelta(3.14, $decoded['float'], 0.001);
        $this->assertTrue($decoded['bool']);
        $this->assertNull($decoded['null']);
        $this->assertSame([1, 2, 3], $decoded['array']);
        $this->assertSame(['a' => 1, 'b' => 2], $decoded['nested']);
    }
}
