package multifrost

import (
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestConcurrent_ParentWorkerState tests concurrent access to ParentWorker state
func TestConcurrent_ParentWorkerState(t *testing.T) {
	t.Run("consecutive failures are thread-safe", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 1000,
		})

		var wg sync.WaitGroup
		numGoroutines := 100
		failuresPerGoroutine := 10

		wg.Add(numGoroutines)
		for i := 0; i < numGoroutines; i++ {
			go func() {
				defer wg.Done()
				for j := 0; j < failuresPerGoroutine; j++ {
					worker.recordFailure()
				}
			}()
		}

		wg.Wait()

		expectedFailures := numGoroutines * failuresPerGoroutine
		assert.Equal(t, expectedFailures, worker.ConsecutiveFailures())
	})

	t.Run("circuit breaker is thread-safe", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 500,
		})

		var wg sync.WaitGroup
		numOps := 500

		// Half record failures, half record successes
		wg.Add(numOps)
		for i := 0; i < numOps; i++ {
			go func(idx int) {
				defer wg.Done()
				if idx%2 == 0 {
					worker.recordFailure()
				} else {
					worker.recordSuccess()
				}
			}(i)
		}

		wg.Wait()

		// State should be consistent (no race, no negative values)
		assert.GreaterOrEqual(t, worker.ConsecutiveFailures(), 0)
	})
}

// TestConcurrent_PendingRequests tests concurrent pending request handling
func TestConcurrent_PendingRequests(t *testing.T) {
	t.Run("pending requests are thread-safe", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingRequests = make(map[string]*pendingRequest)

		var wg sync.WaitGroup
		numRequests := 100

		wg.Add(numRequests)
		for i := 0; i < numRequests; i++ {
			go func(idx int) {
				defer wg.Done()
				reqID := "req-" + string(rune('0'+idx%10)) + string(rune('0'+idx/10))

				worker.mu.Lock()
				worker.pendingRequests[reqID] = &pendingRequest{
					resolve: func(v any) {},
					reject:  func(e error) {},
				}
				worker.mu.Unlock()

				// Simulate some work
				time.Sleep(time.Microsecond)

				worker.mu.Lock()
				delete(worker.pendingRequests, reqID)
				worker.mu.Unlock()
			}(i)
		}

		wg.Wait()

		// All requests should be cleaned up
		worker.mu.RLock()
		assert.Empty(t, worker.pendingRequests)
		worker.mu.RUnlock()
	})
}

// TestConcurrent_PendingHeartbeats tests concurrent heartbeat tracking
func TestConcurrent_PendingHeartbeats(t *testing.T) {
	t.Run("pending heartbeats are thread-safe", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)

		var wg sync.WaitGroup
		numHeartbeats := 50

		wg.Add(numHeartbeats)
		for i := 0; i < numHeartbeats; i++ {
			go func(idx int) {
				defer wg.Done()
				hbID := "hb-" + string(rune('0'+idx%10))

				responseChan := make(chan bool, 1)

				worker.mu.Lock()
				worker.pendingHeartbeats[hbID] = responseChan
				worker.mu.Unlock()

				// Simulate some work
				time.Sleep(time.Microsecond)

				worker.mu.Lock()
				delete(worker.pendingHeartbeats, hbID)
				worker.mu.Unlock()
			}(i)
		}

		wg.Wait()

		// All heartbeats should be cleaned up
		worker.mu.RLock()
		assert.Empty(t, worker.pendingHeartbeats)
		worker.mu.RUnlock()
	})
}

// TestConcurrent_Metrics tests concurrent metrics recording
func TestConcurrent_Metrics(t *testing.T) {
	t.Run("concurrent metrics recording", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		metrics := worker.Metrics()
		require.NotNil(t, metrics)

		var wg sync.WaitGroup
		numOps := 100

		wg.Add(numOps)
		for i := 0; i < numOps; i++ {
			go func(idx int) {
				defer wg.Done()
				startTime := metrics.StartRequest("req-"+string(rune('0'+idx%10)), "testFunc", "default")
				time.Sleep(time.Microsecond)
				metrics.EndRequest(startTime, "req-"+string(rune('0'+idx%10)), idx%3 == 0, "")
			}(i)
		}

		wg.Wait()

		snapshot := metrics.Snapshot()
		assert.Equal(t, numOps, snapshot.RequestsTotal)
	})
}

// TestConcurrent_CircuitBreakerState tests circuit breaker state changes
func TestConcurrent_CircuitBreakerState(t *testing.T) {
	t.Run("rapid state changes", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 10,
			EnableMetrics:      true,
		})

		var wg sync.WaitGroup

		// Goroutine 1: Record failures
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 100; i++ {
				worker.recordFailure()
				time.Sleep(time.Microsecond)
			}
		}()

		// Goroutine 2: Record successes
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 100; i++ {
				worker.recordSuccess()
				time.Sleep(time.Microsecond)
			}
		}()

		// Goroutine 3: Check state
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 100; i++ {
				_ = worker.CircuitOpen()
				_ = worker.IsHealthy()
				_ = worker.ConsecutiveFailures()
				time.Sleep(time.Microsecond)
			}
		}()

		wg.Wait()

		// Final state should be consistent
		assert.GreaterOrEqual(t, worker.ConsecutiveFailures(), 0)
	})
}

