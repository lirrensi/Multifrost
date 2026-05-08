<?php

declare(strict_types=1);

namespace Multifrost;

use MessagePack\Packer;
use MessagePack\PackOptions;
use MessagePack\BufferUnpacker;

/**
 * Binary frame encode/decode.
 * Layout: [u32 big-endian envelope_len][msgpack envelope][msgpack body]
 * Mirrors golang/frame.go.
 */
final class FrameCodec
{
    private Packer $packer;
    private BufferUnpacker $unpackerPrototype;

    public function __construct()
    {
        $this->packer = new Packer(
            PackOptions::DETECT_STR_BIN | PackOptions::DETECT_ARR_MAP
        );
        $this->unpackerPrototype = new BufferUnpacker();
    }

    /**
     * MsgPack-encode a value to bytes.
     */
    public function encode(mixed $value): string
    {
        return $this->packer->pack($value);
    }

    /**
     * MsgPack-decode bytes to a PHP value.
     */
    public function decode(string $data): mixed
    {
        $unpacker = clone $this->unpackerPrototype;
        $unpacker->reset($data);
        return $unpacker->unpack();
    }

    /**
     * Encode a full frame: [u32 BE envelope_len][msgpack envelope][msgpack body].
     */
    public function encodeFrame(Envelope $envelope, string $bodyBytes): string
    {
        $envelopeBytes = $this->encode($envelope->toArray());
        $len = \strlen($envelopeBytes);
        return \pack('N', $len) . $envelopeBytes . $bodyBytes;
    }

    /**
     * Decode a full frame from raw bytes.
     * Returns a Frame with the decoded envelope and raw body bytes.
     */
    public function decodeFrame(string $data): Frame
    {
        if (\strlen($data) < 4) {
            throw new \InvalidArgumentException('frame too short for envelope length');
        }

        $unpacked = \unpack('Nlen', \substr($data, 0, 4));
        $envelopeLen = (int) $unpacked['len'];

        if ($envelopeLen < 0) {
            throw new \InvalidArgumentException('invalid envelope length');
        }
        if (\strlen($data) < 4 + $envelopeLen) {
            throw new \InvalidArgumentException('frame shorter than declared envelope length');
        }

        $envelopeBytes = \substr($data, 4, $envelopeLen);
        $bodyBytes = \substr($data, 4 + $envelopeLen);

        $decoded = $this->decode($envelopeBytes);
        if (!\is_array($decoded)) {
            throw new \InvalidArgumentException('envelope must decode to an array');
        }

        /** @var array<string, mixed> $decoded */
        $envelope = Envelope::fromArray($decoded);

        return new Frame(envelope: $envelope, bodyBytes: $bodyBytes);
    }
}
