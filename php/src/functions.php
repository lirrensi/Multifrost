<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Public API functions for Multifrost PHP binding.
 *
 * These are loaded by Composer's "files" autoload so they are always available
 * after `require 'vendor/autoload.php'` without needing class-triggered autoload.
 *
 * Class-based APIs (Connection, Handle, ServiceWorker, etc.) continue to use
 * the classmap autoloader in composer.json.
 */

// ── Caller API ────────────────────────────────────────────────────

/**
 * Create a caller Connection for a target service peer.
 *
 * Usage:
 *   $connection = connect('math-service');
 *   $handle = $connection->handle();
 *   $handle->start();
 *   $result = $handle->add(1, 2);
 *   $handle->stop();
 */
function connect(string $defaultTargetPeerId, ?ConnectOptions $options = null): Connection
{
    return new Connection(
        $defaultTargetPeerId,
        $options ?? new ConnectOptions(),
    );
}

// ── Process API ────────────────────────────────────────────────────

/**
 * Convenience function for spawning a service process.
 *
 * @param array<string, string> $extraEnv
 */
function spawn(string $entrypoint, array $extraEnv = []): ServiceProcess
{
    return ServiceProcess::spawn($entrypoint, $extraEnv);
}

// ── Service API ────────────────────────────────────────────────────

/**
 * Run a service peer: connect to router, register as service, dispatch inbound calls.
 * Blocks until the connection closes or an error occurs.
 *
 * Usage:
 *   $worker = new class implements ServiceWorker {
 *       public function handleCall(string $function, array $args): mixed {
 *           return match ($function) {
 *               'add' => $args[0] + $args[1],
 *               default => throw new \RuntimeException("unknown function: {$function}"),
 *           };
 *       }
 *   };
 *   runService($worker, new ServiceContext(peerId: 'math-service'));
 */
function runService(ServiceWorker $worker, ServiceContext $context): void
{
    // Set entrypoint path env if provided
    if ($context->entrypointPath !== null && $context->entrypointPath !== '') {
        if (\getenv(Protocol::ENTRYPOINT_PATH_ENV) === false || \getenv(Protocol::ENTRYPOINT_PATH_ENV) === '') {
            $canonical = \realpath($context->entrypointPath);
            if ($canonical !== false) {
                \putenv(Protocol::ENTRYPOINT_PATH_ENV . '=' . $canonical);
            }
        }
    }

    // Resolve peer ID
    $peerId = resolveServicePeerId($context);
    if ($peerId === '' || $peerId === null) {
        throw new BootstrapError('could not resolve service peer_id');
    }

    // Resolve port
    $port = $context->routerPort;
    if ($port <= 0) {
        $port = RouterBootstrap::routerPortFromEnv();
    }

    // Ensure router
    RouterBootstrap::ensureRouter($port);
    $endpoint = RouterBootstrap::routerEndpoint($port);

    // Dial transport as service peer
    $transport = PeerTransport::dial(
        endpoint: $endpoint,
        peerId: $peerId,
        class: PeerClass::Service,
    );

    // Dispatch handler
    $handler = function (Frame $frame) use ($transport, $worker, $peerId): void {
        handleServiceFrame($transport, $worker, $peerId, $frame);
    };

    // Main service loop — blocks until connection closes
    $transport->serve($handler);
}

/**
 * Resolve the service peer ID: explicit > env entrypoint > executable path.
 * @internal
 */
function resolveServicePeerId(ServiceContext $context): ?string
{
    if ($context->peerId !== null && $context->peerId !== '') {
        return $context->peerId;
    }

    $entrypoint = \getenv(Protocol::ENTRYPOINT_PATH_ENV);
    if ($entrypoint !== false && $entrypoint !== '') {
        $realPath = \realpath($entrypoint);
        if ($realPath !== false) {
            return $realPath;
        }
    }

    // Fall back to PHP executable path
    if (isset($_SERVER['SCRIPT_FILENAME']) && $_SERVER['SCRIPT_FILENAME'] !== '') {
        $realPath = \realpath($_SERVER['SCRIPT_FILENAME']);
        if ($realPath !== false) {
            return $realPath;
        }
    }

    if (isset($_SERVER['PHP_SELF']) && $_SERVER['PHP_SELF'] !== '') {
        $realPath = \realpath($_SERVER['PHP_SELF']);
        if ($realPath !== false) {
            return $realPath;
        }
    }

    return null;
}

