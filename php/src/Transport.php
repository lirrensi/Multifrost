<?php

declare(strict_types=1);

namespace Multifrost;

use WebSocket\Client;
use WebSocket\Configuration;
use WebSocket\Middleware\CloseHandler;
use WebSocket\Middleware\PingResponder;
use WebSocket\Message\Message;
use WebSocket\Exception\ExceptionInterface as WebSocketException;

/**
 * Live WebSocket peer transport.
 * Mirrors golang/transport.go.
 *
 * For callers: send a request, then read responses until msg_id matches.
 * For services: the read loop dispatches incoming calls to a handler.
 */
final class PeerTransport
{
    private Client $client;
    private FrameCodec $codec;
    private bool $closed = false;

    public function __construct(
        public readonly string $peerId,
        public readonly PeerClass $class,
    ) {
        $this->codec = new FrameCodec();
    }

    /**
     * Open WebSocket, send register, wait for ack.
     * Returns the connected PeerTransport.
     */
    /**
     * @param \Closure(Frame): void|null $dispatch unused; kept for API compatibility
     */
    public static function dial(
        string $endpoint,
        string $peerId,
        PeerClass $class,
        ?\Closure $dispatch = null,
        int $timeout = 5,
    ): self {
        $transport = new self($peerId, $class);

        try {
            $config = new Configuration(timeout: $timeout);
            $transport->client = new Client($endpoint, $config);
            $transport->client
                ->addMiddleware(new CloseHandler())
                ->addMiddleware(new PingResponder());
        } catch (\Throwable $e) {
            throw new TransportError(
                \sprintf('websocket dial failed: %s', $e->getMessage()),
                $e,
            );
        }

        $transport->register();

        return $transport;
    }

    /**
     * Send register frame and wait for ack.
     */
    private function register(): void
    {
        $body = ['peer_id' => $this->peerId, 'class' => $this->class->value];
        $bodyBytes = $this->codec->encode($body);
        $envelope = Protocol::newEnvelope(Protocol::KIND_REGISTER, $this->peerId, Protocol::ROUTER_PEER_ID);
        $frameBytes = $this->codec->encodeFrame($envelope, $bodyBytes);

        $this->writeBinary($frameBytes);

        $responseFrame = $this->readOneFrame();

        switch ($responseFrame->envelope->kind) {
            case Protocol::KIND_RESPONSE:
                $ack = $this->codec->decode($responseFrame->bodyBytes);
                if (!\is_array($ack)) {
                    throw new RegistrationError('invalid register ack format');
                }
                if (!($ack['accepted'] ?? false)) {
                    $reason = $ack['reason'] ?? 'router rejected registration';
                    throw new RegistrationError((string) $reason);
                }
                return;

            case Protocol::KIND_ERROR:
                $errBody = $this->decodeErrorBody($responseFrame->bodyBytes);
                throw new RegistrationError($errBody->message);

            default:
                throw new RegistrationError(
                    \sprintf('unexpected register reply kind: %s', $responseFrame->envelope->kind)
                );
        }
    }

    /**
     * Blocking call: send call frame, then read until matching msg_id response/error.
     *
     * @param list<mixed> $args
     */
    public function call(string $targetPeerId, string $function, array $args = []): mixed
    {
        $body = ['function' => $function, 'args' => $args];
        $bodyBytes = $this->codec->encode($body);
        $envelope = Protocol::newEnvelope(Protocol::KIND_CALL, $this->peerId, $targetPeerId);
        $msgId = $envelope->msgId;

        $frameBytes = $this->codec->encodeFrame($envelope, $bodyBytes);
        $this->writeBinary($frameBytes);

        // Read until we get a response/error with matching msg_id
        while (true) {
            $responseFrame = $this->readOneFrame();

            if ($responseFrame->envelope->msgId !== $msgId) {
                // Unsolicited frame — dispatch if service mode, otherwise skip
                continue;
            }

            switch ($responseFrame->envelope->kind) {
                case Protocol::KIND_RESPONSE:
                    $decoded = $this->codec->decode($responseFrame->bodyBytes);
                    if (\is_array($decoded) && \array_key_exists('result', $decoded)) {
                        return $decoded['result'];
                    }
                    return $decoded;

                case Protocol::KIND_ERROR:
                    $errBody = $this->decodeErrorBody($responseFrame->bodyBytes);
                    throw errorFromWire($errBody);

                default:
                    throw new RouterError(
                        'unexpected_response_kind',
                        \sprintf('unexpected frame kind in call response: %s', $responseFrame->envelope->kind),
                    );
            }
        }
    }

    /**
     * Query whether a peer exists.
     */
    public function queryExists(string $peerId): QueryExistsResponseBody
    {
        $body = ['query' => Protocol::QUERY_PEER_EXISTS, 'peer_id' => $peerId];
        $bodyBytes = $this->codec->encode($body);
        $envelope = Protocol::newEnvelope(Protocol::KIND_QUERY, $this->peerId, Protocol::ROUTER_PEER_ID);
        $msgId = $envelope->msgId;

        $frameBytes = $this->codec->encodeFrame($envelope, $bodyBytes);
        $this->writeBinary($frameBytes);

        $responseFrame = $this->readOneFrame();

        // The response msg_id should match, but handle mismatch gracefully
        // (query is simple request/response, router shouldn't interleave)

        if ($responseFrame->envelope->kind === Protocol::KIND_ERROR) {
            $errBody = $this->decodeErrorBody($responseFrame->bodyBytes);
            throw errorFromWire($errBody);
        }

        /** @var array<string, mixed> $decoded */
        $decoded = $this->codec->decode($responseFrame->bodyBytes);
        if (!\is_array($decoded)) { // @phpstan-ignore function.alreadyNarrowedType
            throw new RouterError('malformed_query_response', 'query response must be an object');
        }

        return new QueryExistsResponseBody(
            peerId: (string) ($decoded['peer_id'] ?? ''),
            exists: (bool) ($decoded['exists'] ?? false),
            class: isset($decoded['class']) ? PeerClass::tryFrom((string) $decoded['class']) : null,
            connected: (bool) ($decoded['connected'] ?? false),
        );
    }

