<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Service worker interface — the user implements this to handle inbound calls.
 * Mirrors golang/service.go ServiceWorker interface.
 */
interface ServiceWorker
{
    /**
     * Handle an inbound call. Return the result or throw an exception.
     *
     * @param list<mixed> $args
     * @return mixed
     */
    public function handleCall(string $function, array $args): mixed;
}

/**
 * Service context — configuration for a service peer.
 */
final readonly class ServiceContext
{
    public function __construct(
        public ?string $peerId = null,
        public int $routerPort = 0,
        public ?string $entrypointPath = null,
    ) {}
}

// Public API functions (runService, spawn, connect) and internal helpers
// are defined in src/functions.php (loaded via composer "files" autoload)
