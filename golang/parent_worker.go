package multifrost

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"sync"
	"time"

	zmq "github.com/go-zeromq/zmq4"
)

// ParentWorker manages child processes and provides IPC communication.
// Supports two modes:
//   - Spawn mode: Parent spawns child process (owns the child)
//   - Connect mode: Parent connects to existing service via ServiceRegistry
type ParentWorker struct {
	// Configuration
	scriptPath  string
	executable  string
	serviceID   string
	port        int
	isSpawnMode bool

	// Auto-restart options
	AutoRestart        bool
	MaxRestartAttempts int
	restartCount       int

	// ZMQ state
	socket zmq.Socket

	// Process management
	process *exec.Cmd
	running bool
	closed  bool

	// Pending requests
	pendingRequests map[string]*pendingRequest
	mu              sync.RWMutex

	// Channels
	done     chan struct{}
	stopChan chan struct{}

	// Proxy interfaces
	Call  *SyncProxy
	ACall *AsyncProxy
}

type pendingRequest struct {
	resolve func(any)
	reject  func(error)
	ctx     context.Context
}

// ParentWorkerConfig holds configuration for creating a ParentWorker
type ParentWorkerConfig struct {
	ScriptPath         string
	Executable         string
	ServiceID          string
	Port               int
	AutoRestart        bool
	MaxRestartAttempts int
}

// NewParentWorker creates a new ParentWorker with the given config
func NewParentWorker(config ParentWorkerConfig) *ParentWorker {
	if config.Executable == "" {
		config.Executable = "go" // Default to Go
	}
	if config.MaxRestartAttempts == 0 {
		config.MaxRestartAttempts = 5
	}

	pw := &ParentWorker{
		scriptPath:         config.ScriptPath,
		executable:         config.Executable,
		serviceID:          config.ServiceID,
		port:               config.Port,
		isSpawnMode:        config.ScriptPath != "",
		AutoRestart:        config.AutoRestart,
		MaxRestartAttempts: config.MaxRestartAttempts,
		pendingRequests:    make(map[string]*pendingRequest),
		done:               make(chan struct{}),
		stopChan:           make(chan struct{}),
	}

	// Setup proxy interfaces
	pw.Call = &SyncProxy{worker: pw}
	pw.ACall = &AsyncProxy{worker: pw}

	return pw
}

// Spawn creates a ParentWorker in spawn mode (owns the child process)
func Spawn(scriptPath string, executable ...string) *ParentWorker {
	exe := "go"
	if len(executable) > 0 && executable[0] != "" {
		exe = executable[0]
	}

	port := findFreePort()
	return NewParentWorker(ParentWorkerConfig{
		ScriptPath: scriptPath,
		Executable: exe,
		Port:       port,
	})
}

// Connect creates a ParentWorker in connect mode (connects to existing service)
func Connect(ctx context.Context, serviceID string, timeout ...time.Duration) (*ParentWorker, error) {
	t := DiscoveryTimeout
	if len(timeout) > 0 {
		t = timeout[0]
	}

	port, err := Discover(serviceID, t)
	if err != nil {
		return nil, fmt.Errorf("failed to discover service '%s': %w", serviceID, err)
	}

	return NewParentWorker(ParentWorkerConfig{
		ServiceID: serviceID,
		Port:      port,
	}), nil
}

// Start starts the parent worker and child process (if spawn mode)
func (pw *ParentWorker) Start() error {
	// Setup ZeroMQ DEALER socket
	pw.socket = zmq.NewDealer(context.Background())

	// Bind or connect based on mode
	if pw.isSpawnMode {
		endpoint := fmt.Sprintf("tcp://*:%d", pw.port)
		if err := pw.socket.Listen(endpoint); err != nil {
			return fmt.Errorf("failed to bind to %s: %w", endpoint, err)
		}

		// Start child process
		if err := pw.startChildProcess(); err != nil {
			return fmt.Errorf("failed to start child process: %w", err)
		}
	} else {
		endpoint := fmt.Sprintf("tcp://localhost:%d", pw.port)
		if err := pw.socket.Dial(endpoint); err != nil {
			return fmt.Errorf("failed to connect to %s: %w", endpoint, err)
		}
	}

	pw.running = true

	// Start message loop
	go pw.messageLoop()

	// Wait for connection
	time.Sleep(500 * time.Millisecond)

	return nil
}

// startChildProcess spawns the child process
func (pw *ParentWorker) startChildProcess() error {
	cmd := exec.Command(pw.executable, pw.scriptPath)
	cmd.Env = append(os.Environ(),
		fmt.Sprintf("COMLINK_ZMQ_PORT=%d", pw.port),
		"COMLINK_WORKER_MODE=1",
	)

	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Start(); err != nil {
		return fmt.Errorf("failed to start process: %w", err)
	}

	pw.process = cmd

	// Quick health check
	time.Sleep(200 * time.Millisecond)
	if pw.process.Process == nil {
		return fmt.Errorf("child process failed to start")
	}

	return nil
}

// messageLoop handles incoming ZMQ messages
func (pw *ParentWorker) messageLoop() {
	for pw.running {
		select {
		case <-pw.stopChan:
			return
		default:
		}

		// DEALER socket receives: [empty_frame, message_data]
		msg, err := pw.socket.Recv()
		if err != nil {
			if pw.running {
				// Check if it's a timeout or no message
				time.Sleep(10 * time.Millisecond)
			}
			continue
		}

		frames := msg.Frames
		if len(frames) >= 2 {
			// frames[0] is empty delimiter
			messageData := frames[1]
			pw.handleMessage(messageData)
		}
	}
}

