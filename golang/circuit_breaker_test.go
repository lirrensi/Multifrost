package multifrost

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestCircuitBreaker_Behavior tests the circuit breaker in action
// The circuit breaker should:
// 1. Start in closed state (allowing requests)
// 2. Open after MaxRestartAttempts consecutive failures
// 3. Reset on success

func TestCircuitBreaker_InitialState(t *testing.T) {
	t.Run("circuit breaker starts closed", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})

		// Initially circuit should be closed
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 0, worker.ConsecutiveFailures())
	})

	t.Run("circuit breaker respects custom threshold", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 10,
		})

		assert.Equal(t, 10, worker.MaxRestartAttempts)
	})
}

func TestCircuitBreaker_TripOnFailures(t *testing.T) {
	t.Run("circuit opens after threshold failures", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})

		// Record failures up to threshold
		worker.recordFailure()
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 1, worker.ConsecutiveFailures())

		worker.recordFailure()
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 2, worker.ConsecutiveFailures())

		// Third failure should trip the circuit
		worker.recordFailure()
		assert.True(t, worker.CircuitOpen())
		assert.Equal(t, 3, worker.ConsecutiveFailures())
	})

	t.Run("circuit stays open after more failures", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 2,
		})

		// Trip the circuit
		worker.recordFailure()
		worker.recordFailure()
		assert.True(t, worker.CircuitOpen())

		// More failures don't change state
		worker.recordFailure()
		worker.recordFailure()
		assert.True(t, worker.CircuitOpen())
		assert.Equal(t, 4, worker.ConsecutiveFailures())
	})
}

func TestCircuitBreaker_ResetOnSuccess(t *testing.T) {
	t.Run("success resets circuit breaker", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})

		// Record some failures (but not enough to trip)
		worker.recordFailure()
		worker.recordFailure()
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 2, worker.ConsecutiveFailures())

		// Success should reset
		worker.recordSuccess()
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 0, worker.ConsecutiveFailures())
	})

	t.Run("success resets open circuit", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 2,
		})

		// Trip the circuit
		worker.recordFailure()
		worker.recordFailure()
		assert.True(t, worker.CircuitOpen())

		// Success should close it again
		worker.recordSuccess()
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 0, worker.ConsecutiveFailures())
	})
}

func TestCircuitBreaker_WithMetrics(t *testing.T) {
	t.Run("circuit trip records metrics", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 2,
			EnableMetrics:      true,
		})

		// Trip the circuit
		worker.recordFailure()
		worker.recordFailure()

		// Check metrics recorded the trip
		metrics := worker.Metrics()
		require.NotNil(t, metrics)
		snapshot := metrics.Snapshot()
		assert.Equal(t, 1, snapshot.CircuitBreakerTrips)
		assert.Equal(t, "open", snapshot.CircuitBreakerState)
	})

	t.Run("circuit reset records metrics", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 2,
			EnableMetrics:      true,
		})

		// Trip and reset
		worker.recordFailure()
		worker.recordFailure()
		worker.recordSuccess()

		metrics := worker.Metrics()
		snapshot := metrics.Snapshot()
		assert.Equal(t, "closed", snapshot.CircuitBreakerState)
	})
}

func TestCircuitBreaker_IsHealthy(t *testing.T) {
	t.Run("healthy when running and circuit closed", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.running = true

		assert.True(t, worker.IsHealthy())
	})

	t.Run("unhealthy when circuit open", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 1,
		})
		worker.running = true
		worker.recordFailure()

		assert.False(t, worker.IsHealthy())
	})

	t.Run("unhealthy when not running", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		// running is false by default

		assert.False(t, worker.IsHealthy())
	})
}

func TestCircuitBreaker_ConcurrentAccess(t *testing.T) {
	t.Run("concurrent failure recording", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 100,
		})

		// Record failures concurrently
		done := make(chan bool, 10)
		for i := 0; i < 10; i++ {
			go func() {
				for j := 0; j < 10; j++ {
					worker.recordFailure()
				}
				done <- true
			}()
		}

		// Wait for all goroutines
		for i := 0; i < 10; i++ {
			<-done
		}

		// Should have 100 failures
		assert.Equal(t, 100, worker.ConsecutiveFailures())
	})

	t.Run("concurrent success and failure", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 50,
		})

		done := make(chan bool, 2)

		// One goroutine records failures
		go func() {
			for i := 0; i < 50; i++ {
				worker.recordFailure()
			}
			done <- true
		}()

		// One goroutine records successes
		go func() {
			for i := 0; i < 25; i++ {
				worker.recordSuccess()
			}
			done <- true
		}()

		// Wait for both
		<-done
		<-done

		// Final state should be consistent (no race detected)
		// The exact value depends on timing, but it should be >= 0
		assert.GreaterOrEqual(t, worker.ConsecutiveFailures(), 0)
	})
}

func TestCircuitBreaker_EdgeCases(t *testing.T) {
	t.Run("zero threshold never trips", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 0, // Will be set to default 5
		})

		// Default should kick in
		assert.Equal(t, 5, worker.MaxRestartAttempts)
	})

	t.Run("single failure threshold", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 1,
		})

		// Single failure should trip
		worker.recordFailure()
		assert.True(t, worker.CircuitOpen())
	})

	t.Run("rapid fire failures and successes", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 5,
		})

		// Rapid alternation
		for i := 0; i < 100; i++ {
			worker.recordFailure()
			worker.recordSuccess()
		}

		// Should never trip because successes reset
		assert.False(t, worker.CircuitOpen())
		assert.Equal(t, 0, worker.ConsecutiveFailures())
	})
}

func TestCircuitBreaker_ErrorTypes(t *testing.T) {
	t.Run("CircuitOpenError message", func(t *testing.T) {
		err := &CircuitOpenError{ConsecutiveFailures: 5}
		assert.Contains(t, err.Error(), "5")
		assert.Contains(t, err.Error(), "circuit breaker open")
	})

	t.Run("CircuitOpenError with wrapped error", func(t *testing.T) {
		innerErr := &RemoteCallError{Message: "connection refused"}
		err := &CircuitOpenError{
			ConsecutiveFailures: 3,
			Err:                 innerErr,
		}
		assert.Contains(t, err.Error(), "3")
		assert.Contains(t, err.Error(), "connection refused")
	})
}

func TestCircuitBreaker_TimeBased(t *testing.T) {
	t.Run("failures over time accumulate", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 3,
		})

		// Failure 1
		worker.recordFailure()
		time.Sleep(10 * time.Millisecond)

		// Failure 2
		worker.recordFailure()
		time.Sleep(10 * time.Millisecond)

		// Failure 3 - should trip
		worker.recordFailure()

		assert.True(t, worker.CircuitOpen())
	})
}
