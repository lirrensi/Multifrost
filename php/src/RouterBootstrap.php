<?php

declare(strict_types=1);

/**
 * FILE: php/src/RouterBootstrap.php
 * PURPOSE: Router discovery, startup, and lock-file bootstrap for the shared v5 router.
 * OWNS: Process liveness checks, lock-file JSON format, router spawning per platform.
 * EXPORTS: RouterBootstrap (final) — ensureRouter(), routerReachable(), routerPortFromEnv(),
 *          routerEndpoint(), evaluateExistingLock(), isProcessAlive()
 * DOCS: docs/spec.md (Lock File Format section), agent_chat/plan_json_lock_file_2026-05-08.md
 */

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
    private const LOCK_FORMAT = 'v1';

    /**
     * Ensure the router is running and reachable.
     * Returns the router endpoint URL on success.
     */
    public static function ensureRouter(int $port): string
    {
        $endpoint = self::routerEndpoint($port);

        // 1. Quick reachability check
        if (self::routerReachable($endpoint)) {
            return $endpoint;
        }

        $lockPath = self::defaultLockPath();

        // 2. Acquire exclusive startup lock (or get skip_spawn signal)
        $lockGuard = self::acquireLock($lockPath, $port);

        if ($lockGuard === null) {
            // Router is already running per evaluateExistingLock → poll for readiness
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
        }

        $succeeded = false;
        try {
            // 3. Double-check reachability after acquiring lock
            if (self::routerReachable($endpoint)) {
                $succeeded = true;
                return $endpoint;
            }

            // 4. Spawn the router process
            $routerPid = self::spawnRouterProcess($port);
            self::updateLock($lockGuard, ['router_pid' => $routerPid]);

            // 5. Poll for readiness
            $deadline = \microtime(true) + self::BOOTSTRAP_TIMEOUT_SEC;
            while (\microtime(true) < $deadline) {
                if (self::routerReachable($endpoint)) {
                    self::updateLock($lockGuard, ['status' => 'ready']);
                    $succeeded = true;
                    return $endpoint;
                }
                \usleep(self::POLL_DELAY_USEC);
            }

            // Timeout
            self::updateLock($lockGuard, ['status' => 'failed']);
            throw new BootstrapError(
                \sprintf('router did not become reachable at %s within %d seconds', $endpoint, self::BOOTSTRAP_TIMEOUT_SEC),
            );
        } finally {
            if (!$succeeded) {
                self::updateLock($lockGuard, ['status' => 'failed']);
            }
            self::releaseLock($lockGuard);
        }
    }

    /**
     * Check if router is reachable by attempting a WebSocket dial.
     * @phpstan-impure
     */
    public static function routerReachable(string $endpoint): bool
    {
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

    /**
     * Evaluate an existing lock file and determine the action to take.
     *
     * Evaluation rules (in order):
     * 1. Empty/unreadable/unparseable content → reclaim
     * 2. format !== "v1" → reclaim (legacy format)
     * 3. expires_at_unix < now → reclaim (self-declared dead)
     * 4. holder PID not alive → reclaim (dead holder)
     * 5. status === "failed" → reclaim
     * 6. router_pid set and alive → skip_spawn (router already running)
     * 7. Otherwise → wait (valid lock, retry later)
     *
     * @param string $path Path to the lock file
     * @param int    $port Target router port (for diagnostic logging)
     * @return string "reclaim", "skip_spawn", or "wait"
     */
    public static function evaluateExistingLock(string $path, int $port): string
    {
        $content = @\file_get_contents($path);
        if ($content === false || $content === '') {
            return 'reclaim';
        }

        $data = @\json_decode($content, true);
        if (!\is_array($data)) {
            return 'reclaim';
        }

        // 2. Check format version
        if (!isset($data['format']) || $data['format'] !== self::LOCK_FORMAT) {
            return 'reclaim';
        }

        // 3. Check expiry
        if (isset($data['expires_at_unix']) && $data['expires_at_unix'] < \microtime(true)) {
            return 'reclaim';
        }

        // 4. Check holder process liveness
        if (isset($data['pid']) && \is_int($data['pid'])) {
            if (!self::isProcessAlive($data['pid'])) {
                return 'reclaim';
            }
        } else {
            return 'reclaim';
        }

        // 5. Check failed status
        if (isset($data['status']) && $data['status'] === 'failed') {
            return 'reclaim';
        }

        // 6. Check if router is already running
        if (isset($data['router_pid']) && \is_int($data['router_pid']) && $data['router_pid'] > 0) {
            if (self::isProcessAlive($data['router_pid'])) {
                return 'skip_spawn';
            }
        }

        // 7. Valid lock, wait and retry
        return 'wait';
    }

    /**
     * Check whether a process is alive by its PID.
     *
     * - Windows: uses `tasklist` to query process existence
     * - Unix: uses `kill -0` which sends a null signal (no-op, checks existence)
     */
    public static function isProcessAlive(int $pid): bool
    {
        if ($pid <= 0) {
            return false;
        }

        if (\PHP_OS_FAMILY === 'Windows') {
            $cmd = \sprintf('tasklist /FI "PID eq %d" /FO csv /NH 2>NUL', $pid);
            @\exec($cmd, $output, $exitCode);
            // tasklist outputs a CSV line when PID is found;
            // verify the output actually contains the PID string
            foreach ((array) $output as $line) {
                if (\strpos($line, (string) $pid) !== false) {
                    return true;
                }
            }
            return false;
        }

        // Unix: kill -0 (signal 0 = check existence, no signal sent)
        $escapedPid = \escapeshellarg((string) $pid);
        @\exec("kill -0 {$escapedPid} 2>/dev/null", $_, $exitCode);
        return $exitCode === 0;
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
     * Acquire an exclusive startup lock using atomic file creation (fopen 'x').
     *
     * On success returns a guard array. On "skip_spawn" (router already running
     * under a valid lock held by another process) returns null.
     *
     * @param string $lockPath
     * @param int    $port
     * @return array{path: string, port: int}|null
     */
    private static function acquireLock(string $lockPath, int $port): ?array
    {
        $lockDir = \dirname($lockPath);
        if (!\is_dir($lockDir)) {
            @\mkdir($lockDir, 0o755, true);
        }

        $deadline = \microtime(true) + self::BOOTSTRAP_TIMEOUT_SEC;

        while (\microtime(true) < $deadline) {
            // Atomic exclusive-create — mutual exclusion primitive
            $fd = @\fopen($lockPath, 'x');
            if ($fd !== false) {
                $now = \microtime(true);
                $data = [
                    'format' => self::LOCK_FORMAT,
                    'pid' => \getmypid(),
                    'router_pid' => null,
                    'port' => $port,
                    'created_at_unix' => $now,
                    'expires_at_unix' => $now + self::BOOTSTRAP_TIMEOUT_SEC,
                    'status' => 'starting',
                    'language' => 'php',
                ];
                \fwrite($fd, \json_encode($data, \JSON_UNESCAPED_SLASHES));
                \fclose($fd);
                return ['path' => $lockPath, 'port' => $port];
            }

            // File exists — evaluate it
            $decision = self::evaluateExistingLock($lockPath, $port);
            if ($decision === 'reclaim') {
                @\unlink($lockPath);
                \usleep(self::LOCK_RETRY_DELAY_USEC);
                continue;
            }
            if ($decision === 'skip_spawn') {
                // Router is already running under a valid lock
                return null;
            }
            // "wait" — lock is valid, retry after delay
            \usleep(self::LOCK_RETRY_DELAY_USEC);
        }

        throw new BootstrapError('timed out waiting for router startup lock');
    }

    /**
     * Release the startup lock by deleting the lock file.
     *
     * @param array{path: string, port: int}|null $guard
     */
    private static function releaseLock(?array $guard): void
    {
        if ($guard === null) {
            return;
        }
        @\unlink($guard['path']);
    }

    /**
     * Update fields in the lock JSON file (best-effort, suppresses errors).
     *
     * Reads the existing JSON, merges in $fields, writes back.
     *
     * @param array{path: string, port: int} $guard
     * @param array<string, mixed>           $fields Key-value pairs to merge
     */
    private static function updateLock(array $guard, array $fields): void
    {
        $path = $guard['path'];
        $content = @\file_get_contents($path);
        if ($content === false) {
            return;
        }
        $data = @\json_decode($content, true);
        if (!\is_array($data)) {
            return;
        }
        foreach ($fields as $key => $value) {
            $data[$key] = $value;
        }
        $encoded = @\json_encode($data, \JSON_UNESCAPED_SLASHES);
        if ($encoded === false) {
            return;
        }
        @\file_put_contents($path, $encoded);
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
     *
     * @return int Router PID
     */
    private static function spawnRouterProcess(int $port): int
    {
        $binary = self::resolveRouterBinary();

        if (\PHP_OS_FAMILY === 'Windows') {
            return self::spawnRouterViaPowerShell($binary, $port);
        }

        return self::spawnRouterViaProcOpen($binary, $port);
    }

    /**
     * Spawn router via proc_open (Unix path).
     *
     * proc_open works correctly on Linux and macOS, where the child process
     * inherits a properly initialized Winsock-equivalent stack.
     *
     * @return int Router PID
     */
    private static function spawnRouterViaProcOpen(string $binary, int $port): int
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

        // Capture PID from proc_get_status (safe to call once)
        $status = @\proc_get_status($process);
        $pid = $status['pid'];

        // Don't wait — router is a long-lived process
        // The poll loop in ensureRouter will confirm readiness
        return $pid;
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
     *
     * @return int Router PID
     */
    private static function spawnRouterViaPowerShell(string $binary, int $port): int
    {
        $logDir = self::routerLogDir();
        if (!\is_dir($logDir)) {
            @\mkdir($logDir, 0o755, true);
        }

        // Set the port env var so the router child process inherits it
        \putenv(\sprintf('%s=%d', Protocol::ROUTER_PORT_ENV, $port));

        // PowerShell's Start-Process returns immediately (non-blocking).
        // -WindowStyle Hidden avoids flashing a console window.
        // -PassThru returns the process object; $p.Id gives us the PID.
        // Single-quoted path avoids cmd.exe variable expansion issues.
        $escapedBinary = \str_replace("'", "''", $binary);
        $cmd = \sprintf(
            'powershell -NoProfile -ExecutionPolicy Bypass -Command "$p = Start-Process -WindowStyle Hidden -PassThru \'%s\'; $p.Id"',
            $escapedBinary,
        );

        // exec runs through cmd.exe, which properly inherits putenv changes
        \exec($cmd, $output, $exitCode);

        if ($exitCode !== 0 || empty($output)) {
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

        return (int) \trim($output[0]);
    }
}