// handleMessage processes an incoming message
func (pw *ParentWorker) handleMessage(data []byte) {
	msg, err := Unpack(data)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Failed to unpack message: %v\n", err)
		return
	}

	// Validate app
	if msg.App != AppName || msg.ID == "" {
		return
	}

	// Handle message types
	switch msg.Type {
	case string(MessageTypeResponse), string(MessageTypeError):
		pw.handleResponse(msg)
	case string(MessageTypeStdout):
		if msg.Output != "" {
			name := pw.scriptPath
			if name == "" {
				name = pw.serviceID
			}
			if name == "" {
				name = "worker"
			}
			fmt.Printf("[%s STDOUT]: %s\n", name, msg.Output)
		}
	case string(MessageTypeStderr):
		if msg.Output != "" {
			name := pw.scriptPath
			if name == "" {
				name = pw.serviceID
			}
			if name == "" {
				name = "worker"
			}
			fmt.Fprintf(os.Stderr, "[%s STDERR]: %s\n", name, msg.Output)
		}
	}
}

// handleResponse handles a response message
func (pw *ParentWorker) handleResponse(msg *ComlinkMessage) {
	pw.mu.Lock()
	pending, exists := pw.pendingRequests[msg.ID]
	if exists {
		delete(pw.pendingRequests, msg.ID)
	}
	pw.mu.Unlock()

	if !exists || pending == nil {
		return
	}

	// Check context cancellation
	if pending.ctx != nil {
		select {
		case <-pending.ctx.Done():
			return
		default:
		}
	}

	if msg.Type == string(MessageTypeResponse) {
		pending.resolve(msg.Result)
	} else {
		pending.reject(&RemoteCallError{Message: msg.Error})
	}
}

// CallFunction calls a function on the remote worker
func (pw *ParentWorker) CallFunction(ctx context.Context, functionName string, args ...any) (any, error) {
	return pw.CallFunctionWithTimeout(ctx, functionName, 0, args...)
}

// CallFunctionWithTimeout calls a function with a specific timeout
func (pw *ParentWorker) CallFunctionWithTimeout(ctx context.Context, functionName string, timeout time.Duration, args ...any) (any, error) {
	if !pw.running {
		return nil, fmt.Errorf("worker is not running")
	}

	// Create message
	msg := CreateCall(functionName, args, "default", "", "")

	// Create pending request
	resultChan := make(chan any, 1)
	errorChan := make(chan error, 1)

	pw.mu.Lock()
	pw.pendingRequests[msg.ID] = &pendingRequest{
		resolve: func(v any) { resultChan <- v },
		reject:  func(e error) { errorChan <- e },
		ctx:     ctx,
	}
	pw.mu.Unlock()

	// Cleanup on exit
	defer func() {
		pw.mu.Lock()
		delete(pw.pendingRequests, msg.ID)
		pw.mu.Unlock()
	}()

	// Send message
	data, err := msg.Pack()
	if err != nil {
		return nil, fmt.Errorf("failed to pack message: %w", err)
	}

	// DEALER envelope: [empty_frame, message_data]
	zmqMsg := zmq.NewMsgFrom([]byte{}, data)
	if err := pw.socket.Send(zmqMsg); err != nil {
		return nil, fmt.Errorf("failed to send message: %w", err)
	}

	// Wait for response or timeout
	var timeoutChan <-chan time.Time
	if timeout > 0 {
		timeoutChan = time.After(timeout)
	}

	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-timeoutChan:
		return nil, fmt.Errorf("function '%s' timed out after %v", functionName, timeout)
	case result := <-resultChan:
		return result, nil
	case err := <-errorChan:
		return nil, err
	}
}

// Close stops the worker and cleans up resources
func (pw *ParentWorker) Close() error {
	if pw.closed {
		return nil
	}
	pw.closed = true
	pw.running = false

	// Signal stop
	close(pw.stopChan)

	// Cancel pending requests
	pw.mu.Lock()
	for _, pending := range pw.pendingRequests {
		if pending != nil && pending.reject != nil {
			pending.reject(fmt.Errorf("worker shutting down"))
		}
	}
	pw.pendingRequests = make(map[string]*pendingRequest)
	pw.mu.Unlock()

	// Close socket
	if pw.socket != nil {
		pw.socket.Close()
	}

	// Terminate process (spawn mode only)
	if pw.isSpawnMode && pw.process != nil && pw.process.Process != nil {
		pw.process.Process.Signal(os.Interrupt)

		// Wait for graceful shutdown
		done := make(chan error, 1)
		go func() {
			done <- pw.process.Wait()
		}()

		select {
		case <-done:
			// Process exited
		case <-time.After(2 * time.Second):
			// Force kill
			pw.process.Process.Kill()
		}
	}

	return nil
}

// GetPort returns the current port
func (pw *ParentWorker) GetPort() int {
	return pw.port
}

// IsRunning returns whether the worker is running
func (pw *ParentWorker) IsRunning() bool {
	return pw.running
}

// SyncProxy provides synchronous method calling
type SyncProxy struct {
	worker *ParentWorker
}

// Call calls a function synchronously
func (p *SyncProxy) Call(functionName string, args ...any) (any, error) {
	return p.worker.CallFunction(context.Background(), functionName, args...)
}

// AsyncProxy provides asynchronous method calling
type AsyncProxy struct {
	worker *ParentWorker
}

// Call calls a function asynchronously
func (p *AsyncProxy) Call(ctx context.Context, functionName string, args ...any) (any, error) {
	return p.worker.CallFunction(ctx, functionName, args...)
}

// CallWithTimeout calls a function with a timeout
func (p *AsyncProxy) CallWithTimeout(functionName string, timeout time.Duration, args ...any) (any, error) {
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	return p.worker.CallFunction(ctx, functionName, args...)
}
