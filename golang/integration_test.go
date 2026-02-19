package multifrost

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestIntegration_MessageRoundTrip tests message serialization round-trip
// without actual process spawning (for CI stability)
func TestIntegration_MessageRoundTrip(t *testing.T) {
	t.Run("call message round-trip", func(t *testing.T) {
		original := CreateCall("testFunc", []any{1, "hello", 3.14}, "default", "msg-123", "client-1")

		data, err := original.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
		assert.Equal(t, original.Function, unpacked.Function)
		assert.Equal(t, original.Namespace, unpacked.Namespace)
		assert.Equal(t, original.ClientName, unpacked.ClientName)
	})

	t.Run("response message round-trip", func(t *testing.T) {
		original := CreateResponse(map[string]int{"result": 42}, "msg-456")

		data, err := original.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
	})

	t.Run("error message round-trip", func(t *testing.T) {
		original := CreateError("something went wrong", "msg-789")

		data, err := original.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
		assert.Equal(t, original.Error, unpacked.Error)
	})
}

// TestIntegration_HandleLifecycle tests Handle lifecycle
func TestIntegration_HandleLifecycle(t *testing.T) {
	t.Run("handle start and stop", func(t *testing.T) {
		worker := Spawn("dummy_worker", "go", "run")
		handle := worker.Handle()

		require.NotNil(t, handle)
		assert.False(t, handle.IsHealthy()) // Not started yet
	})

	t.Run("handle delegates call", func(t *testing.T) {
		worker := Spawn("dummy_worker", "go", "run")
		handle := worker.Handle()

		// Call should fail because worker is not running
		_, err := handle.Call("testFunc", 1, 2)
		assert.Error(t, err)
	})

	t.Run("handle with timeout", func(t *testing.T) {
		worker := Spawn("dummy_worker", "go", "run")
		handle := worker.Handle()

		// CallWithTimeout should fail because worker is not running
		_, err := handle.CallWithTimeout("testFunc", 1*time.Second, 1, 2)
		assert.Error(t, err)
	})

	t.Run("handle with context", func(t *testing.T) {
		worker := Spawn("dummy_worker", "go", "run")
		handle := worker.Handle()

		ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
		defer cancel()

		// CallWithContext should fail because worker is not running
		_, err := handle.CallWithContext(ctx, "testFunc", 1, 2)
		assert.Error(t, err)
	})
}

// TestIntegration_ProxyInterfaces tests proxy interfaces
func TestIntegration_ProxyInterfaces(t *testing.T) {
	t.Run("sync proxy exists", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		require.NotNil(t, worker.Call)
		require.NotNil(t, worker.Call.CallWithTimeout)
	})

	t.Run("async proxy exists", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		require.NotNil(t, worker.ACall)
	})

	t.Run("sync proxy call fails on non-running worker", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		_, err := worker.Call.Call("testFunc", 1, 2)
		assert.Error(t, err)
	})

	t.Run("async proxy call fails on non-running worker", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		_, err := worker.ACall.Call(context.Background(), "testFunc", 1, 2)
		assert.Error(t, err)
	})
}

// TestIntegration_Configuration tests configuration options
func TestIntegration_Configuration(t *testing.T) {
	t.Run("spawn mode configuration", func(t *testing.T) {
		worker := Spawn("test_worker.go", "python3")

		assert.True(t, worker.IsSpawnMode())
		assert.Equal(t, "test_worker.go", worker.ScriptPath())
		assert.Equal(t, "python3", worker.executable)
		assert.Greater(t, worker.GetPort(), 0)
	})

	t.Run("connect mode configuration", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			ServiceID: "test-service",
			Port:      5555,
		})

		assert.False(t, worker.IsSpawnMode())
		assert.Equal(t, "test-service", worker.ServiceID())
		assert.Equal(t, 5555, worker.GetPort())
	})

	t.Run("auto restart configuration", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			AutoRestart:        true,
			MaxRestartAttempts: 10,
		})

		assert.True(t, worker.AutoRestart)
		assert.Equal(t, 10, worker.MaxRestartAttempts)
	})

	t.Run("timeout configuration", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			DefaultTimeout: 30 * time.Second,
		})

		assert.Equal(t, 30*time.Second, worker.DefaultTimeout)
	})
}

// TestIntegration_MetricsCollection tests metrics collection
func TestIntegration_MetricsCollection(t *testing.T) {
	t.Run("metrics enabled", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		metrics := worker.Metrics()
		require.NotNil(t, metrics)

		snapshot := metrics.Snapshot()
		assert.Equal(t, 0, snapshot.RequestsTotal)
	})

	t.Run("metrics disabled", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: false,
		})

		metrics := worker.Metrics()
		assert.Nil(t, metrics)
	})

	t.Run("handle returns metrics", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})
		handle := worker.Handle()

		metrics := handle.Metrics()
		require.NotNil(t, metrics)
	})
}

// TestIntegration_CircuitBreakerIntegration tests circuit breaker with call failures
func TestIntegration_CircuitBreakerIntegration(t *testing.T) {
	t.Run("circuit opens on call failures", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
			EnableMetrics:      true,
		})

		// Worker is not running, calls will fail
		assert.False(t, worker.CircuitOpen())

		// Simulate failures
		worker.recordFailure()
		worker.recordFailure()
		worker.recordFailure()

		assert.True(t, worker.CircuitOpen())
	})

	t.Run("circuit error is returned on open circuit", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 1,
		})

		// Open the circuit
		worker.recordFailure()

		// Call should return CircuitOpenError
		_, err := worker.CallFunction(context.Background(), "testFunc", 1, 2)
		require.Error(t, err)

		// Should be CircuitOpenError
		assert.Contains(t, err.Error(), "circuit breaker open")
	})
}

