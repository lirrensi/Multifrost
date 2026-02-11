package multifrost

import (
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/vmihailenco/msgpack/v5"
)

const AppName = "comlink_ipc_v3"

// MessageType represents the type of IPC message
type MessageType string

const (
	MessageTypeCall      MessageType = "call"
	MessageTypeResponse  MessageType = "response"
	MessageTypeError     MessageType = "error"
	MessageTypeStdout    MessageType = "stdout"
	MessageTypeStderr    MessageType = "stderr"
	MessageTypeHeartbeat MessageType = "heartbeat"
	MessageTypeShutdown  MessageType = "shutdown"
)

// ComlinkMessage represents an IPC message
type ComlinkMessage struct {
	App        string         `msgpack:"app"`
	ID         string         `msgpack:"id"`
	Type       string         `msgpack:"type"`
	Timestamp  float64        `msgpack:"timestamp"`
	Function   string         `msgpack:"function,omitempty"`
	Args       []any          `msgpack:"args,omitempty"`
	Namespace  string         `msgpack:"namespace,omitempty"`
	Result     any            `msgpack:"result,omitempty"`
	Error      string         `msgpack:"error,omitempty"`
	Output     string         `msgpack:"output,omitempty"`
	ClientName string         `msgpack:"client_name,omitempty"`
	Metadata   map[string]any `msgpack:"metadata,omitempty"`
}

// NewComlinkMessage creates a new message with defaults
func NewComlinkMessage() *ComlinkMessage {
	return &ComlinkMessage{
		App:       AppName,
		ID:        uuid.New().String(),
		Timestamp: float64(time.Now().UnixNano()) / 1e9,
	}
}

// CreateCall creates a function call message
func CreateCall(function string, args []any, namespace string, msgID string, clientName string) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(MessageTypeCall)
	msg.Function = function
	msg.Args = args
	msg.Namespace = namespace
	msg.ClientName = clientName
	if msgID != "" {
		msg.ID = msgID
	}
	return msg
}

// CreateResponse creates a response message
func CreateResponse(result any, msgID string) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(MessageTypeResponse)
	msg.Result = result
	msg.ID = msgID
	return msg
}

// CreateError creates an error message
func CreateError(err string, msgID string) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(MessageTypeError)
	msg.Error = err
	msg.ID = msgID
	return msg
}

// CreateOutput creates an output message (stdout/stderr)
func CreateOutput(output string, msgType MessageType) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(msgType)
	msg.Output = output
	return msg
}

// CreateHeartbeat creates a heartbeat request message
func CreateHeartbeat(msgID string) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(MessageTypeHeartbeat)
	msg.Metadata = map[string]any{
		"hb_timestamp": float64(time.Now().UnixNano()) / 1e9,
	}
	if msgID != "" {
		msg.ID = msgID
	}
	return msg
}

// CreateHeartbeatResponse creates a heartbeat response message
func CreateHeartbeatResponse(requestID string, originalTimestamp float64) *ComlinkMessage {
	msg := NewComlinkMessage()
	msg.Type = string(MessageTypeHeartbeat)
	msg.ID = requestID
	msg.Metadata = map[string]any{
		"hb_timestamp": originalTimestamp,
		"hb_response":  true,
	}
	return msg
}

// Pack serializes the message to msgpack
func (m *ComlinkMessage) Pack() ([]byte, error) {
	return msgpack.Marshal(m)
}

// Unpack deserializes a message from msgpack
func Unpack(data []byte) (*ComlinkMessage, error) {
	var msg ComlinkMessage
	err := msgpack.Unmarshal(data, &msg)
	if err != nil {
		return nil, err
	}
	return &msg, nil
}

// RemoteCallError represents an error from a remote call
type RemoteCallError struct {
	Message string
}

func (e *RemoteCallError) Error() string {
	return e.Message
}

// CircuitOpenError is raised when circuit breaker is open (too many consecutive failures)
type CircuitOpenError struct {
	ConsecutiveFailures int
}

func (e *CircuitOpenError) Error() string {
	return fmt.Sprintf("circuit breaker open after %d consecutive failures", e.ConsecutiveFailures)
}
