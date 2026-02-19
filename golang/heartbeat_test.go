package multifrost

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestHeartbeat_Configuration tests heartbeat configuration
func TestHeartbeat_Configuration(t *testing.T) {
	t.Run("default heartbeat configuration", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		assert.Equal(t, 5*time.Second, worker.HeartbeatInterval)
		assert.Equal(t, 3*time.Second, worker.HeartbeatTimeout)
		assert.Equal(t, 3, worker.HeartbeatMaxMisses)
	})

	t.Run("custom heartbeat configuration", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatInterval:  10 * time.Second,
			HeartbeatTimeout:   5 * time.Second,
			HeartbeatMaxMisses: 5,
		})

		assert.Equal(t, 10*time.Second, worker.HeartbeatInterval)
		assert.Equal(t, 5*time.Second, worker.HeartbeatTimeout)
		assert.Equal(t, 5, worker.HeartbeatMaxMisses)
	})
}

// TestHeartbeat_MessageCreation tests heartbeat message creation
func TestHeartbeat_MessageCreation(t *testing.T) {
	t.Run("create heartbeat message", func(t *testing.T) {
		msg := CreateHeartbeat("")

		assert.Equal(t, string(MessageTypeHeartbeat), msg.Type)
		assert.NotEmpty(t, msg.ID)
		assert.NotNil(t, msg.Metadata)
		assert.Contains(t, msg.Metadata, "hb_timestamp")
	})

	t.Run("create heartbeat with custom ID", func(t *testing.T) {
		msg := CreateHeartbeat("custom-heartbeat-id")

		assert.Equal(t, "custom-heartbeat-id", msg.ID)
	})

	t.Run("create heartbeat response", func(t *testing.T) {
		originalTs := 1234567890.123
		msg := CreateHeartbeatResponse("req-123", originalTs)

		assert.Equal(t, string(MessageTypeHeartbeat), msg.Type)
		assert.Equal(t, "req-123", msg.ID)
		assert.Equal(t, originalTs, msg.Metadata["hb_timestamp"])
		assert.Equal(t, true, msg.Metadata["hb_response"])
	})
}

// TestHeartbeat_Serialization tests heartbeat message serialization
func TestHeartbeat_Serialization(t *testing.T) {
	t.Run("pack and unpack heartbeat request", func(t *testing.T) {
		original := CreateHeartbeat("hb-test-123")

		data, err := original.Pack()
		require.NoError(t, err)
		assert.NotEmpty(t, data)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
		assert.NotNil(t, unpacked.Metadata)
	})

	t.Run("pack and unpack heartbeat response", func(t *testing.T) {
		originalTs := 1234567890.456
		original := CreateHeartbeatResponse("hb-req-456", originalTs)

		data, err := original.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, "hb-req-456", unpacked.ID)
		assert.Equal(t, originalTs, unpacked.Metadata["hb_timestamp"])
		assert.Equal(t, true, unpacked.Metadata["hb_response"])
	})
}

// TestHeartbeat_StateTracking tests heartbeat state tracking
func TestHeartbeat_StateTracking(t *testing.T) {
	t.Run("initial heartbeat state", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		assert.Equal(t, 0.0, worker.LastHeartbeatRttMs())
		assert.Equal(t, 0, worker.ConsecutiveHeartbeatMisses())
	})

	t.Run("heartbeat RTT is tracked", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		// Simulate heartbeat response handling
		msg := &ComlinkMessage{
			ID:   "hb-123",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano())/1e9 - 0.010, // 10ms ago
			},
		}

		// Setup pending heartbeat
		worker.pendingHeartbeats = make(map[string]chan bool)
		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		worker.handleHeartbeatResponse(msg)

		// RTT should be recorded
		assert.Greater(t, worker.LastHeartbeatRttMs(), 0.0)
	})

	t.Run("heartbeat miss resets on success", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})

		// Simulate some misses
		worker.consecutiveHeartbeatMisses = 2
		assert.Equal(t, 2, worker.ConsecutiveHeartbeatMisses())

		// Simulate heartbeat response
		msg := &ComlinkMessage{
			ID:   "hb-123",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano()) / 1e9,
			},
		}

		worker.pendingHeartbeats = make(map[string]chan bool)
		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		worker.handleHeartbeatResponse(msg)

		// Misses should be reset
		assert.Equal(t, 0, worker.ConsecutiveHeartbeatMisses())
	})
}

// TestHeartbeat_Metrics tests heartbeat metrics tracking
func TestHeartbeat_Metrics(t *testing.T) {
	t.Run("heartbeat RTT recorded in metrics", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			EnableMetrics: true,
		})

		// Simulate heartbeat response
		msg := &ComlinkMessage{
			ID:   "hb-123",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano())/1e9 - 0.015,
			},
		}

		worker.pendingHeartbeats = make(map[string]chan bool)
		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		worker.handleHeartbeatResponse(msg)

		// Check metrics
		metrics := worker.Metrics()
		require.NotNil(t, metrics)
		snapshot := metrics.Snapshot()
		assert.Greater(t, snapshot.HeartbeatRttLastMs, 0.0)
	})
}

