package multifrost

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMessage_Creation(t *testing.T) {
	t.Run("new message has defaults", func(t *testing.T) {
		msg := NewComlinkMessage()
		require.NotNil(t, msg)
		assert.Equal(t, AppName, msg.App)
		assert.NotEmpty(t, msg.ID)
		assert.Greater(t, msg.Timestamp, 0.0)
	})

	t.Run("create call message", func(t *testing.T) {
		args := []any{1, 2, 3}
		msg := CreateCall("testFunc", args, "test-ns", "msg-123", "client-1")

		assert.Equal(t, string(MessageTypeCall), msg.Type)
		assert.Equal(t, "testFunc", msg.Function)
		assert.Equal(t, args, msg.Args)
		assert.Equal(t, "test-ns", msg.Namespace)
		assert.Equal(t, "msg-123", msg.ID)
		assert.Equal(t, "client-1", msg.ClientName)
	})

	t.Run("create response message", func(t *testing.T) {
		msg := CreateResponse("test result", "msg-123")

		assert.Equal(t, string(MessageTypeResponse), msg.Type)
		assert.Equal(t, "test result", msg.Result)
		assert.Equal(t, "msg-123", msg.ID)
	})

	t.Run("create error message", func(t *testing.T) {
		msg := CreateError("test error", "msg-123")

		assert.Equal(t, string(MessageTypeError), msg.Type)
		assert.Equal(t, "test error", msg.Error)
		assert.Equal(t, "msg-123", msg.ID)
	})

	t.Run("create output message", func(t *testing.T) {
		msg := CreateOutput("test output", MessageTypeStdout)

		assert.Equal(t, string(MessageTypeStdout), msg.Type)
		assert.Equal(t, "test output", msg.Output)
	})

	t.Run("create stderr output message", func(t *testing.T) {
		msg := CreateOutput("error output", MessageTypeStderr)

		assert.Equal(t, string(MessageTypeStderr), msg.Type)
		assert.Equal(t, "error output", msg.Output)
	})
}

func TestMessage_Heartbeat(t *testing.T) {
	t.Run("create heartbeat request", func(t *testing.T) {
		msg := CreateHeartbeat("")

		assert.Equal(t, string(MessageTypeHeartbeat), msg.Type)
		assert.NotEmpty(t, msg.ID)
		assert.NotNil(t, msg.Metadata)
		assert.Contains(t, msg.Metadata, "hb_timestamp")
		assert.Greater(t, msg.Metadata["hb_timestamp"], 0.0)
	})

	t.Run("create heartbeat with custom ID", func(t *testing.T) {
		msg := CreateHeartbeat("custom-id")

		assert.Equal(t, "custom-id", msg.ID)
	})

	t.Run("create heartbeat response", func(t *testing.T) {
		originalTs := 1234567890.123
		msg := CreateHeartbeatResponse("req-123", originalTs)

		assert.Equal(t, string(MessageTypeHeartbeat), msg.Type)
		assert.Equal(t, "req-123", msg.ID)
		assert.NotNil(t, msg.Metadata)
		assert.Equal(t, originalTs, msg.Metadata["hb_timestamp"])
		assert.Equal(t, true, msg.Metadata["hb_response"])
	})
}

func TestMessage_Serialization(t *testing.T) {
	t.Run("pack and unpack message", func(t *testing.T) {
		original := CreateCall("testFunc", []any{1, 2, 3}, "test-ns", "msg-123", "client-1")

		data, err := original.Pack()
		require.NoError(t, err)
		assert.NotNil(t, data)
		assert.Greater(t, len(data), 0)

		unpacked, err := Unpack(data)
		require.NoError(t, err)
		assert.NotNil(t, unpacked)

		assert.Equal(t, original.App, unpacked.App)
		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
		assert.Equal(t, original.Function, unpacked.Function)
		assert.Equal(t, original.Namespace, unpacked.Namespace)
		assert.Equal(t, original.ClientName, unpacked.ClientName)
	})

	t.Run("unpack invalid data", func(t *testing.T) {
		invalidData := []byte{0xFF, 0xFF, 0xFF}
		msg, err := Unpack(invalidData)
		assert.Error(t, err)
		assert.Nil(t, msg)
	})

	t.Run("pack and unpack heartbeat", func(t *testing.T) {
		original := CreateHeartbeat("hb-123")

		data, err := original.Pack()
		require.NoError(t, err)

		unpacked, err := Unpack(data)
		require.NoError(t, err)

		assert.Equal(t, original.ID, unpacked.ID)
		assert.Equal(t, original.Type, unpacked.Type)
		assert.NotNil(t, unpacked.Metadata)
		assert.Equal(t, original.Metadata["hb_timestamp"], unpacked.Metadata["hb_timestamp"])
	})
}

func TestMessage_Errors(t *testing.T) {
	t.Run("remote call error", func(t *testing.T) {
		err := &RemoteCallError{Message: "test error"}
		assert.Equal(t, "test error", err.Error())
	})

	t.Run("circuit open error", func(t *testing.T) {
		err := &CircuitOpenError{ConsecutiveFailures: 5}
		assert.Contains(t, err.Error(), "circuit breaker open")
		assert.Contains(t, err.Error(), "5")
	})
}
