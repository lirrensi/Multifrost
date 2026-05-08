<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Service process launcher — spawns a PHP service script.
 * Mirrors golang/process.go.
 *
 * Spawn only starts the process. The started process then connects to the router
 * and registers itself as a service peer. Spawn does not own network state.
 */
final class ServiceProcess
{
    /** @var resource|null */
    private $process = null;
    private bool $stopped = false;

    /**
     * @param resource $process proc_open resource
     */
    private function __construct($process)
    {
        $this->process = $process;
    }

    /**
     * Spawn a PHP service script as a separate process.
     *
     * @param string $entrypoint Absolute path to the PHP service script
     * @param array<string, string> $extraEnv Additional environment variables
     */
    public static function spawn(string $entrypoint, array $extraEnv = []): self
    {
        $absPath = self::canonicalPath($entrypoint);

        if (!\file_exists($absPath)) {
            throw new BootstrapError("service entrypoint not found: {$absPath}");
        }

        $env = $_ENV;
        $env[Protocol::ENTRYPOINT_PATH_ENV] = $absPath;
        foreach ($extraEnv as $key => $value) {
            $env[$key] = $value;
        }

        $phpBinary = \PHP_BINARY;

        $descriptorSpec = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ];

        $process = \proc_open(
            [$phpBinary, $absPath],
            $descriptorSpec,
            $pipes,
            null,
            $env,
        );

        if (!\is_resource($process)) {
            throw new BootstrapError("failed to spawn service process: {$absPath}");
        }

        // Close pipes we don't need
        \fclose($pipes[0]);
        \fclose($pipes[1]);
        \fclose($pipes[2]);

        return new self($process);
    }

    /**
     * Get the process PID.
     */
    public function id(): int
    {
        if ($this->process === null) {
            return 0;
        }
        $status = \proc_get_status($this->process);
        return $status['pid'];
    }

    /**
     * Stop the service process (SIGTERM on unix, terminate on windows).
     */
    public function stop(): void
    {
        if ($this->stopped || $this->process === null) {
            return;
        }
        $this->stopped = true;
        @\proc_terminate($this->process);
        \proc_close($this->process);
        $this->process = null;
    }

    /**
     * Wait for the process to exit and return its exit code.
     */
    public function wait(): int
    {
        if ($this->process === null) {
            return -1;
        }
        $status = \proc_get_status($this->process);
        while ($status['running']) {
            \usleep(50_000); // 50ms
            $status = \proc_get_status($this->process);
        }
        \proc_close($this->process);
        $exitCode = $status['exitcode'];
        $this->process = null;
        return $exitCode;
    }

    public function __destruct()
    {
        $this->stop();
    }

    private static function canonicalPath(string $path): string
    {
        if ($path === '') {
            throw new \InvalidArgumentException('service entrypoint path is required');
        }

        // Resolve relative to absolute
        $realPath = \realpath($path);
        if ($realPath !== false) {
            return $realPath;
        }

        // If realpath fails (file might not exist yet), try manual resolution
        if (\str_starts_with($path, '/') || (\PHP_OS_FAMILY === 'Windows' && \preg_match('#^[A-Z]:\\\\#i', $path))) {
            return $path;
        }

        return \getcwd() . \DIRECTORY_SEPARATOR . $path;
    }
}

// spawn() function is defined in src/functions.php (loaded via composer "files" autoload)
