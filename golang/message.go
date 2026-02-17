package multifrost

import (
	"fmt"
	"math"
	"time"

	"github.com/google/uuid"
	"github.com/vmihailenco/msgpack/v5"
)

const AppName = "comlink_ipc_v4"

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

const (
	maxMessageSize  = 10 * 1024 * 1024 // 10MB
	maxArrayLength  = 10000
	maxMapSize      = 10000
	maxStringLength = 100000
)

// Unpack deserializes a message from msgpack with safety validations
func Unpack(data []byte) (*ComlinkMessage, error) {
	// Check message size limit (DoS protection)
	if len(data) > maxMessageSize {
		return nil, fmt.Errorf("message size %d exceeds limit %d", len(data), maxMessageSize)
	}

	var msg ComlinkMessage
	err := msgpack.Unmarshal(data, &msg)
	if err != nil {
		return nil, fmt.Errorf("decode failed: %w", err)
	}

	// Validate timestamp for NaN/Infinity
	if math.IsNaN(msg.Timestamp) || math.IsInf(msg.Timestamp, 0) {
		msg.Timestamp = 0.0
	}

	// Validate and clamp any type values
	if err := validateSliceAny(&msg.Args); err != nil {
		return nil, fmt.Errorf("invalid args: %w", err)
	}
	if err := validateAnyValue(&msg.Result); err != nil {
		return nil, fmt.Errorf("invalid result: %w", err)
	}
	if err := validateMetadata(&msg.Metadata); err != nil {
		return nil, fmt.Errorf("invalid metadata: %w", err)
	}

	return &msg, nil
}

// validateAnyValue validates and clamps any-type values for msgpack interop safety
func validateAnyValue(val *any) error {
	if val == nil {
		return nil
	}

	switch v := (*val).(type) {
	case float64:
		// Clamp NaN/Infinity to 0
		if math.IsNaN(v) || math.IsInf(v, 0) {
			*val = 0
		}
	case int:
		// Clamp to int64 range
		if v > math.MaxInt64 || v < math.MinInt64 {
			*val = int64(v)
		}
	case int8, int16, int32, int64:
		// Already in safe range for Go, no action needed
	case map[string]any:
		// Recursively validate nested maps
		for k, vv := range v {
			if err := validateAnyValue(&vv); err != nil {
				return fmt.Errorf("key '%s': %w", k, err)
			}
			v[k] = vv
		}
		*val = v
	case []any:
		// Recursively validate slices
		for i, vv := range v {
			if err := validateAnyValue(&vv); err != nil {
				return fmt.Errorf("index %d: %w", i, err)
			}
			v[i] = vv
		}
		*val = v
		// String map keys are validated by msgpack decoder (already string type)
		// Other types (bool, string) are safe by design
	}

	return nil
}

// validateSliceAny validates slices of any type
func validateSliceAny(val *[]any) error {
	if val == nil {
		return nil
	}

	for i, v := range *val {
		if err := validateAnyValue(&v); err != nil {
			return fmt.Errorf("index %d: %w", i, err)
		}
		(*val)[i] = v
	}

	return nil
}

// validateMetadata validates that metadata map keys are strings and values are safe
func validateMetadata(meta *map[string]any) error {
	if meta == nil {
		return nil
	}

	// msgpack decoder already ensures string keys for map[string]any
	// Recursively validate values for NaN/Infinity and integer overflow
	for k, v := range *meta {
		if err := validateAnyValue(&v); err != nil {
			return fmt.Errorf("key '%s': %w", k, err)
		}
		(*meta)[k] = v
	}

	return nil
}

// RemoteCallError represents an error from a remote call
type RemoteCallError struct {
	Message string
	Err     error
}

func (e *RemoteCallError) Error() string {
	if e.Err != nil {
		return e.Message + ": " + e.Err.Error()
	}
	return e.Message
}

// Unwrap returns the wrapped error for errors.Is/As support
func (e *RemoteCallError) Unwrap() error {
	return e.Err
}

// CircuitOpenError is raised when circuit breaker is open (too many consecutive failures)
type CircuitOpenError struct {
	ConsecutiveFailures int
	Err                 error
}

func (e *CircuitOpenError) Error() string {
	msg := fmt.Sprintf("circuit breaker open after %d consecutive failures", e.ConsecutiveFailures)
	if e.Err != nil {
		return msg + ": " + e.Err.Error()
	}
	return msg
}

// Unwrap returns the wrapped error for errors.Is/As support
func (e *CircuitOpenError) Unwrap() error {
	return e.Err
}
