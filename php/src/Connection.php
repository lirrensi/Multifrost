<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Caller-side v5 configuration and live handle.
 * Mirrors golang/connection.go.
 */
final readonly class ConnectOptions
{
    public function __construct(
        public ?string $peerId = null,
        public int $routerPort = 0,
        public float $requestTimeout = 30.0,
        public float $bootstrapTimeout = 10.0,
        public bool $validateTarget = false,
    ) {}
}

final class Connection
{
    /**
     * @internal
     */
    public function __construct(
        public readonly string $defaultTargetPeerId,
        public readonly ConnectOptions $options,
    ) {}

    /**
     * Create a new live handle from this connection configuration.
     */
    public function handle(): Handle
    {
        return new Handle($this->defaultTargetPeerId, $this->options);
    }
}

/**
 * Live caller handle — owns the WebSocket transport.
 *
 * Dynamic method calls are dispatched via __call() to remote service.
 *
 * @method mixed add(mixed ...$args)
 * @method mixed multiply(mixed ...$args)
 * @method mixed throw_error(mixed ...$args)
 * @method mixed echo(mixed ...$args)
 *
 * Usage:
 *   $handle = connect('math-service')->handle();
 *   $handle->start();
 *   $result = $handle->add(1, 2);
 *   $handle->stop();
 */
final class Handle
{
    private ?PeerTransport $transport = null;
    private bool $started = false;
    private string $peerId = '';

    /**
     * @internal
     */
    public function __construct(
        public readonly string $defaultTargetPeerId,
        public readonly ConnectOptions $options,
    ) {}

    /**
     * Start the handle: ensure router, dial transport, register.
     */
    public function start(): void
    {
        if ($this->started) {
            return;
        }

        $port = $this->options->routerPort;
        if ($port <= 0) {
            $port = RouterBootstrap::routerPortFromEnv();
        }

        RouterBootstrap::ensureRouter($port);

        $peerId = $this->options->peerId;
        if ($peerId === null || $peerId === '') {
            $peerId = Protocol::newMsgId();
        }

        $endpoint = RouterBootstrap::routerEndpoint($port);

        $this->transport = PeerTransport::dial(
            endpoint: $endpoint,
            peerId: $peerId,
            class: PeerClass::Caller,
        );
        $this->peerId = $peerId;

        // Optional: validate the target peer exists and is a service
        if ($this->options->validateTarget && $this->defaultTargetPeerId !== '') {
            $details = $this->transport->queryGet($this->defaultTargetPeerId);

            if (!$details->exists || !$details->connected) {
                $this->transport->close();
                $this->transport = null;
                throw new RouterError(
                    'peer_not_found',
                    \sprintf('target peer "%s" not found', $this->defaultTargetPeerId),
                );
            }
            if ($details->class !== PeerClass::Service) {
                $this->transport->close();
                $this->transport = null;
                throw new RouterError(
                    'invalid_target_class',
                    \sprintf('target peer "%s" is not a service', $this->defaultTargetPeerId),
                );
            }
        }

        $this->started = true;
    }

    /**
     * Stop the handle: disconnect and close transport.
     */
    public function stop(): void
    {
        if (!$this->started || $this->transport === null) {
            return;
        }
        $this->started = false;
        $this->transport->disconnect();
        $this->transport->close();
        $this->transport = null;
    }

    /**
     * Issue a remote call by function name.
     *
     * @param list<mixed> $args
     */
    public function call(string $function, array $args = []): mixed
    {
        $this->requireTransport();
        return $this->transport->call($this->defaultTargetPeerId, $function, $args);
    }

    /**
     * Magic method for ergonomic calling: $handle->add(1, 2)
     *
     * @param list<mixed> $arguments
     */
    public function __call(string $name, array $arguments): mixed
    {
        return $this->call($name, $arguments);
    }

    /**
     * Check if a peer exists on the router.
     */
    public function queryPeerExists(string $peerId): bool
    {
        $this->requireTransport();
        return $this->transport->queryExists($peerId)->exists;
    }

    /**
     * Get full peer details from the router.
     */
    public function queryPeerGet(string $peerId): QueryGetResponseBody
    {
        $this->requireTransport();
        return $this->transport->queryGet($peerId);
    }

    public function getPeerId(): string
    {
        return $this->peerId;
    }

    public function isStarted(): bool
    {
        return $this->started;
    }

    private function requireTransport(): PeerTransport
    {
        if ($this->transport === null || !$this->started) {
            throw new TransportError('handle not started');
        }
        return $this->transport;
    }
}

// connect() function is defined in src/functions.php (loaded via composer "files" autoload)
