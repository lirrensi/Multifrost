package multifrost

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestParentWorker_SpawnMode(t *testing.T) {
	t.Run("basic spawn mode", func(t *testing.T) {
		// Spawn the worker
		worker := Spawn("path/to/test_worker", "go", "run")
		require.NotNil(t, worker)

		// Verify worker was created with correct configuration
		assert.True(t, worker.isSpawnMode)
		assert.NotEmpty(t, worker.scriptPath)
		assert.Greater(t, worker.port, 0)
	})
}

func TestParentWorker_ConnectMode(t *testing.T) {
	t.Run("basic connect mode", func(t *testing.T) {
		// Create worker in connect mode (without actual service)
		// This tests the configuration, not actual connection
		worker := NewParentWorker(ParentWorkerConfig{
			ServiceID: "test-service",
			Port:      5555,
		})
		require.NotNil(t, worker)

		// Verify worker was created with correct configuration
		assert.False(t, worker.isSpawnMode)
		assert.Equal(t, "test-service", worker.serviceID)
		assert.Equal(t, 5555, worker.port)
	})
}

func TestParentWorker_HealthChecks(t *testing.T) {
	t.Run("circuit breaker functionality", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})

		// Initially healthy
		assert.False(t, worker.CircuitOpen())
		assert.False(t, worker.IsHealthy()) // Not running yet

		// Simulate failures by directly manipulating internal state
		worker.consecutiveFailures = 3
		worker.circuitOpen = true
		assert.True(t, worker.CircuitOpen())
		assert.False(t, worker.IsHealthy())

		// Reset and verify
		worker.consecutiveFailures = 0
		worker.circuitOpen = false
		worker.running = true
		assert.False(t, worker.CircuitOpen())
		assert.True(t, worker.IsHealthy())
	})

	t.Run("heartbeat monitoring", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatInterval:  5 * time.Second,
			HeartbeatTimeout:   3 * time.Second,
			HeartbeatMaxMisses: 3,
		})

		// Initially no heartbeat
		assert.Equal(t, 0.0, worker.LastHeartbeatRttMs())

		// Simulate heartbeat
		worker.lastHeartbeatRttMs = 10.5
		assert.Equal(t, 10.5, worker.LastHeartbeatRttMs())
	})
}

func TestParentWorker_Metrics(t *testing.T) {
	t.Run("metrics collection", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		// Initially no metrics
		metrics := worker.Metrics()
		assert.NotNil(t, metrics)

		// Test metrics snapshot
		snapshot := metrics.Snapshot()
		assert.Equal(t, 0, snapshot.RequestsTotal)
		assert.Equal(t, 0, snapshot.RequestsSuccess)
		assert.Equal(t, 0, snapshot.RequestsFailed)
	})

	t.Run("metrics disabled", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: false,
		})

		// Metrics should be nil when disabled
		metrics := worker.Metrics()
		assert.Nil(t, metrics)
	})
}

func TestParentWorker_Configuration(t *testing.T) {
	t.Run("default timeout", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			DefaultTimeout: 30 * time.Second,
		})

		// Test default timeout
		assert.Equal(t, 30*time.Second, worker.DefaultTimeout)
	})

	t.Run("send retry logic", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 5,
		})

		// Test retry configuration
		assert.Equal(t, 5, worker.MaxRestartAttempts)
	})

	t.Run("default values", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		// Test default values
		assert.Equal(t, 5, worker.MaxRestartAttempts)
		assert.Equal(t, 5*time.Second, worker.HeartbeatInterval)
		assert.Equal(t, 3*time.Second, worker.HeartbeatTimeout)
		assert.Equal(t, 3, worker.HeartbeatMaxMisses)
	})
}

func TestParentWorker_ProxyInterfaces(t *testing.T) {
	t.Run("sync proxy exists", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		assert.NotNil(t, worker.Call)
	})

	t.Run("async proxy exists", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		assert.NotNil(t, worker.ACall)
	})
}

func TestParentWorker_StateManagement(t *testing.T) {
	t.Run("port getter", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			Port: 12345,
		})
		assert.Equal(t, 12345, worker.GetPort())
	})

	t.Run("running state", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		assert.False(t, worker.IsRunning())
	})
}
