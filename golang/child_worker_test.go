package multifrost

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestChildWorker_Creation(t *testing.T) {
	t.Run("basic creation", func(t *testing.T) {
		worker := NewChildWorker()
		require.NotNil(t, worker)
		assert.Equal(t, "default", worker.Namespace)
		assert.False(t, worker.running)
	})

	t.Run("creation with service ID", func(t *testing.T) {
		worker := NewChildWorkerWithService("test-service")
		require.NotNil(t, worker)
		assert.Equal(t, "test-service", worker.ServiceID)
		assert.Equal(t, "default", worker.Namespace)
	})
}

func TestChildWorker_MethodDiscovery(t *testing.T) {
	t.Run("list functions excludes base methods", func(t *testing.T) {
		worker := NewChildWorker()
		methods := worker.ListFunctions()

		// Should not include base methods
		for _, method := range methods {
			assert.NotEqual(t, "Start", method)
			assert.NotEqual(t, "Stop", method)
			assert.NotEqual(t, "Run", method)
			assert.NotEqual(t, "ListFunctions", method)
		}
	})
}

func TestChildWorker_State(t *testing.T) {
	t.Run("initial state", func(t *testing.T) {
		worker := NewChildWorker()
		assert.False(t, worker.IsRunning())
		assert.Equal(t, 0, worker.GetPort())
	})
}

func TestChildWorker_DoneChannel(t *testing.T) {
	t.Run("done channel exists", func(t *testing.T) {
		worker := NewChildWorker()
		done := worker.Done()
		assert.NotNil(t, done)

		// Channel should be open initially
		select {
		case <-done:
			t.Error("Done channel should not be closed initially")
		case <-time.After(10 * time.Millisecond):
			// Expected - channel is open
		}
	})
}
