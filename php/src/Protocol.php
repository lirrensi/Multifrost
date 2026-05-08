<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Canonical v5 wire shapes and protocol constants.
 * Mirrors golang/protocol.go.
 */
final class Protocol
{
    public const PROTOCOL_KEY = 'multifrost_ipc_v5';
    public const PROTOCOL_VERSION = 5;
    public const ROUTER_PEER_ID = 'router';
    public const DEFAULT_ROUTER_PORT = 9981;
    public const ROUTER_PORT_ENV = 'MULTIFROST_ROUTER_PORT';
    public const ROUTER_LOG_PATH_SUFFIX = '.multifrost/router.log';
    public const ROUTER_LOCK_PATH_SUFFIX = '.multifrost/router.lock';
    public const ROUTER_BIN_ENV = 'MULTIFROST_ROUTER_BIN';
    public const ENTRYPOINT_PATH_ENV = 'MULTIFROST_ENTRYPOINT_PATH';

    // Message kinds
    public const KIND_REGISTER = 'register';
    public const KIND_QUERY = 'query';
    public const KIND_CALL = 'call';
    public const KIND_RESPONSE = 'response';
    public const KIND_ERROR = 'error';
    public const KIND_HEARTBEAT = 'heartbeat';
    public const KIND_DISCONNECT = 'disconnect';

    // Query types
    public const QUERY_PEER_EXISTS = 'peer.exists';
    public const QUERY_PEER_GET = 'peer.get';

    // Valid kinds for fast lookup
    public const VALID_KINDS = [
        self::KIND_REGISTER,
        self::KIND_QUERY,
        self::KIND_CALL,
        self::KIND_RESPONSE,
        self::KIND_ERROR,
        self::KIND_HEARTBEAT,
        self::KIND_DISCONNECT,
    ];

    /**
     * Validate an envelope mapping. Throws on invalid data.
     *
     * @param array<string, mixed> $env
     * @throws \InvalidArgumentException
     */
    public static function validateEnvelope(array $env): void
    {
        if (($env['v'] ?? null) !== self::PROTOCOL_VERSION) {
            throw new \InvalidArgumentException(
                \sprintf('invalid protocol version: %s', $env['v'] ?? 'null')
            );
        }
        if (empty($env['kind']) || !\in_array($env['kind'], self::VALID_KINDS, true)) {
            throw new \InvalidArgumentException(
                \sprintf('invalid or missing envelope kind: %s', $env['kind'] ?? 'null')
            );
        }
        if (empty($env['msg_id'])) {
            throw new \InvalidArgumentException('missing envelope msg_id');
        }
        if (empty($env['from'])) {
            throw new \InvalidArgumentException('missing envelope from');
        }
        if (empty($env['to'])) {
            throw new \InvalidArgumentException('missing envelope to');
        }
        $ts = $env['ts'] ?? 0.0;
        if (!\is_float($ts) && !\is_int($ts)) {
            throw new \InvalidArgumentException('invalid envelope ts type');
        }
        if (\is_nan($ts) || \is_infinite($ts) || $ts <= 0) {
            throw new \InvalidArgumentException('invalid envelope ts value');
        }
    }

    /**
     * Create a new envelope with a generated msg_id and current timestamp.
     */
    public static function newEnvelope(string $kind, string $from, string $to): Envelope
    {
        return new Envelope(
            v: self::PROTOCOL_VERSION,
            kind: $kind,
            msgId: self::newMsgId(),
            from: $from,
            to: $to,
            ts: self::nowTs(),
        );
    }

    public static function nowTs(): float
    {
        return \microtime(true);
    }

    public static function newMsgId(): string
    {
        // UUID v4 generation without external dependency
        $data = \random_bytes(16);
        $data[6] = \chr((\ord($data[6]) & 0x0F) | 0x40);
        $data[8] = \chr((\ord($data[8]) & 0x3F) | 0x80);
        return \sprintf(
            '%s-%s-%s-%s-%s',
            \bin2hex(\substr($data, 0, 4)),
            \bin2hex(\substr($data, 4, 2)),
            \bin2hex(\substr($data, 6, 2)),
            \bin2hex(\substr($data, 8, 2)),
            \bin2hex(\substr($data, 10, 6)),
        );
    }
}

/**
 * Peer class enum.
 */
enum PeerClass: string
{
    case Service = 'service';
    case Caller = 'caller';
}

/**
 * Envelope — routing metadata for every frame.
 */
final readonly class Envelope
{
    public function __construct(
        public int $v,
        public string $kind,
        public string $msgId,
        public string $from,
        public string $to,
        public float $ts,
    ) {}

    /**
     * Convert to array for msgpack encoding.
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        return [
            'v' => $this->v,
            'kind' => $this->kind,
            'msg_id' => $this->msgId,
            'from' => $this->from,
            'to' => $this->to,
            'ts' => $this->ts,
        ];
    }

    /**
     * Create from msgpack-decoded array.
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        Protocol::validateEnvelope($data);
        return new self(
            v: (int) $data['v'],
            kind: (string) $data['kind'],
            msgId: (string) $data['msg_id'],
            from: (string) $data['from'],
            to: (string) $data['to'],
            ts: (float) $data['ts'],
        );
    }
}

/**
 * Wire body structs — matching golang/protocol.go exactly.
 */

final readonly class RegisterBody
{
    public function __construct(
        public string $peerId,
        public PeerClass $class,
    ) {}
}

final readonly class RegisterAckBody
{
    public function __construct(
        public bool $accepted,
        public ?string $reason = null,
    ) {}
}

final readonly class QueryBody
{
    public function __construct(
        public string $query,
        public string $peerId,
    ) {}
}

final readonly class QueryExistsResponseBody
{
    public function __construct(
        public string $peerId,
        public bool $exists,
        public ?PeerClass $class = null,
        public bool $connected = false,
    ) {}
}

final readonly class QueryGetResponseBody
{
    public function __construct(
        public string $peerId,
        public bool $exists,
        public ?PeerClass $class = null,
        public bool $connected = false,
    ) {}
}

final readonly class CallBody
{
    public function __construct(
        public string $function,
        /** @var list<mixed> */
        public array $args = [],
    ) {}
}

final readonly class ResponseBody
{
    public function __construct(
        public mixed $result = null,
    ) {}
}

final readonly class ErrorBody
{
    public function __construct(
        public string $code,
        public string $message,
        public string $kind,
        public ?string $stack = null,
        public mixed $details = null,
    ) {}
}

/**
 * Decoded frame — envelope plus raw body bytes.
 */
final readonly class Frame
{
    public function __construct(
        public Envelope $envelope,
        /** @var string raw msgpack bytes */
        public string $bodyBytes,
    ) {}
}