    /**
     * Query full peer details.
     */
    public function queryGet(string $peerId): QueryGetResponseBody
    {
        $body = ['query' => Protocol::QUERY_PEER_GET, 'peer_id' => $peerId];
        $bodyBytes = $this->codec->encode($body);
        $envelope = Protocol::newEnvelope(Protocol::KIND_QUERY, $this->peerId, Protocol::ROUTER_PEER_ID);
        $msgId = $envelope->msgId;

        $frameBytes = $this->codec->encodeFrame($envelope, $bodyBytes);
        $this->writeBinary($frameBytes);

        $responseFrame = $this->readOneFrame();

        if ($responseFrame->envelope->kind === Protocol::KIND_ERROR) {
            $errBody = $this->decodeErrorBody($responseFrame->bodyBytes);
            throw errorFromWire($errBody);
        }

        /** @var array<string, mixed> $decoded */
        $decoded = $this->codec->decode($responseFrame->bodyBytes);
        if (!\is_array($decoded)) { // @phpstan-ignore function.alreadyNarrowedType
            throw new RouterError('malformed_query_response', 'query response must be an object');
        }

        return new QueryGetResponseBody(
            peerId: (string) ($decoded['peer_id'] ?? ''),
            exists: (bool) ($decoded['exists'] ?? false),
            class: isset($decoded['class']) ? PeerClass::tryFrom((string) $decoded['class']) : null,
            connected: (bool) ($decoded['connected'] ?? false),
        );
    }

    /**
     * Graceful disconnect.
     */
    public function disconnect(): void
    {
        if ($this->closed) {
            return;
        }
        $envelope = Protocol::newEnvelope(Protocol::KIND_DISCONNECT, $this->peerId, Protocol::ROUTER_PEER_ID);
        $frameBytes = $this->codec->encodeFrame($envelope, '');
        try {
            $this->writeBinary($frameBytes);
        } catch (\Throwable) {
            // Best effort
        }
        $this->close();
    }

    /**
     * Close the WebSocket connection.
     */
    public function close(): void
    {
        if ($this->closed) {
            return;
        }
        $this->closed = true;
        try {
            $this->client->close();
        } catch (\Throwable) {
            // Best effort
        }
    }

    /**
     * Service read loop: receive frames and dispatch to handler.
     * Blocks until connection closes or dispatch callback throws.
     */
    public function serve(\Closure $handler): void
    {
        while (!$this->closed) {
            try {
                $frame = $this->readOneFrame();
            } catch (WebSocketException) {
                $this->closed = true;
                break;
            } catch (TransportError) {
                $this->closed = true;
                break;
            }

            switch ($frame->envelope->kind) {
                case Protocol::KIND_CALL:
                    $handler($frame);
                    break;
                case Protocol::KIND_DISCONNECT:
                    $this->close();
                    break 2;
                case Protocol::KIND_HEARTBEAT:
                    // Ignore
                    break;
                default:
                    // Ignore unexpected kinds in service mode
                    break;
            }
        }
    }

    /**
     * Send a prepared frame (envelope + body bytes) directly.
     */
    public function send(Envelope $envelope, string $bodyBytes): void
    {
        $frameBytes = $this->codec->encodeFrame($envelope, $bodyBytes);
        $this->writeBinary($frameBytes);
    }

    public function isClosed(): bool
    {
        return $this->closed;
    }

    // ── Private helpers ──────────────────────────────────────────

    /**
     * Write raw binary bytes to the WebSocket.
     */
    private function writeBinary(string $bytes): void
    {
        if ($this->closed) {
            throw new TransportError('transport closed');
        }
        try {
            $this->client->binary($bytes);
        } catch (\Throwable $e) {
            $this->close();
            throw new TransportError($e->getMessage(), $e);
        }
    }

    /**
     * Read one binary frame from the WebSocket, blocking.
     */
    private function readOneFrame(): Frame
    {
        try {
            $message = $this->client->receive();
            $content = $message->getContent();
            return $this->codec->decodeFrame($content);
        } catch (WebSocketException $e) {
            $this->closed = true;
            throw new TransportError(
                \sprintf('websocket connection error: %s', $e->getMessage()),
                $e,
            );
        } catch (\InvalidArgumentException $e) {
            throw new TransportError(
                \sprintf('frame decode error: %s', $e->getMessage()),
                $e,
            );
        }
    }

    /**
     * Decode an error body from raw bytes.
     */
    private function decodeErrorBody(string $bodyBytes): ErrorBody
    {
        $decoded = $this->codec->decode($bodyBytes);
        if (!\is_array($decoded)) {
            throw new RouterError('malformed_error_body', 'error body must be an object');
        }

        return new ErrorBody(
            code: (string) ($decoded['code'] ?? 'unknown'),
            message: (string) ($decoded['message'] ?? ''),
            kind: (string) ($decoded['kind'] ?? ErrorOrigin::Router->value),
            stack: isset($decoded['stack']) ? (string) $decoded['stack'] : null,
            details: $decoded['details'] ?? null,
        );
    }
}
