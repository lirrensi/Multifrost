package multifrost

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestMetrics_Creation(t *testing.T) {
	t.Run("default creation", func(t *testing.T) {
		m := NewMetrics(0, 0)
		assert.NotNil(t, m)
	})

	t.Run("custom creation", func(t *testing.T) {
		m := NewMetrics(500, 30.0)
		assert.NotNil(t, m)
	})
}

func TestMetrics_RequestTracking(t *testing.T) {
	t.Run("start and end request", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Start a request
		startTime := m.StartRequest("req-1", "testFunc", "default")
		assert.False(t, startTime.IsZero())

		// Simulate some work
		time.Sleep(10 * time.Millisecond)

		// End the request successfully
		latency := m.EndRequest(startTime, "req-1", true, "")
		assert.Greater(t, latency, 0.0)

		// Check snapshot
		snapshot := m.Snapshot()
		assert.Equal(t, 1, snapshot.RequestsTotal)
		assert.Equal(t, 1, snapshot.RequestsSuccess)
		assert.Equal(t, 0, snapshot.RequestsFailed)
	})

	t.Run("failed request tracking", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		startTime := m.StartRequest("req-1", "testFunc", "default")
		m.EndRequest(startTime, "req-1", false, "test error")

		snapshot := m.Snapshot()
		assert.Equal(t, 1, snapshot.RequestsTotal)
		assert.Equal(t, 0, snapshot.RequestsSuccess)
		assert.Equal(t, 1, snapshot.RequestsFailed)
	})

	t.Run("multiple requests", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Track multiple requests
		for i := 0; i < 5; i++ {
			startTime := m.StartRequest("req-"+string(rune('0'+i)), "testFunc", "default")
			success := i%2 == 0
			m.EndRequest(startTime, "req-"+string(rune('0'+i)), success, "")
		}

		snapshot := m.Snapshot()
		assert.Equal(t, 5, snapshot.RequestsTotal)
		assert.Equal(t, 3, snapshot.RequestsSuccess)
		assert.Equal(t, 2, snapshot.RequestsFailed)
	})
}

func TestMetrics_LatencyPercentiles(t *testing.T) {
	t.Run("latency percentiles calculation", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Simulate requests with different latencies
		latencies := []float64{10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0}
		for i, lat := range latencies {
			_ = m.StartRequest("req-"+string(rune('0'+i)), "testFunc", "default")
			m.mu.Lock()
			m.latencies = append(m.latencies, lat)
			m.mu.Unlock()
		}

		snapshot := m.Snapshot()
		assert.Equal(t, 10, snapshot.RequestsTotal)
		assert.Greater(t, snapshot.LatencyAvgMs, 0.0)
		assert.Greater(t, snapshot.LatencyP50Ms, 0.0)
		assert.Greater(t, snapshot.LatencyP95Ms, 0.0)
		assert.Greater(t, snapshot.LatencyP99Ms, 0.0)
		assert.Equal(t, 10.0, snapshot.LatencyMinMs)
		assert.Equal(t, 100.0, snapshot.LatencyMaxMs)
	})
}

func TestMetrics_CircuitBreaker(t *testing.T) {
	t.Run("circuit breaker state tracking", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Initially closed
		snapshot := m.Snapshot()
		assert.Equal(t, "closed", snapshot.CircuitBreakerState)
		assert.Equal(t, 0, snapshot.CircuitBreakerTrips)

		// Trip the circuit
		m.RecordCircuitBreakerTrip()
		snapshot = m.Snapshot()
		assert.Equal(t, "open", snapshot.CircuitBreakerState)
		assert.Equal(t, 1, snapshot.CircuitBreakerTrips)

		// Reset the circuit
		m.RecordCircuitBreakerReset()
		snapshot = m.Snapshot()
		assert.Equal(t, "closed", snapshot.CircuitBreakerState)

		// Half-open state
		m.RecordCircuitBreakerHalfOpen()
		snapshot = m.Snapshot()
		assert.Equal(t, "half-open", snapshot.CircuitBreakerState)
	})
}

func TestMetrics_Heartbeat(t *testing.T) {
	t.Run("heartbeat RTT tracking", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Record some RTTs
		m.RecordHeartbeatRtt(10.5)
		m.RecordHeartbeatRtt(15.2)
		m.RecordHeartbeatRtt(12.8)

		snapshot := m.Snapshot()
		assert.Greater(t, snapshot.HeartbeatRttAvgMs, 0.0)
		assert.Equal(t, 12.8, snapshot.HeartbeatRttLastMs)
		assert.Equal(t, 0, snapshot.HeartbeatMisses)
	})

	t.Run("heartbeat miss tracking", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		m.RecordHeartbeatMiss()
		m.RecordHeartbeatMiss()

		snapshot := m.Snapshot()
		assert.Equal(t, 2, snapshot.HeartbeatMisses)
	})
}

func TestMetrics_QueueDepth(t *testing.T) {
	t.Run("queue depth tracking", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Simulate queue changes
		m.StartRequest("req-1", "test", "default")
		m.StartRequest("req-2", "test", "default")
		m.StartRequest("req-3", "test", "default")

		snapshot := m.Snapshot()
		assert.Equal(t, 3, snapshot.QueueDepth)
		assert.Equal(t, 3, snapshot.QueueMaxDepth)

		// End one request
		startTime := time.Now()
		m.EndRequest(startTime, "req-1", true, "")

		snapshot = m.Snapshot()
		assert.Equal(t, 2, snapshot.QueueDepth)
		assert.Equal(t, 3, snapshot.QueueMaxDepth) // Max should remain
	})
}

func TestMetrics_Reset(t *testing.T) {
	t.Run("reset clears all metrics", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)

		// Add some data
		startTime := m.StartRequest("req-1", "test", "default")
		m.EndRequest(startTime, "req-1", true, "")
		m.RecordCircuitBreakerTrip()
		m.RecordHeartbeatRtt(10.0)
		m.RecordHeartbeatMiss()

		// Reset
		m.Reset()

		snapshot := m.Snapshot()
		assert.Equal(t, 0, snapshot.RequestsTotal)
		assert.Equal(t, 0, snapshot.RequestsSuccess)
		assert.Equal(t, 0, snapshot.RequestsFailed)
		assert.Equal(t, 0, snapshot.CircuitBreakerTrips)
		assert.Equal(t, "closed", snapshot.CircuitBreakerState)
		assert.Equal(t, 0, snapshot.QueueDepth)
		assert.Equal(t, 0, snapshot.QueueMaxDepth)
		assert.Equal(t, 0, snapshot.HeartbeatMisses)
		assert.Equal(t, 0.0, snapshot.HeartbeatRttAvgMs)
	})
}

func TestMetrics_SnapshotTimestamp(t *testing.T) {
	t.Run("snapshot includes timestamp", func(t *testing.T) {
		m := NewMetrics(1000, 60.0)
		before := time.Now()
		snapshot := m.Snapshot()
		after := time.Now()

		assert.False(t, snapshot.Timestamp.IsZero())
		assert.True(t, snapshot.Timestamp.After(before) || snapshot.Timestamp.Equal(before))
		assert.True(t, snapshot.Timestamp.Before(after) || snapshot.Timestamp.Equal(after))
	})
}
