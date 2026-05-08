<?php

declare(strict_types=1);

namespace Multifrost;

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
        // Use a raw TCP socket check instead of phrity/websocket's lazy Client.
        // phrity/websocket defers the actual TCP connection until first I/O,
        // which means new Client() never throws and close() on an unopened
        // connection is silently ignored. A raw socket connect gives us the
        // real answer: can we reach the port at all?
        $host = '127.0.0.1';
        $port = 9981;

        // Parse port from ws://host:port format
        if (\preg_match('#^ws://[^:]+:(\d+)$#', $endpoint, $m)) {
            $port = (int) $m[1];
        }

        $sock = @\fsockopen($host, $port, $errno, $errstr, self::REACHABILITY_PROBE_SEC);
        if ($sock !== false) {
            \fclose($sock);
            return true;
        }

        return false;
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

    /**
     * Extract the router binary path from env or fall back to the default name.
     */
    private static function resolveRouterBinary(): string
    {
        $binary = \getenv(Protocol::ROUTER_BIN_ENV);
        if ($binary !== false && $binary !== '') {
            return $binary;
        }
        return 'multifrost-router';
    }

    /**
     * Create the log directory and return the log path.
     */
    private static function routerLogDir(): string
    {
        return self::homeDir() . \DIRECTORY_SEPARATOR . '.multifrost';
    }

    /**
     * Spawn the router process using the platform-appropriate method.
     * - Unix (Linux/macOS): proc_open (works reliably, captures logs).
     * - Windows: PowerShell Start-Process (avoids proc_open Winsock bug).
     */
    private static function spawnRouterProcess(int $port): void
    {
        $binary = self::resolveRouterBinary();

        if (\PHP_OS_FAMILY === 'Windows') {
            self::spawnRouterViaPowerShell($binary, $port);
        } else {
            self::spawnRouterViaProcOpen($binary, $port);
        }
    }

    /**
     * Spawn router via proc_open (Unix path).
     *
     * proc_open works correctly on Linux and macOS, where the child process
     * inherits a properly initialized Winsock-equivalent stack.
     */
    private static function spawnRouterViaProcOpen(string $binary, int $port): void
    {
        $logDir = self::routerLogDir();
        if (!\is_dir($logDir)) {
            @\mkdir($logDir, 0o755, true);
        }

        $logPath = $logDir . \DIRECTORY_SEPARATOR . 'router.log';
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
            \fclose($logFile);
            throw new BootstrapError("failed to start router process: {$binary}");
        }

        // Close stdin pipe immediately
        \fclose($pipes[0]);

        // Don't wait — router is a long-lived process
        // The poll loop in ensureRouter will confirm readiness
    }

    /**
     * Spawn router via PowerShell (Windows path).
     *
     * PHP's proc_open on some Windows builds creates child processes where
     * Winsock fails to initialise (error 10106: WSAEPROVIDERFAILEDINIT).
     * This causes the router (a Rust binary) to crash immediately when it
     * tries to bind a TCP socket.
     *
     * PowerShell's Start-Process delegates to .NET's Process.Start, which
     * properly initialises Winsock in the child process. We assume
     * PowerShell 5.1+ is present — this is true for all supported Windows
     * versions (10+, Server 2016+), including Server Core.
     *
     * @see https://github.com/Multifrost/multifrost/issues/... for the full
     *      investigation and test results.
     */
    private static function spawnRouterViaPowerShell(string $binary, int $port): void
    {
        $logDir = self::routerLogDir();
        if (!\is_dir($logDir)) {
            @\mkdir($logDir, 0o755, true);
        }

        // Set the port env var so the router child process inherits it
        \putenv(\sprintf('%s=%d', Protocol::ROUTER_PORT_ENV, $port));

        // PowerShell's Start-Process returns immediately (non-blocking).
        // -WindowStyle Hidden avoids flashing a console window.
        // Single-quoted path avoids cmd.exe variable expansion issues.
        $escapedBinary = \str_replace("'", "''", $binary);
        $cmd = \sprintf(
            'powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -WindowStyle Hidden \'%s\'"',
            $escapedBinary,
        );

        // exec runs through cmd.exe, which properly inherits putenv changes
        \exec($cmd, $_, $exitCode);

        if ($exitCode !== 0) {
            throw new BootstrapError(
                \sprintf(
                    'failed to start router process via PowerShell on Windows. ' .
                    'Start the router manually: set %s=%d && start /B "" "%s"',
                    Protocol::ROUTER_PORT_ENV,
                    $port,
                    $binary,
                ),
            );
        }
    }
}
