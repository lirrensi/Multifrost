package multifrost

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"reflect"
	"strings"
	"syscall"
	"time"

	zmq "github.com/go-zeromq/zmq4"
)

// ChildWorker is the base class for creating ZeroMQ-enabled worker scripts.
// Workers inherit from this class and implement methods that can be called
// remotely by the parent process.
type ChildWorker struct {
	Namespace string
	ServiceID string

	// self holds a reference to the outer struct (for method dispatch)
	self any

	// Internal state
	running bool
	socket  zmq.Socket
	port    int
	done    chan struct{}

	// Output forwarding
	lastSenderID   []byte
	originalStdout *os.File
	originalStderr *os.File
	stdoutPipe     *os.File
	stderrPipe     *os.File
}

// NewChildWorker creates a new ChildWorker instance
func NewChildWorker() *ChildWorker {
	return &ChildWorker{
		Namespace: "default",
		running:   false,
		done:      make(chan struct{}),
	}
}

// NewChildWorkerWithService creates a ChildWorker in connect mode
func NewChildWorkerWithService(serviceID string) *ChildWorker {
	worker := NewChildWorker()
	worker.ServiceID = serviceID
	return worker
}

// SetSelf sets a reference to the outer struct for method dispatch.
// This must be called before Run() when embedding ChildWorker.
// Example:
//
//	type MyWorker struct {
//	    *multifrost.ChildWorker
//	}
//	worker := &MyWorker{ChildWorker: multifrost.NewChildWorker()}
//	worker.SetSelf(worker)  // Enable method discovery
//	worker.Run()
func (w *ChildWorker) SetSelf(self any) {
	w.self = self
}

// Start begins the worker message loop
func (w *ChildWorker) Start() error {
	if err := w.setupZMQ(); err != nil {
		return fmt.Errorf("ZMQ setup failed: %w", err)
	}

	w.running = true
	go w.messageLoop()

	return nil
}

// setupZMQ configures the ZeroMQ ROUTER socket
func (w *ChildWorker) setupZMQ() error {
	// Create ROUTER socket
	w.socket = zmq.NewRouter(context.Background())

	// Determine mode: SPAWN or CONNECT
	portEnv := os.Getenv("COMLINK_ZMQ_PORT")
	if portEnv != "" {
		// SPAWN MODE: Parent gave us port (connect)
		var port int
		if _, err := fmt.Sscanf(portEnv, "%d", &port); err != nil {
			return fmt.Errorf("invalid port '%s': %w", portEnv, err)
		}
		if err := ValidatePort(port); err != nil {
			return err
		}
		w.port = port

		endpoint := fmt.Sprintf("tcp://localhost:%d", w.port)
		if err := w.socket.Dial(endpoint); err != nil {
			return fmt.Errorf("failed to connect to %s: %w", endpoint, err)
		}

	} else if w.ServiceID != "" {
		// CONNECT MODE: Register service, bind to port
		w.port = findFreePort()

		if err := Register(w.ServiceID, w.port); err != nil {
			return fmt.Errorf("failed to register service: %w", err)
		}

		endpoint := fmt.Sprintf("tcp://*:%d", w.port)
		if err := w.socket.Listen(endpoint); err != nil {
			return fmt.Errorf("failed to bind to %s: %w", endpoint, err)
		}

		fmt.Fprintf(os.Stderr, "Service '%s' ready on port %d\n", w.ServiceID, w.port)

	} else {
		return fmt.Errorf("need COMLINK_ZMQ_PORT env or ServiceID parameter")
	}

	// Setup output redirection AFTER ZMQ is ready
	if err := w.setupOutputRedirection(); err != nil {
		return fmt.Errorf("failed to setup output redirection: %w", err)
	}

	return nil
}

// setupOutputRedirection redirects stdout/stderr to send messages over ZMQ
func (w *ChildWorker) setupOutputRedirection() error {
	// Save original stdout/stderr
	w.originalStdout = os.Stdout
	w.originalStderr = os.Stderr

	// Create pipes for stdout/stderr
	stdoutR, stdoutW, err := os.Pipe()
	if err != nil {
		return fmt.Errorf("failed to create stdout pipe: %w", err)
	}
	w.stdoutPipe = stdoutW

	stderrR, stderrW, err := os.Pipe()
	if err != nil {
		return fmt.Errorf("failed to create stderr pipe: %w", err)
	}
	w.stderrPipe = stderrW

	// Redirect stdout/stderr to pipes
	os.Stdout = stdoutW
	os.Stderr = stderrW

	// Start goroutines to read from pipes and send to parent
	go w.forwardOutput(stdoutR, MessageTypeStdout)
	go w.forwardOutput(stderrR, MessageTypeStderr)

	return nil
}