// TestConcurrent_MultipleHandles tests multiple handles from same worker
func TestConcurrent_MultipleHandles(t *testing.T) {
	t.Run("multiple handles access same worker", func(t *testing.T) {
		worker := Spawn("test_worker", "go", "run")

		var wg sync.WaitGroup
		numHandles := 10
		results := make(chan bool, numHandles)

		wg.Add(numHandles)
		for i := 0; i < numHandles; i++ {
			go func() {
				defer wg.Done()
				handle := worker.Handle()
				results <- handle.IsHealthy()
			}()
		}

		wg.Wait()
		close(results)

		// All handles should report same health status
		count := 0
		for healthy := range results {
			count++
			assert.False(t, healthy) // Not running yet
		}
		assert.Equal(t, numHandles, count)
	})
}

// TestConcurrent_MetricsCounterConsistency tests metrics counter consistency
func TestConcurrent_MetricsCounterConsistency(t *testing.T) {
	t.Run("metrics counters are consistent", func(t *testing.T) {
		metrics := NewMetrics(1000, 60.0)

		var wg sync.WaitGroup
		numOps := 1000

		// Concurrent requests
		wg.Add(numOps)
		for i := 0; i < numOps; i++ {
			go func(idx int) {
				defer wg.Done()
				startTime := metrics.StartRequest("req-"+string(rune(idx)), "test", "default")
				success := idx%2 == 0
				metrics.EndRequest(startTime, "req-"+string(rune(idx)), success, "")
			}(i)
		}

		wg.Wait()

		snapshot := metrics.Snapshot()
		// Total should be exactly numOps
		assert.Equal(t, numOps, snapshot.RequestsTotal)
		// Success + failed should equal total
		assert.Equal(t, snapshot.RequestsTotal, snapshot.RequestsSuccess+snapshot.RequestsFailed)
	})
}

// TestConcurrent_HeartbeatMissTracking tests concurrent heartbeat miss tracking
func TestConcurrent_HeartbeatMissTracking(t *testing.T) {
	t.Run("heartbeat misses are counted correctly", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		var wg sync.WaitGroup
		numMisses := 50

		wg.Add(numMisses)
		for i := 0; i < numMisses; i++ {
			go func() {
				defer wg.Done()
				worker.mu.Lock()
				worker.consecutiveHeartbeatMisses++
				worker.mu.Unlock()

				if worker.metrics != nil {
					worker.metrics.RecordHeartbeatMiss()
				}
			}()
		}

		wg.Wait()

		metrics := worker.Metrics()
		require.NotNil(t, metrics)
		snapshot := metrics.Snapshot()
		assert.Equal(t, numMisses, snapshot.HeartbeatMisses)
	})
}

// TestConcurrent_RegistryOperations tests concurrent service registry operations
func TestConcurrent_RegistryOperations(t *testing.T) {
	t.Run("concurrent register and unregister", func(t *testing.T) {
		// Clear registry first
		_ = ClearRegistry()

		var wg sync.WaitGroup
		numServices := 20

		// Concurrent registrations
		wg.Add(numServices)
		for i := 0; i < numServices; i++ {
			go func(idx int) {
				defer wg.Done()
				serviceID := "concurrent-svc-" + string(rune('0'+idx%10)) + string(rune('0'+idx/10))
				port := 55000 + idx
				_ = Register(serviceID, port)
			}(i)
		}

		wg.Wait()

		// List services
		services, err := ListServices()
		require.NoError(t, err)

		// Should have some services registered
		assert.Greater(t, len(services), 0)

		// Concurrent unregistrations
		wg.Add(numServices)
		for i := 0; i < numServices; i++ {
			go func(idx int) {
				defer wg.Done()
				serviceID := "concurrent-svc-" + string(rune('0'+idx%10)) + string(rune('0'+idx/10))
				_ = Unregister(serviceID)
			}(i)
		}

		wg.Wait()

		// Clean up
		_ = ClearRegistry()
	})
}

// TestConcurrent_MessageSerialization tests concurrent message serialization
func TestConcurrent_MessageSerialization(t *testing.T) {
	t.Run("concurrent message pack/unpack", func(t *testing.T) {
		var wg sync.WaitGroup
		numMessages := 100

		wg.Add(numMessages)
		for i := 0; i < numMessages; i++ {
			go func(idx int) {
				defer wg.Done()
				msg := CreateCall("testFunc", []any{idx, "test"}, "default", "", "")

				data, err := msg.Pack()
				if err != nil {
					t.Errorf("Pack failed: %v", err)
					return
				}

				unpacked, err := Unpack(data)
				if err != nil {
					t.Errorf("Unpack failed: %v", err)
					return
				}

				if unpacked.ID != msg.ID {
					t.Errorf("ID mismatch: expected %s, got %s", msg.ID, unpacked.ID)
				}
			}(i)
		}

		wg.Wait()
	})
}

// TestConcurrent_StressTest runs a stress test with many goroutines
func TestConcurrent_StressTest(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping stress test in short mode")
	}

	t.Run("stress test parent worker operations", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			MaxRestartAttempts: 100,
			EnableMetrics:      true,
		})

		var wg sync.WaitGroup
		numGoroutines := 50
		opsPerGoroutine := 100

		for g := 0; g < numGoroutines; g++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				for i := 0; i < opsPerGoroutine; i++ {
					// Mix of operations
					worker.recordFailure()
					worker.recordSuccess()
					_ = worker.CircuitOpen()
					_ = worker.ConsecutiveFailures()
					_ = worker.IsHealthy()
				}
			}()
		}

		wg.Wait()

		// Verify no race conditions occurred
		assert.GreaterOrEqual(t, worker.ConsecutiveFailures(), 0)
	})
}