// ── Service dispatch helpers ───────────────────────────────────────

/**
 * Handle a single inbound frame for a service peer.
 * @internal
 */
function handleServiceFrame(
    PeerTransport $transport,
    ServiceWorker $worker,
    string $peerId,
    Frame $frame,
): void {
    switch ($frame->envelope->kind) {
        case Protocol::KIND_CALL:
            handleServiceCall($transport, $worker, $peerId, $frame);
            break;
        case Protocol::KIND_DISCONNECT:
            $transport->close();
            break;
        case Protocol::KIND_HEARTBEAT:
            // Ignore
            break;
        default:
            // Ignore unexpected kinds
            break;
    }
}

/**
 * Handle a CALL frame: decode body, invoke worker, send response or error.
 * @internal
 */
function handleServiceCall(
    PeerTransport $transport,
    ServiceWorker $worker,
    string $peerId,
    Frame $frame,
): void {
    $codec = new FrameCodec();

    try {
        $decoded = $codec->decode($frame->bodyBytes);
        if (!\is_array($decoded)) {
            sendServiceError($transport, $peerId, $frame->envelope, 'malformed_call_body', 'call body must be an object', ErrorOrigin::Protocol);
            return;
        }

        $function = (string) ($decoded['function'] ?? '');
        /** @var list<mixed> $args */
        $args = $decoded['args'] ?? [];

        $result = $worker->handleCall($function, $args);

        $responseBody = $codec->encode(['result' => $result]);
        $env = new Envelope(
            v: Protocol::PROTOCOL_VERSION,
            kind: Protocol::KIND_RESPONSE,
            msgId: $frame->envelope->msgId,
            from: $peerId,
            to: $frame->envelope->from,
            ts: Protocol::nowTs(),
        );
        $transport->send($env, $responseBody);
    } catch (\Throwable $e) {
        sendServiceError($transport, $peerId, $frame->envelope, 'remote_call_error', $e->getMessage(), ErrorOrigin::Application);
    }
}

/**
 * Send an error frame back to the caller.
 * @internal
 */
function sendServiceError(
    PeerTransport $transport,
    string $peerId,
    Envelope $requestEnvelope,
    string $code,
    string $message,
    ErrorOrigin $kind,
): void {
    $codec = new FrameCodec();

    $errorBody = $codec->encode([
        'code' => $code,
        'message' => $message,
        'kind' => $kind->value,
    ]);

    $env = new Envelope(
        v: Protocol::PROTOCOL_VERSION,
        kind: Protocol::KIND_ERROR,
        msgId: $requestEnvelope->msgId,
        from: $peerId,
        to: $requestEnvelope->from,
        ts: Protocol::nowTs(),
    );

    try {
        $transport->send($env, $errorBody);
    } catch (\Throwable) {
        // If we can't send the error, close the transport
        $transport->close();
    }
}

// ── Error translation ──────────────────────────────────────────────

/**
 * Translate a wire ErrorBody into the appropriate PHP exception.
 */
function errorFromWire(ErrorBody $body): MultifrostException
{
    return match ($body->kind) {
        ErrorOrigin::Protocol->value => new RouterError(
            errorCode: $body->code,
            message: $body->message,
        ),
        ErrorOrigin::Router->value, 'library' => new RouterError(
            errorCode: $body->code,
            message: $body->message,
        ),
        'application', ErrorOrigin::Remote->value => new RemoteCallError(
            message: $body->message,
        ),
        default => new RouterError(
            errorCode: $body->code,
            message: $body->message,
        ),
    };
}