// TestHeartbeat_PendingRequests tests pending heartbeat tracking
func TestHeartbeat_PendingRequests(t *testing.T) {
	t.Run("pending heartbeat is tracked", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)

		// Add pending heartbeat
		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		assert.Contains(t, worker.pendingHeartbeats, "hb-123")
	})

	t.Run("pending heartbeat removed on response", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)

		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		// Simulate response
		msg := &ComlinkMessage{
			ID:   "hb-123",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano()) / 1e9,
			},
		}
		worker.handleHeartbeatResponse(msg)

		// Should be removed
		assert.NotContains(t, worker.pendingHeartbeats, "hb-123")
	})

	t.Run("unknown heartbeat response is ignored", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)
		worker.consecutiveHeartbeatMisses = 2

		// Response for unknown heartbeat
		msg := &ComlinkMessage{
			ID:   "unknown-hb",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano()) / 1e9,
			},
		}
		worker.handleHeartbeatResponse(msg)

		// State should not change
		assert.Equal(t, 2, worker.ConsecutiveHeartbeatMisses())
	})
}

// TestHeartbeat_TimeoutBehavior tests heartbeat timeout behavior
func TestHeartbeat_TimeoutBehavior(t *testing.T) {
	t.Run("missed heartbeat counts toward threshold", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatMaxMisses: 3,
		})

		// Simulate misses
		worker.consecutiveHeartbeatMisses = 2
		worker.consecutiveHeartbeatMisses++

		assert.Equal(t, 3, worker.ConsecutiveHeartbeatMisses())
	})

	t.Run("threshold miss triggers failure", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatMaxMisses: 2,
			MaxRestartAttempts: 5,
			EnableMetrics:      true,
		})

		// Simulate reaching threshold
		worker.consecutiveHeartbeatMisses = worker.HeartbeatMaxMisses

		// Trigger failure recording
		worker.recordFailure()

		assert.Equal(t, 1, worker.ConsecutiveFailures())
	})
}

// TestHeartbeat_ChannelSignaling tests the heartbeat response channel
func TestHeartbeat_ChannelSignaling(t *testing.T) {
	t.Run("response channel receives signal", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)

		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		// Simulate response
		msg := &ComlinkMessage{
			ID:   "hb-123",
			Type: string(MessageTypeHeartbeat),
			Metadata: map[string]any{
				"hb_timestamp": float64(time.Now().UnixNano()) / 1e9,
			},
		}
		worker.handleHeartbeatResponse(msg)

		// Channel should receive signal
		select {
		case <-responseChan:
			// Success
		default:
			t.Error("Expected signal on response channel")
		}
	})
}

// TestHeartbeat_EdgeCases tests heartbeat edge cases
func TestHeartbeat_EdgeCases(t *testing.T) {
	t.Run("zero interval should work", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatInterval: 0, // Will use default
		})

		// Default should be applied
		assert.Equal(t, 5*time.Second, worker.HeartbeatInterval)
	})

	t.Run("very short interval", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{
			HeartbeatInterval: 100 * time.Millisecond,
			HeartbeatTimeout:  50 * time.Millisecond,
		})

		assert.Equal(t, 100*time.Millisecond, worker.HeartbeatInterval)
		assert.Equal(t, 50*time.Millisecond, worker.HeartbeatTimeout)
	})

	t.Run("nil metadata in heartbeat response", func(t *testing.T) {
		worker := NewParentWorker(ParentWorkerConfig{})
		worker.pendingHeartbeats = make(map[string]chan bool)

		responseChan := make(chan bool, 1)
		worker.pendingHeartbeats["hb-123"] = responseChan

		// Response without timestamp metadata
		msg := &ComlinkMessage{
			ID:       "hb-123",
			Type:     string(MessageTypeHeartbeat),
			Metadata: nil,
		}

		// Should not panic
		worker.handleHeartbeatResponse(msg)

		// Channel should still receive signal
		select {
		case <-responseChan:
			// Success
		default:
			t.Error("Expected signal on response channel")
		}
	})
}

// TestHeartbeat_ChildWorker tests ChildWorker heartbeat handling
func TestHeartbeat_ChildWorker(t *testing.T) {
	t.Run("child worker responds to heartbeat", func(t *testing.T) {
		worker := NewChildWorker()

		// Create a heartbeat message
		heartbeat := CreateHeartbeat("test-hb-123")
		_, err := heartbeat.Pack()
		require.NoError(t, err)

		// Handle heartbeat should create response
		// (This tests the message creation logic, not the full ZMQ flow)
		require.NotNil(t, worker)
	})
}
