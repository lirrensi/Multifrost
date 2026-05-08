<?php

declare(strict_types=1);

namespace Multifrost;

use WebSocket\Client;
use WebSocket\Configuration;

/**
 * Router bootstrap — discover, start, and wait for the shared v5 router.
 * Mirrors golang/router_bootstrap.go.
 */
final class RouterBootstrap
{
    private const BOOTSTRAP_TIMEOUT_SEC = 10;
    private const POLL_DELAY_USEC = 100_000; // 100ms
    private const REACHABILITY_PROBE_SEC = 1;
    private const LOCK_RETRY_DELAY_USEC = 100_000; // 100ms
    private const LOCK_MAX_AGE_SEC = 120; // 2 minutes

    /**
     * Ensure the router is running and reachable.
     * Returns the router endpoint URL on success.
     */
    public static function ensureRouter(int $port): string
    {
        $endpoint = self::routerEndpoint($port);

        if (self::routerReachable($endpoint)) {
            return $endpoint;
        }

        $lockPath = self::defaultLockPath();

        // Acquire lock
        $lockFd = self::acquireLock($lockPath);
        try {
            // Double-check after acquiring lock
            if (self::routerReachable($endpoint)) {
                return $endpoint;
            }

            // Spawn router
            self::spawnRouterProcess($port);

            // Wait for readiness
            $deadline = \microtime(true) + self::BOOTSTRAP_TIMEOUT_SEC;
            while (\microtime(true) < $deadline) {
                if (self::routerReachable($endpoint)) {
                    return $endpoint;
                }
                \usleep(self::POLL_DELAY_USEC);
            }

            throw new BootstrapError(
                \sprintf('router did not become reachable at %s within %d seconds', $endpoint, self::BOOTSTRAP_TIMEOUT_SEC),
            );
        } finally {
            self::releaseLock($lockFd, $lockPath);
        }
    }

    /**
     * Check if router is reachable by attempting a WebSocket dial.
     * @phpstan-impure
     */
    public static function routerReachable(string $endpoint): bool
    {
        try {
            $config = new Configuration(timeout: self::REACHABILITY_PROBE_SEC);
            $client = new Client($endpoint, $config);
            $client->close();
            return true;
        } catch (\Throwable) {
            return false;
        }
    }

    /**
     * Get router port from environment or default.
     */
    public static function routerPortFromEnv(): int
    {
        $value = \getenv(Protocol::ROUTER_PORT_ENV);
        if ($value !== false && $value !== '') {
            $port = (int) $value;
            if ($port > 0 && $port <= 65535) {
                return $port;
            }
        }
        return Protocol::DEFAULT_ROUTER_PORT;
    }

    /**
     * Get the router WebSocket endpoint URL.
     */
    public static function routerEndpoint(int $port): string
    {
        return \sprintf('ws://127.0.0.1:%d', $port);
    }

    // ── Private helpers ──────────────────────────────────────────

    private static function defaultLockPath(): string
    {
        $home = self::homeDir();
        return $home . \DIRECTORY_SEPARATOR . '.multifrost' . \DIRECTORY_SEPARATOR . 'router.lock';
    }

    private static function homeDir(): string
    {
        if (isset($_SERVER['HOME']) && $_SERVER['HOME'] !== '') {
            return $_SERVER['HOME'];
        }
        if (isset($_SERVER['USERPROFILE']) && $_SERVER['USERPROFILE'] !== '') {
            return $_SERVER['USERPROFILE'];
        }
        if (isset($_SERVER['HOMEDRIVE'], $_SERVER['HOMEPATH'])) {
            return $_SERVER['HOMEDRIVE'] . $_SERVER['HOMEPATH'];
        }
        return '.';
    }

    /**
     * Acquire a filesystem lock. Returns the file handle.
     * @return resource
     */
    private static function acquireLock(string $lockPath)
    {
        $lockDir = \dirname($lockPath);
        if (!\is_dir($lockDir)) {
            @\mkdir($lockDir, 0o755, true);
        }

        $deadline = \microtime(true) + self::BOOTSTRAP_TIMEOUT_SEC;

        while (\microtime(true) < $deadline) {
            // Check for stale lock
            if (\file_exists($lockPath) && self::isStaleLock($lockPath)) {
                @\unlink($lockPath);
            }

            $fd = @\fopen($lockPath, 'x');
            if ($fd !== false) {
                // Write PID to lock file
                \fwrite($fd, (string) \getmypid() . "\n");
                // Lock the file
                if (\flock($fd, \LOCK_EX | \LOCK_NB)) {
                    return $fd;
                }
                \fclose($fd);
            }

            \usleep(self::LOCK_RETRY_DELAY_USEC);
        }

        throw new BootstrapError('timed out waiting for router startup lock');
    }

    /**
     * @param resource|closed-resource $fd
     */
    private static function releaseLock($fd, string $lockPath): void
    {
        if (\is_resource($fd)) {
            \flock($fd, \LOCK_UN);
            \fclose($fd);
        }
        @\unlink($lockPath);
    }

    private static function isStaleLock(string $path): bool
    {
        if (!\file_exists($path)) {
            return false;
        }
        $mtime = @\filemtime($path);
        if ($mtime === false) {
            return true;
        }
        return (\time() - $mtime) > self::LOCK_MAX_AGE_SEC;
    }

    private static function spawnRouterProcess(int $port): void
    {
        $binary = \getenv(Protocol::ROUTER_BIN_ENV);
        if ($binary === false || $binary === '') {
            $binary = 'multifrost-router';
        }

        $logPath = self::homeDir() . \DIRECTORY_SEPARATOR . '.multifrost' . \DIRECTORY_SEPARATOR . 'router.log';
        $logDir = \dirname($logPath);
        if (!\is_dir($logDir)) {
            @\mkdir($logDir, 0o755, true);
        }

        $logFile = @\fopen($logPath, 'a');
        if ($logFile === false) {
            throw new BootstrapError("cannot open router log: {$logPath}");
        }

        $env = $_ENV;
        $env[Protocol::ROUTER_PORT_ENV] = (string) $port;

        $descriptorSpec = [
            0 => ['pipe', 'r'],  // stdin
            1 => $logFile,       // stdout -> log
            2 => $logFile,       // stderr -> log
        ];

        $process = @\proc_open($binary, $descriptorSpec, $pipes, null, $env);
        if (!\is_resource($process)) {
            throw new BootstrapError("failed to start router process: {$binary}");
        }

        // Close stdin pipe immediately
        \fclose($pipes[0]);

        // Don't wait — router is a long-lived process
        // The poll loop in ensureRouter will confirm readiness
    }
}
