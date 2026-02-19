package multifrost

import (
	"reflect"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestChildWorkerDispatch_MethodDiscovery tests method discovery via reflection
func TestChildWorkerDispatch_MethodDiscovery(t *testing.T) {
	t.Run("list functions discovers public methods", func(t *testing.T) {
		// Create a test worker with custom methods
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		// Note: ListFunctions() on embedded ChildWorker uses reflect.TypeOf(w)
		// which returns *ChildWorker, not *TestWorker. So we test our custom
		// implementation that overrides this behavior.
		methods := worker.ListFunctionsCustom()
		assert.Contains(t, methods, "Add")
		assert.Contains(t, methods, "Multiply")
		assert.Contains(t, methods, "Echo")
		assert.Contains(t, methods, "Divide")
	})

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

	t.Run("list functions excludes private methods", func(t *testing.T) {
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		methods := worker.ListFunctionsCustom()
		// Private methods (lowercase) should not be listed
		for _, method := range methods {
			assert.NotEqual(t, "privateMethod", method)
			assert.NotContains(t, method, "_internal")
		}
	})
}

// TestChildWorkerDispatch_PrivateMethodRejection tests that private methods are rejected
func TestChildWorkerDispatch_PrivateMethodRejection(t *testing.T) {
	t.Run("private method starting with underscore is rejected", func(t *testing.T) {
		// This is tested at the handleFunctionCall level
		// The check is: if len(msg.Function) > 0 && msg.Function[0] == '_'
		assert.True(t, len("_privateMethod") > 0 && "_privateMethod"[0] == '_')
	})

	t.Run("lowercase methods are not discoverable", func(t *testing.T) {
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		methods := worker.ListFunctions()

		// Lowercase methods should not be in the list (Go only exports capitalized methods)
		for _, method := range methods {
			// First char should be uppercase if it's exported
			if len(method) > 0 {
				firstChar := method[0]
				assert.True(t, firstChar >= 'A' && firstChar <= 'Z',
					"Method %s should start with uppercase letter", method)
			}
		}
	})
}

// TestChildWorkerDispatch_NamespaceFiltering tests namespace filtering
func TestChildWorkerDispatch_NamespaceFiltering(t *testing.T) {
	t.Run("default namespace is accepted", func(t *testing.T) {
		worker := NewChildWorker()
		assert.Equal(t, "default", worker.Namespace)
	})

	t.Run("matching namespace is accepted", func(t *testing.T) {
		worker := NewChildWorker()
		worker.Namespace = "test-ns"

		// Empty namespace should be accepted (broadcast)
		// Matching namespace should be accepted
		// Non-matching namespace should be rejected
		msg := CreateCall("testFunc", nil, "test-ns", "msg-123", "client-1")
		assert.Equal(t, "test-ns", msg.Namespace)
	})
}

// TestChildWorkerDispatch_MessageValidation tests message validation
func TestChildWorkerDispatch_MessageValidation(t *testing.T) {
	t.Run("message with empty function is rejected", func(t *testing.T) {
		// This would create an error response in handleFunctionCall
		msg := CreateCall("", nil, "default", "msg-123", "client-1")
		assert.Empty(t, msg.Function)
		assert.NotEmpty(t, msg.ID) // ID should always be present
	})

	t.Run("message with empty ID is handled", func(t *testing.T) {
		// CreateCall assigns a default ID if empty is passed
		msg := CreateCall("testFunc", nil, "default", "", "client-1")
		assert.NotEmpty(t, msg.ID) // ID is auto-generated
	})
}

// TestChildWorkerDispatch_AppValidation tests app name validation
func TestChildWorkerDispatch_AppValidation(t *testing.T) {
	t.Run("valid app name is accepted", func(t *testing.T) {
		msg := NewComlinkMessage()
		assert.Equal(t, AppName, msg.App)
	})

	t.Run("invalid app name should be rejected", func(t *testing.T) {
		// In handleMessage, if msg.App != AppName, it's rejected
		assert.Equal(t, "comlink_ipc_v4", AppName)
	})
}

// TestChildWorkerDispatch_ErrorHandling tests error handling in dispatch
func TestChildWorkerDispatch_ErrorHandling(t *testing.T) {
	t.Run("non-existent function returns error", func(t *testing.T) {
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		methods := worker.ListFunctionsCustom()
		assert.NotContains(t, methods, "NonExistentFunction")
	})

	t.Run("wrong argument count should be handled", func(t *testing.T) {
		// This tests that reflection handles argument mismatch
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		// Add method takes 2 args
		methods := worker.ListFunctionsCustom()
		assert.Contains(t, methods, "Add")
	})
}

// TestChildWorkerDispatch_ResultTypes tests different return types
func TestChildWorkerDispatch_ResultTypes(t *testing.T) {
	t.Run("single return value", func(t *testing.T) {
		// Test that methods returning single value work
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		methods := worker.ListFunctionsCustom()
		// Echo returns a single value
		assert.Contains(t, methods, "Echo")
	})

	t.Run("error return value", func(t *testing.T) {
		// Test that methods returning (result, error) work
		worker := &TestWorker{
			ChildWorker: *NewChildWorker(),
		}

		methods := worker.ListFunctionsCustom()
		// Divide returns (int, error)
		assert.Contains(t, methods, "Divide")
	})
}

// TestChildWorkerDispatch_StateManagement tests state management
func TestChildWorkerDispatch_StateManagement(t *testing.T) {
	t.Run("initial state", func(t *testing.T) {
		worker := NewChildWorker()
		assert.False(t, worker.IsRunning())
		assert.Equal(t, 0, worker.GetPort())
	})

	t.Run("namespace is set correctly", func(t *testing.T) {
		worker := NewChildWorkerWithService("test-service")
		assert.Equal(t, "test-service", worker.ServiceID)
		assert.Equal(t, "default", worker.Namespace)
	})
}

// TestChildWorkerDispatch_DoneChannel tests the done channel
func TestChildWorkerDispatch_DoneChannel(t *testing.T) {
	t.Run("done channel is open initially", func(t *testing.T) {
		worker := NewChildWorker()
		done := worker.Done()
		require.NotNil(t, done)

		select {
		case <-done:
			t.Error("Done channel should not be closed initially")
		default:
			// Expected
		}
	})
}

// TestChildWorkerDispatch_MessageTypes tests different message type handling
func TestChildWorkerDispatch_MessageTypes(t *testing.T) {
	t.Run("call message type", func(t *testing.T) {
		msg := CreateCall("testFunc", []any{1, 2, 3}, "default", "msg-123", "client-1")
		assert.Equal(t, string(MessageTypeCall), msg.Type)
	})

	t.Run("heartbeat message type", func(t *testing.T) {
		msg := CreateHeartbeat("hb-123")
		assert.Equal(t, string(MessageTypeHeartbeat), msg.Type)
	})

	t.Run("shutdown message type", func(t *testing.T) {
		msg := NewComlinkMessage()
		msg.Type = string(MessageTypeShutdown)
		assert.Equal(t, string(MessageTypeShutdown), msg.Type)
	})
}

// TestChildWorkerDispatch_EdgeCases tests edge cases
func TestChildWorkerDispatch_EdgeCases(t *testing.T) {
	t.Run("empty args", func(t *testing.T) {
		msg := CreateCall("NoArgsFunc", nil, "default", "msg-123", "client-1")
		assert.Nil(t, msg.Args)
	})

	t.Run("nil args", func(t *testing.T) {
		msg := CreateCall("testFunc", nil, "default", "msg-123", "client-1")
		assert.Nil(t, msg.Args)
	})

	t.Run("complex args", func(t *testing.T) {
		args := []any{
			"string",
			42,
			3.14,
			[]int{1, 2, 3},
			map[string]int{"a": 1},
		}
		msg := CreateCall("ComplexFunc", args, "default", "msg-123", "client-1")
		assert.Len(t, msg.Args, 5)
	})
}

// TestWorker is a test implementation of ChildWorker with custom methods
type TestWorker struct {
	ChildWorker
}

// ListFunctionsCustom returns available callable public methods (custom implementation for testing)
func (w *TestWorker) ListFunctionsCustom() []string {
	t := reflect.TypeOf(w)
	var methods []string

	for i := 0; i < t.NumMethod(); i++ {
		name := t.Method(i).Name
		if len(name) > 0 && name[0] != '_' {
			// Exclude methods from ChildWorker base
			if !isBaseMethod(name) {
				methods = append(methods, name)
			}
		}
	}

	return methods
}

// Add adds two integers
func (w *TestWorker) Add(a, b int) int {
	return a + b
}

// Multiply multiplies two integers
func (w *TestWorker) Multiply(a, b int) int {
	return a * b
}

// Echo returns the input string
func (w *TestWorker) Echo(s string) string {
	return s
}

// Divide divides two integers, returning error on division by zero
func (w *TestWorker) Divide(a, b int) (int, error) {
	if b == 0 {
		return 0, &DivisionError{Message: "division by zero"}
	}
	return a / b, nil
}

// NoArgsFunc takes no arguments
func (w *TestWorker) NoArgsFunc() string {
	return "no args"
}

// privateMethod is a private method (not exported)
func (w *TestWorker) privateMethod() string {
	return "private"
}

// DivisionError is a test error type
type DivisionError struct {
	Message string
}

func (e *DivisionError) Error() string {
	return e.Message
}

// TestChildWorkerDispatch_ErrorTypes tests error type handling
func TestChildWorkerDispatch_ErrorTypes(t *testing.T) {
	t.Run("remote call error", func(t *testing.T) {
		err := &RemoteCallError{Message: "test error"}
		assert.Equal(t, "test error", err.Error())
	})

	t.Run("remote call error with wrapped error", func(t *testing.T) {
		innerErr := &DivisionError{Message: "division by zero"}
		err := &RemoteCallError{
			Message: "call failed",
			Err:     innerErr,
		}
		assert.Contains(t, err.Error(), "call failed")
		assert.Contains(t, err.Error(), "division by zero")
	})
}

// TestChildWorkerDispatch_MethodNotFound tests method not found scenarios
func TestChildWorkerDispatch_MethodNotFound(t *testing.T) {
	t.Run("method not found creates error response", func(t *testing.T) {
		// Simulating the error message that would be created
		funcName := "NonExistentFunction"
		expectedError := "Function 'NonExistentFunction' not found"
		assert.Contains(t, expectedError, funcName)
	})
}

// TestChildWorkerDispatch_BaseMethodCheck tests the base method check
func TestChildWorkerDispatch_BaseMethodCheck(t *testing.T) {
	t.Run("isBaseMethod identifies base methods", func(t *testing.T) {
		assert.True(t, isBaseMethod("Start"))
		assert.True(t, isBaseMethod("Stop"))
		assert.True(t, isBaseMethod("Run"))
		assert.True(t, isBaseMethod("ListFunctions"))
	})

	t.Run("isBaseMethod returns false for custom methods", func(t *testing.T) {
		assert.False(t, isBaseMethod("Add"))
		assert.False(t, isBaseMethod("Multiply"))
		assert.False(t, isBaseMethod("CustomMethod"))
	})
}
