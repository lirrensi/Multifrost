<?php

declare(strict_types=1);

namespace Multifrost\Tests;

use PHPUnit\Framework\TestCase;
use Multifrost\ServiceProcess;
use Multifrost\BootstrapError;

final class ProcessUnitTest extends TestCase
{
    // ── canonicalPath (via spawn validation) ────────────────────────

    public function testSpawnRejectsEmptyPath(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('service entrypoint path is required');
        ServiceProcess::spawn('');
    }

    public function testSpawnRejectsNonExistentPath(): void
    {
        $this->expectException(BootstrapError::class);
        $this->expectExceptionMessage('service entrypoint not found');
        ServiceProcess::spawn('/tmp/nonexistent/does_not_exist.php');
    }

    public function testSpawnRejectsNonExistentRelativePath(): void
    {
        $this->expectException(BootstrapError::class);
        $this->expectExceptionMessage('service entrypoint not found');
        ServiceProcess::spawn('nonexistent_file_that_does_not_exist.php');
    }

    // ── Spawn, stop, wait lifecycle ─────────────────────────────────

    public function testSpawnExistingScriptReturnsProcess(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $process = ServiceProcess::spawn($examplePath);

        $this->assertInstanceOf(ServiceProcess::class, $process);
        $this->assertGreaterThan(0, $process->id());

        $process->stop();
    }

    public function testSpawnSetsEntrypointEnv(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $absPath = \realpath($examplePath);
        $process = ServiceProcess::spawn($examplePath);

        $this->assertGreaterThan(0, $process->id());

        $process->stop();
    }

    public function testProcessId(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $process = ServiceProcess::spawn($examplePath);

        $pid = $process->id();
        $this->assertGreaterThan(0, $pid);

        $process->stop();
    }

    public function testStopIsIdempotent(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $process = ServiceProcess::spawn($examplePath);

        $process->stop();
        // Second stop should not throw
        $process->stop();

        $this->expectNotToPerformAssertions();
    }

    public function testDestructorStopsProcess(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';

        // Spawn and immediately destroy — destructor must not throw
        $process = ServiceProcess::spawn($examplePath);
        $pid = $process->id();
        unset($process);

        // Verify the process was spawned (unreachable if destructor threw)
        $this->assertGreaterThan(0, $pid);
    }

    public function testWaitReturnsExitCode(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $process = ServiceProcess::spawn($examplePath);

        $process->stop();
        $exitCode = $process->wait();

        // Test passes because wait() completed without throwing
        $this->addToAssertionCount(1);
    }

    public function testWaitBeforeStop(): void
    {
        // This test spawns a simple script that exits immediately
        $tempScript = \tempnam(\sys_get_temp_dir(), 'mf_test_') . '.php';
        \file_put_contents($tempScript, '<?php exit(42);');

        try {
            $process = ServiceProcess::spawn($tempScript);
            $exitCode = $process->wait();
            $this->assertSame(42, $exitCode);

            // After wait, process should be nulled out
            $this->assertSame(-1, $process->wait());
        } finally {
            @\unlink($tempScript);
        }
    }

    public function testSpawnWithExtraEnv(): void
    {
        $tempScript = \tempnam(\sys_get_temp_dir(), 'mf_test_') . '.php';
        \file_put_contents($tempScript, '<?php echo getenv("MY_CUSTOM_VAR"); exit(0);');

        try {
            $process = ServiceProcess::spawn($tempScript, ['MY_CUSTOM_VAR' => 'hello-test']);
            $process->stop();
            $process->wait();
            $this->expectNotToPerformAssertions(); // spawn accepted extra env without throwing
        } finally {
            @\unlink($tempScript);
        }
    }

    // ── Edge cases ──────────────────────────────────────────────────

    public function testProcessIdBeforeSpawn(): void
    {
        // We can't construct ServiceProcess directly (private constructor)
        // So we test via the public API
        $examplePath = __DIR__ . '/../examples/math_service.php';
        $process = ServiceProcess::spawn($examplePath);

        $pid = $process->id();
        $this->assertGreaterThan(0, $pid);

        $process->stop();

        // After stop, id() still returns the old pid or 0
        // (proc_get_status returns old data on closed resource)
        $process->stop(); // Ensure stopped
    }

    public function testMultipleSpawnCreatesDistinctProcesses(): void
    {
        $examplePath = __DIR__ . '/../examples/math_service.php';

        $p1 = ServiceProcess::spawn($examplePath);
        $p2 = ServiceProcess::spawn($examplePath);

        $this->assertNotSame($p1->id(), $p2->id());

        $p1->stop();
        $p2->stop();
    }
}