// forwardOutput reads from a pipe and sends output messages to parent
func (w *ChildWorker) forwardOutput(pipe *os.File, msgType MessageType) {
	buf := make([]byte, 4096)
	for {
		n, err := pipe.Read(buf)
		if n > 0 {
			output := string(buf[:n])
			w.sendOutput(output, msgType)
		}
		if err != nil {
			break
		}
	}
}

// sendOutput sends stdout/stderr output to parent with retry logic
func (w *ChildWorker) sendOutput(output string, msgType MessageType) {
	if w.socket == nil || len(w.lastSenderID) == 0 {
		return // No parent connected yet
	}

	// Trim trailing newlines
	output = strings.TrimRight(output, "\n")
	if output == "" {
		return
	}

	msg := CreateOutput(output, msgType)
	data, err := msg.Pack()
	if err != nil {
		return
	}

	// Retry up to 2 times with 1ms delay
	maxRetries := 2
	for attempt := 0; attempt < maxRetries; attempt++ {
		zmqMsg := zmq.NewMsgFrom(w.lastSenderID, []byte{}, data)
		if err := w.socket.Send(zmqMsg); err == nil {
			return // Success
		}
		// Retry after brief delay
		if attempt < maxRetries-1 {
			time.Sleep(1 * time.Millisecond)
		}
	}
}

// messageLoop handles incoming messages
func (w *ChildWorker) messageLoop() {
	for w.running {
		// ROUTER socket receives: [sender_id, empty_frame, message_data]
		msg, err := w.socket.Recv()
		if err != nil {
			if w.running {
				fmt.Fprintf(os.Stderr, "ERROR: Receive error: %v\n", err)
			}
			continue
		}

		frames := msg.Frames
		if len(frames) >= 3 {
			senderID := frames[0]
			// Track sender for output forwarding
			w.lastSenderID = senderID
			// frames[1] is empty delimiter
			messageData := frames[2]
			w.handleMessage(messageData, senderID)
		}
	}
}

// handleMessage processes an incoming message
func (w *ChildWorker) handleMessage(data []byte, senderID []byte) {
	msg, err := Unpack(data)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Failed to unpack message: %v\n", err)
		return
	}

	// Validate app
	if msg.App != AppName {
		return
	}

	// Check namespace
	if msg.Namespace != "" && msg.Namespace != w.Namespace {
		return
	}

	// Handle message types
	switch msg.Type {
	case string(MessageTypeCall):
		w.handleFunctionCall(msg, senderID)
	case string(MessageTypeHeartbeat):
		w.handleHeartbeat(msg, senderID)
	case string(MessageTypeShutdown):
		w.running = false
		close(w.done)
	}
}

// handleHeartbeat handles a heartbeat message - echo it back immediately
func (w *ChildWorker) handleHeartbeat(msg *ComlinkMessage, senderID []byte) {
	// Extract original timestamp from request
	var originalTs float64
	if msg.Metadata != nil {
		if ts, ok := msg.Metadata["hb_timestamp"].(float64); ok {
			originalTs = ts
		}
	}

	// Create heartbeat response with same ID and original timestamp
	response := CreateHeartbeatResponse(msg.ID, originalTs)

	// Send response with ROUTER envelope
	data, err := response.Pack()
	if err != nil {
		fmt.Fprintf(os.Stderr, "CRITICAL: Failed to pack heartbeat response: %v\n", err)
		return
	}

	zmqMsg := zmq.NewMsgFrom(senderID, []byte{}, data)
	if err := w.socket.Send(zmqMsg); err != nil {
		// Don't log heartbeat failures - they're frequent and noisy
	}
}

// unwrapInterface unwraps an interface{} value to get the underlying value.
// This handles msgpack decoding where all values are decoded as interface{}.
func unwrapInterface(value reflect.Value) reflect.Value {
	// Only unwrap if it's an interface with a valid underlying value
	if value.Kind() == reflect.Interface && value.Elem().IsValid() {
		return value.Elem()
	}
	return value
}