// TestIntegration_ServiceRegistry tests service registry integration
func TestIntegration_ServiceRegistry(t *testing.T) {
	t.Run("service registration and discovery", func(t *testing.T) {
		// Clear registry
		_ = ClearRegistry()

		serviceID := "test-integration-" + time.Now().Format("20060102150405")
		port := 55001

		err := Register(serviceID, port)
		require.NoError(t, err)

		discoveredPort, err := Discover(serviceID, 2*time.Second)
		require.NoError(t, err)
		assert.Equal(t, port, discoveredPort)

		// Cleanup
		_ = Unregister(serviceID)
	})

	t.Run("connect mode discovery failure", func(t *testing.T) {
		// Try to connect to non-existent service
		_, err := Connect(context.Background(), "non-existent-service", 500*time.Millisecond)
		assert.Error(t, err)
	})
}

// TestIntegration_ErrorHandling tests error handling scenarios
func TestIntegration_ErrorHandling(t *testing.T) {
	t.Run("call on non-running worker", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		_, err := worker.CallFunction(context.Background(), "testFunc", 1, 2)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "not running")
	})

	t.Run("call with cancelled context fails early", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		ctx, cancel := context.WithCancel(context.Background())
		cancel() // Cancel immediately

		// Call should fail because worker is not running
		// (The "not running" check happens before the cancelled context check)
		_, err := worker.CallFunction(ctx, "testFunc", 1, 2)
		assert.Error(t, err)
	})

	t.Run("call returns circuit open error", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 1,
		})

		// Open the circuit
		worker.recordFailure()

		// Should return CircuitOpenError
		_, err := worker.CallFunction(context.Background(), "testFunc", 1, 2)
		require.Error(t, err)
		assert.Contains(t, err.Error(), "circuit breaker open")
	})
}

// TestIntegration_PortAllocation tests port allocation
func TestIntegration_PortAllocation(t *testing.T) {
	t.Run("spawn allocates free port", func(t *testing.T) {
		worker1 := Spawn("worker1", "go", "run")
		worker2 := Spawn("worker2", "go", "run")

		// Each worker should get a different port
		assert.NotEqual(t, worker1.GetPort(), worker2.GetPort())

		// Ports should be in valid range
		assert.Greater(t, worker1.GetPort(), 1024)
		assert.Less(t, worker1.GetPort(), 65536)
		assert.Greater(t, worker2.GetPort(), 1024)
		assert.Less(t, worker2.GetPort(), 65536)
	})

	t.Run("connect mode uses specified port", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			Port: 12345,
		})

		assert.Equal(t, 12345, worker.GetPort())
	})
}

// TestIntegration_EdgeCases tests edge cases
func TestIntegration_EdgeCases(t *testing.T) {
	t.Run("empty args", func(t *testing.T) {
		msg := CreateCall("noArgsFunc", nil, "default", "msg-1", "client-1")
		data, err := msg.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)
		assert.Nil(t, unpacked.Args)
	})

	t.Run("large args", func(t *testing.T) {
		largeArgs := make([]any, 1000)
		for i := range largeArgs {
			largeArgs[i] = i
		}

		msg := CreateCall("largeArgsFunc", largeArgs, "default", "msg-1", "client-1")
		data, err := msg.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)
		assert.Len(t, unpacked.Args, 1000)
	})

	t.Run("unicode in function name", func(t *testing.T) {
		msg := CreateCall("тестFunc", []any{1, 2}, "default", "msg-1", "client-1")
		data, err := msg.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)
		assert.Equal(t, "тестFunc", unpacked.Function)
	})
}

// TestIntegration_Close tests worker close behavior
func TestIntegration_Close(t *testing.T) {
	t.Run("close idempotent", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		err := worker.Close()
		require.NoError(t, err)

		// Second close should be safe
		err = worker.Close()
		require.NoError(t, err)
	})

	t.Run("close cleans up state", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})
		worker.running = true

		err := worker.Close()
		require.NoError(t, err)

		assert.False(t, worker.running)
	})
}

// TestIntegration_WorkerFilePath tests worker file path handling
func TestIntegration_WorkerFilePath(t *testing.T) {
	t.Run("relative path", func(t *testing.T) {
		worker := Spawn("./workers/test_worker.py", "python3")
		assert.Equal(t, "./workers/test_worker.py", worker.ScriptPath())
	})

	t.Run("absolute path", func(t *testing.T) {
		absPath := filepath.Join(os.TempDir(), "test_worker.py")
		worker := Spawn(absPath, "python3")
		assert.Equal(t, absPath, worker.ScriptPath())
	})
}

// TestIntegration_MultipleWorkers tests multiple workers
func TestIntegration_MultipleWorkers(t *testing.T) {
	t.Run("multiple workers have independent state", func(t *testing.T) {
		worker1 := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})
		worker2 := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 5,
		})

		worker1.recordFailure()
		worker1.recordFailure()

		assert.Equal(t, 2, worker1.ConsecutiveFailures())
		assert.Equal(t, 0, worker2.ConsecutiveFailures())
		assert.Equal(t, 3, worker1.MaxRestartAttempts)
		assert.Equal(t, 5, worker2.MaxRestartAttempts)
	})
}
