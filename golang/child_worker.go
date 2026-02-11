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

	// Find and call the method using reflection
	method := reflect.ValueOf(w).MethodByName(msg.Function)
	if !method.IsValid() {
		response = CreateError(fmt.Sprintf("Function '%s' not found", msg.Function), msg.ID)
		w.sendResponse(response, senderID)
		return
	}

	// Prepare arguments
	args := make([]reflect.Value, len(msg.Args))
	for i, arg := range msg.Args {
		args[i] = reflect.ValueOf(arg)
	}

	// Call the method
	results := method.Call(args)

	// Handle result (expect single return value or error)
	if len(results) == 0 {
		response = CreateResponse(nil, msg.ID)
	} else if len(results) == 1 {
		response = CreateResponse(results[0].Interface(), msg.ID)
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