// convertArg converts a reflect.Value to match the expected target type.
// This handles msgpack type differences between languages (e.g., int64 from Python -> int in Go).
func convertArg(value reflect.Value, target reflect.Type) reflect.Value {
	// If already the right type, return as-is
	if value.Type() == target {
		return value
	}

	// Handle nil/invalid
	if !value.IsValid() {
		return reflect.Zero(target)
	}

	// Unwrap interface{} values from msgpack decoding
	value = unwrapInterface(value)

	srcKind := value.Kind()

	// Handle integer conversions (msgpack sends int64, Go methods may expect int/int32/etc)
	if isIntegerKind(srcKind) && isIntegerKind(target.Kind()) {
		srcInt := value.Int()
		return reflect.ValueOf(srcInt).Convert(target)
	}

	// Handle float->int conversion (msgpack may send floats for small integers)
	if srcKind == reflect.Float64 || srcKind == reflect.Float32 {
		if isIntegerKind(target.Kind()) {
			srcFloat := value.Float()
			return reflect.ValueOf(int64(srcFloat)).Convert(target)
		}
	}

	// Handle int->float conversion
	if isIntegerKind(srcKind) && (target.Kind() == reflect.Float64 || target.Kind() == reflect.Float32) {
		srcInt := value.Int()
		return reflect.ValueOf(float64(srcInt)).Convert(target)
	}

	// Handle interface{} / any - just return the value
	if target == reflect.TypeOf((*any)(nil)).Elem() {
		return value
	}

	// Handle slices/arrays
	if target.Kind() == reflect.Slice || target.Kind() == reflect.Array {
		if srcKind == reflect.Slice || srcKind == reflect.Array {
			return convertSlice(value, target)
		}
	}

	// Handle maps
	if target.Kind() == reflect.Map && srcKind == reflect.Map {
		return convertMap(value, target)
	}

	// Try direct conversion if possible
	if value.CanConvert(target) {
		return value.Convert(target)
	}

	// Fallback: return as-is and let the call fail with a clear error
	return value
}

// isIntegerKind checks if a kind is an integer type
func isIntegerKind(k reflect.Kind) bool {
	return k == reflect.Int || k == reflect.Int8 || k == reflect.Int16 ||
		k == reflect.Int32 || k == reflect.Int64 ||
		k == reflect.Uint || k == reflect.Uint8 || k == reflect.Uint16 ||
		k == reflect.Uint32 || k == reflect.Uint64
}

// convertSlice converts a slice/array to the target slice type
func convertSlice(value reflect.Value, target reflect.Type) reflect.Value {
	length := value.Len()
	targetElemType := target.Elem()
	result := reflect.MakeSlice(target, length, length)

	for i := 0; i < length; i++ {
		elem := value.Index(i)
		// Unwrap interface{} from msgpack decoding before conversion
		elem = unwrapInterface(elem)
		converted := convertArg(elem, targetElemType)
		result.Index(i).Set(converted)
	}

	return result
}

// convertMap converts a map to the target map type
func convertMap(value reflect.Value, target reflect.Type) reflect.Value {
	targetKeyType := target.Key()
	targetValueType := target.Elem()
	result := reflect.MakeMap(target)

	for _, key := range value.MapKeys() {
		// Unwrap interface{} from msgpack decoding before conversion
		key = unwrapInterface(key)
		convertedKey := convertArg(key, targetKeyType)

		value := value.MapIndex(key)
		value = unwrapInterface(value)
		convertedValue := convertArg(value, targetValueType)
		result.SetMapIndex(convertedKey, convertedValue)
	}

	return result
}

