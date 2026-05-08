<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\FrameCodec;
use Multifrost\Envelope;
use Multifrost\Protocol;

final class FrameEdgeCaseTest extends TestCase
{
    private FrameCodec $codec;

    protected function setUp(): void
    {
        $this->codec = new FrameCodec();
    }

    // ── Body edge cases ─────────────────────────────────────────────

    public function testEmptyBodyRoundTrip(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_RESPONSE, 'svc-1', 'caller-1');
        $bodyBytes = '';

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame($bodyBytes, $decoded->bodyBytes);
        $this->assertSame($envelope->msgId, $decoded->envelope->msgId);
    }

    public function testBinaryBodyRoundTrip(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'svc-1');
        // Binary data including null bytes and high bytes
        $bodyBytes = "\x00\x01\x02\xFF\xFE\x7F\x80\x00hello\x00world\x00";

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame($bodyBytes, $decoded->bodyBytes);
    }

    public function testLargeBodyRoundTrip(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'svc-1');
        // 100KB of body data
        $bodyBytes = \str_repeat('Multifrost-RPC-data-', 5000);

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame(\strlen($bodyBytes), \strlen($decoded->bodyBytes));
        $this->assertSame($bodyBytes, $decoded->bodyBytes);
    }

    public function testVeryLargeBodyRoundTrip(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'svc-1');
        // 1MB of body data
        $bodyBytes = \str_repeat('X', 1024 * 1024);

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame(1024 * 1024, \strlen($decoded->bodyBytes));
        $this->assertSame($bodyBytes, $decoded->bodyBytes);
    }

    public function testMsgPackBodyRoundTrip(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'svc-1');
        $body = ['function' => 'add', 'args' => [1, 2]];
        $bodyBytes = $this->codec->encode($body);

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);
        $decodedBody = $this->codec->decode($decoded->bodyBytes);

        $this->assertSame('add', $decodedBody['function']);
        $this->assertSame([1, 2], $decodedBody['args']);
    }

    // ── Encode/decode all message kinds ──────────────────────────────

    public function testAllKindsRoundTrip(): void
    {
        $bodyBytes = $this->codec->encode(['result' => 42]);
        $kinds = Protocol::VALID_KINDS;

        foreach ($kinds as $kind) {
            $envelope = new Envelope(
                v: 5,
                kind: $kind,
                msgId: 'msg-' . $kind,
                from: 'test-' . $kind,
                to: 'router',
                ts: 1000.0,
            );

            $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
            $decoded = $this->codec->decodeFrame($encoded);

            $this->assertSame($kind, $decoded->envelope->kind);
            $this->assertSame('msg-' . $kind, $decoded->envelope->msgId);
            $this->assertSame($bodyBytes, $decoded->bodyBytes);
        }
    }

    // ── Decode failure edge cases ───────────────────────────────────

    public function testDecodeRejectsEmptyData(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->codec->decodeFrame('');
    }

    public function testDecodeRejectsThreeBytes(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Need 4 bytes minimum for the length prefix
        $this->codec->decodeFrame("abc");
    }

    public function testDecodeRejectsFourBytesWithNoEnvelope(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Claims envelope is 100 bytes but only provides 4 + nothing
        $this->codec->decodeFrame(\pack('N', 100));
    }

    public function testDecodeRejectsNegativeEnvelopeLength(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Pack a very large unsigned int that overflows to negative when cast to int
        $this->codec->decodeFrame(\pack('N', 0xFFFFFFFF) . "data");
    }

    public function testDecodeRejectsLargeEnvelopeLengthExceedingData(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->codec->decodeFrame(\pack('N', 9999) . "short");
    }

    public function testDecodeRejectsMalformedEnvelope(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Envelope bytes that are valid msgpack but not a valid envelope (missing required fields)
        $badEnvelope = $this->codec->encode(['v' => 5, 'kind' => 'call']);  // missing msg_id, from, to
        $len = \strlen($badEnvelope);
        $this->codec->decodeFrame(\pack('N', $len) . $badEnvelope);
    }

    public function testDecodeRejectsNonArrayEnvelope(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        // Encode a scalar as the envelope
        $badEnvelope = $this->codec->encode('just a string');
        $len = \strlen($badEnvelope);
        $this->codec->decodeFrame(\pack('N', $len) . $badEnvelope);
    }

    // ── MsgPack-specific edge cases ─────────────────────────────────

    public function testMsgPackEncodesUtf8Strings(): void
    {
        $data = ['emoji' => '🔥 PHP 🔥', 'unicode' => '日本語'];
        $encoded = $this->codec->encode($data);
        $decoded = $this->codec->decode($encoded);

        $this->assertSame('🔥 PHP 🔥', $decoded['emoji']);
        $this->assertSame('日本語', $decoded['unicode']);
    }

    public function testMsgPackLargeArrayRoundTrip(): void
    {
        $data = \range(0, 999);
        $encoded = $this->codec->encode($data);
        $decoded = $this->codec->decode($encoded);

        $this->assertCount(1000, $decoded);
        $this->assertSame(500, $decoded[500]);
    }

    public function testMsgPackNestedArrays(): void
    {
        $data = [
            'level1' => [
                'level2' => [
                    'level3' => 'deep',
                    'numbers' => [1, [2, [3]]],
                ],
            ],
        ];
        $encoded = $this->codec->encode($data);
        $decoded = $this->codec->decode($encoded);

        $this->assertSame('deep', $decoded['level1']['level2']['level3']);
        $this->assertSame(3, $decoded['level1']['level2']['numbers'][1][1][0]);
    }

    public function testMsgPackHandlesMaxSafeInteger(): void
    {
        $data = ['big' => 9007199254740991];  // MAX_SAFE_INTEGER
        $encoded = $this->codec->encode($data);
        $decoded = $this->codec->decode($encoded);

        $this->assertSame(9007199254740991, $decoded['big']);
    }

    public function testMsgPackHandlesNegativeIntegers(): void
    {
        $data = ['neg' => -9223372036854775808];
        $encoded = $this->codec->encode($data);
        $decoded = $this->codec->decode($encoded);

        $this->assertSame(-9223372036854775808, $decoded['neg']);
    }

    // ── Frame with envelope that has long peer IDs ──────────────────

    public function testLongPeerIdsRoundTrip(): void
    {
        $envelope = new Envelope(
            v: 5,
            kind: 'call',
            msgId: 'msg-1',
            from: \str_repeat('caller-', 100),   // 700 chars
            to: \str_repeat('service-', 100),     // 800 chars
            ts: 1234.0,
        );
        $bodyBytes = $this->codec->encode(['result' => true]);

        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);
        $decoded = $this->codec->decodeFrame($encoded);

        $this->assertSame($envelope->from, $decoded->envelope->from);
        $this->assertSame($envelope->to, $decoded->envelope->to);
    }

    // ── DecodeFrame with extra trailing bytes (body is everything after envelope) ──

    public function testTrailingBytesArePartOfBody(): void
    {
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, 'caller-1', 'svc-1');
        $bodyBytes = $this->codec->encode(['function' => 'test']);
        $encoded = $this->codec->encodeFrame($envelope, $bodyBytes);

        // Append extra bytes — they should be part of the body
        $tampered = $encoded . 'EXTRA_BYTES_AT_END';

        $decoded = $this->codec->decodeFrame($tampered);

        // Body should include the original message plus the extra trailing bytes
        $this->assertStringEndsWith('EXTRA_BYTES_AT_END', $decoded->bodyBytes);
    }
}