// handleFunctionCall processes a function call message
func (w *ChildWorker) handleFunctionCall(msg *ComlinkMessage, senderID []byte) {
	var response *ComlinkMessage

	// Validate message
	if msg.Function == "" || msg.ID == "" {
		response = CreateError("Message missing 'function' or 'id' field", msg.ID)
		w.sendResponse(response, senderID)
		return
	}

	// Check for private methods
	if len(msg.Function) > 0 && msg.Function[0] == '_' {
		response = CreateError(fmt.Sprintf("Cannot call private method '%s'", msg.Function), msg.ID)
		w.sendResponse(response, senderID)
		return
	}

	// Determine target for method lookup: use self if set (for embedded structs), else use w
	target := any(w)
	if w.self != nil {
		target = w.self
	}

	// Find and call the method using reflection
	method := reflect.ValueOf(target).MethodByName(msg.Function)
	if !method.IsValid() {
		response = CreateError(fmt.Sprintf("Function '%s' not found", msg.Function), msg.ID)
		w.sendResponse(response, senderID)
		return
	}

	// Prepare arguments with type conversion
	methodType := method.Type()
	args := make([]reflect.Value, len(msg.Args))

	for i, arg := range msg.Args {
		if i < methodType.NumIn() {
			// Convert argument to match expected parameter type
			args[i] = convertArg(reflect.ValueOf(arg), methodType.In(i))
		} else {
			args[i] = reflect.ValueOf(arg)
		}
	}

	// Call the method
	results := method.Call(args)

	// Handle result (expect single return value or error)
	if len(results) == 0 {
		response = CreateResponse(nil, msg.ID)
	} else if len(results) == 1 {
		// Single return value - check if it's an error
		result := results[0].Interface()
		if err, ok := result.(error); ok && err != nil {
			response = CreateError(err.Error(), msg.ID)
		} else {
			response = CreateResponse(result, msg.ID)
		}
	} else if len(results) == 2 {
		// Common Go pattern: (result, error)
		if err, ok := results[1].Interface().(error); ok && err != nil {
			response = CreateError(err.Error(), msg.ID)
		} else {
			response = CreateResponse(results[0].Interface(), msg.ID)
		}
	} else {
		response = CreateError("unexpected number of return values", msg.ID)
	}

	w.sendResponse(response, senderID)
}

// sendResponse sends a response message with ROUTER envelope
func (w *ChildWorker) sendResponse(msg *ComlinkMessage, senderID []byte) {
	data, err := msg.Pack()
	if err != nil {
		fmt.Fprintf(os.Stderr, "CRITICAL: Failed to pack response: %v\n", err)
		return
	}

	// ROUTER envelope: [sender_id, empty_frame, response_data]
	zmqMsg := zmq.NewMsgFrom(senderID, []byte{}, data)
	if err := w.socket.Send(zmqMsg); err != nil {
		fmt.Fprintf(os.Stderr, "CRITICAL: Failed to send response: %v\n", err)
	}
}

// Stop stops the worker and cleans up resources
func (w *ChildWorker) Stop() {
	w.running = false

	// Restore stdout/stderr
	if w.originalStdout != nil {
		os.Stdout = w.originalStdout
	}
	if w.originalStderr != nil {
		os.Stderr = w.originalStderr
	}

	// Close pipes
	if w.stdoutPipe != nil {
		w.stdoutPipe.Close()
	}
	if w.stderrPipe != nil {
		w.stderrPipe.Close()
	}

	// Cleanup registry entry
	if w.ServiceID != "" {
		if err := Unregister(w.ServiceID); err != nil {
			fmt.Fprintf(os.Stderr, "Warning: Failed to unregister service: %v\n", err)
		}
	}

	// Close socket
	if w.socket != nil {
		w.socket.Close()
	}
}

// Run starts the worker with signal handling (blocking)
func (w *ChildWorker) Run() {
	// Setup signal handlers
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigChan
		fmt.Fprintln(os.Stderr, "Received signal, shutting down...")
		w.Stop()
	}()

	// Start the worker
	if err := w.Start(); err != nil {
		fmt.Fprintf(os.Stderr, "FATAL: %v\n", err)
		os.Exit(1)
	}

	// Wait for shutdown
	<-w.done
	w.Stop()
}

// ListFunctions returns available callable public methods
func (w *ChildWorker) ListFunctions() []string {
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

// isBaseMethod checks if a method is from the ChildWorker base class
func isBaseMethod(name string) bool {
	baseMethods := map[string]bool{
		"Start":         true,
		"Stop":          true,
		"Run":           true,
		"ListFunctions": true,
	}
	return baseMethods[name]
}

// GetPort returns the current port (for debugging)
func (w *ChildWorker) GetPort() int {
	return w.port
}

// IsRunning returns whether the worker is running
func (w *ChildWorker) IsRunning() bool {
	return w.running
}

// Done returns a channel that closes when the worker stops
func (w *ChildWorker) Done() <-chan struct{} {
	return w.done
}

// SendOutput sends stdout/stderr output to parent
func (w *ChildWorker) SendOutput(output string, msgType MessageType) {
	msg := CreateOutput(output, msgType)
	data, err := msg.Pack()
	if err != nil {
		return
	}

	// Note: In ROUTER mode, we need a sender_id to respond to
	// For output messages, we broadcast (this is a simplification)
	// In practice, output is typically handled via redirected stdout/stderr
	_ = data
}
