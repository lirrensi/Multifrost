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

	// Timeout configuration
	DefaultTimeout time.Duration

	// Heartbeat configuration
	HeartbeatInterval  time.Duration
	HeartbeatTimeout   time.Duration
	HeartbeatMaxMisses int

	// Circuit breaker state
	consecutiveFailures int
	circuitOpen         bool

	// Heartbeat state
	pendingHeartbeats          map[string]chan bool
	consecutiveHeartbeatMisses int
	lastHeartbeatRttMs         float64
	heartbeatRunning           bool
	heartbeatStopChan          chan struct{}

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

	// Metrics
	metrics *Metrics

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
	DefaultTimeout     time.Duration
	HeartbeatInterval  time.Duration
	HeartbeatTimeout   time.Duration
	HeartbeatMaxMisses int
	EnableMetrics      bool
}

// NewParentWorker creates a new ParentWorker with the given config
func NewParentWorker(config ParentWorkerConfig) *ParentWorker {
	if config.Executable == "" {
		config.Executable = "go" // Default to Go
	}
	if config.MaxRestartAttempts == 0 {
		config.MaxRestartAttempts = 5
	}
	if config.HeartbeatInterval == 0 {
		config.HeartbeatInterval = 5 * time.Second
	}
	if config.HeartbeatTimeout == 0 {
		config.HeartbeatTimeout = 3 * time.Second
	}
	if config.HeartbeatMaxMisses == 0 {
		config.HeartbeatMaxMisses = 3
	}

	pw := &ParentWorker{
		scriptPath:         config.ScriptPath,
		executable:         config.Executable,
		serviceID:          config.ServiceID,
		port:               config.Port,
		isSpawnMode:        config.ScriptPath != "",
		AutoRestart:        config.AutoRestart,
		MaxRestartAttempts: config.MaxRestartAttempts,
		DefaultTimeout:     config.DefaultTimeout,
		HeartbeatInterval:  config.HeartbeatInterval,
		HeartbeatTimeout:   config.HeartbeatTimeout,
		HeartbeatMaxMisses: config.HeartbeatMaxMisses,
		pendingRequests:    make(map[string]*pendingRequest),
		pendingHeartbeats:  make(map[string]chan bool),
		done:               make(chan struct{}),
		stopChan:           make(chan struct{}),
		heartbeatStopChan:  make(chan struct{}),
	}

	// Setup metrics if enabled
	if config.EnableMetrics {
		pw.metrics = NewMetrics(1000, 60.0)
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
		ScriptPath:    scriptPath,
		Executable:    exe,
		Port:          port,
		EnableMetrics: true,
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
		ServiceID:     serviceID,
		Port:          port,
		EnableMetrics: true,
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

	// Start heartbeat loop (spawn mode only)
	if pw.isSpawnMode && pw.HeartbeatInterval > 0 {
		pw.heartbeatRunning = true
		go pw.heartbeatLoop()
	}

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
	case string(MessageTypeHeartbeat):
		pw.handleHeartbeatResponse(msg)
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
		pw.recordSuccess()
	} else {
		pending.reject(&RemoteCallError{Message: msg.Error})
		pw.recordFailure()
	}
}

// handleHeartbeatResponse handles a heartbeat response from child
func (pw *ParentWorker) handleHeartbeatResponse(msg *ComlinkMessage) {
	pw.mu.Lock()
	responseChan, exists := pw.pendingHeartbeats[msg.ID]
	if exists {
		delete(pw.pendingHeartbeats, msg.ID)
	}
	pw.mu.Unlock()

	if !exists || responseChan == nil {
		return
	}

	// Calculate RTT from original timestamp
	if msg.Metadata != nil {
		if originalTs, ok := msg.Metadata["hb_timestamp"].(float64); ok {
			rttMs := (float64(time.Now().UnixNano())/1e9 - originalTs) * 1000
			pw.lastHeartbeatRttMs = rttMs
			if pw.metrics != nil {
				pw.metrics.RecordHeartbeatRtt(rttMs)
			}
		}
	}

	// Reset consecutive misses on successful response
	pw.consecutiveHeartbeatMisses = 0

	// Signal heartbeat success
	select {
	case responseChan <- true:
	default:
	}
}

// CallFunction calls a function on the remote worker
func (pw *ParentWorker) CallFunction(ctx context.Context, functionName string, args ...any) (any, error) {
	return pw.CallFunctionWithTimeout(ctx, functionName, 0, args...)
}

// CallFunctionWithTimeout calls a function with a specific timeout
func (pw *ParentWorker) CallFunctionWithTimeout(ctx context.Context, functionName string, timeout time.Duration, args ...any) (any, error) {
	// Check circuit breaker
	if pw.circuitOpen {
		return nil, &CircuitOpenError{ConsecutiveFailures: pw.consecutiveFailures}
	}

	if !pw.running {
		return nil, fmt.Errorf("worker is not running")
	}

	// Use default timeout if not specified
	effectiveTimeout := timeout
	if effectiveTimeout == 0 {
		effectiveTimeout = pw.DefaultTimeout
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

	// Start metrics tracking
	var startTime time.Time
	if pw.metrics != nil {
		startTime = pw.metrics.StartRequest(msg.ID, functionName, "default")
	}

	// Send message with retry logic
	data, err := msg.Pack()
	if err != nil {
		return nil, fmt.Errorf("failed to pack message: %w", err)
	}

	// DEALER envelope: [empty_frame, message_data]
	zmqMsg := zmq.NewMsgFrom([]byte{}, data)
	if err := pw.sendMessageWithRetry(zmqMsg, 5); err != nil {
		return nil, fmt.Errorf("failed to send message: %w", err)
	}

	// Wait for response or timeout
	var timeoutChan <-chan time.Time
	if effectiveTimeout > 0 {
		timeoutChan = time.After(effectiveTimeout)
	}

	select {
	case <-ctx.Done():
		if pw.metrics != nil {
			pw.metrics.EndRequest(startTime, msg.ID, false, "context cancelled")
		}
		pw.recordFailure()
		return nil, ctx.Err()
	case <-timeoutChan:
		if pw.metrics != nil {
			pw.metrics.EndRequest(startTime, msg.ID, false, "timeout")
		}
		pw.recordFailure()
		return nil, fmt.Errorf("function '%s' timed out after %v", functionName, effectiveTimeout)
	case result := <-resultChan:
		if pw.metrics != nil {
			pw.metrics.EndRequest(startTime, msg.ID, true, "")
		}
		pw.recordSuccess()
		return result, nil
	case err := <-errorChan:
		if pw.metrics != nil {
			pw.metrics.EndRequest(startTime, msg.ID, false, err.Error())
		}
		pw.recordFailure()
		return nil, err
	}
}

// sendMessageWithRetry sends a message with retry logic
func (pw *ParentWorker) sendMessageWithRetry(msg zmq.Msg, maxRetries int) error {
	for attempt := 0; attempt < maxRetries; attempt++ {
		if err := pw.socket.Send(msg); err == nil {
			return nil // Success
		} else {
			// Check if it's a retryable error (socket busy)
			if attempt < maxRetries-1 {
				time.Sleep(100 * time.Millisecond)
				continue
			}
			return fmt.Errorf("socket busy after %d retries: %w", maxRetries, err)
		}
	}
	return fmt.Errorf("failed to send message after %d retries", maxRetries)
}

// recordFailure records a failure for circuit breaker tracking
func (pw *ParentWorker) recordFailure() {
	pw.mu.Lock()
	defer pw.mu.Unlock()

	pw.consecutiveFailures++
	if pw.consecutiveFailures >= pw.MaxRestartAttempts {
		pw.circuitOpen = true
		if pw.metrics != nil {
			pw.metrics.RecordCircuitBreakerTrip()
		}
	}
}

// recordSuccess records a success, resetting circuit breaker
func (pw *ParentWorker) recordSuccess() {
	pw.mu.Lock()
	defer pw.mu.Unlock()

	if pw.consecutiveFailures > 0 {
		pw.consecutiveFailures = 0
		if pw.circuitOpen {
			pw.circuitOpen = false
			if pw.metrics != nil {
				pw.metrics.RecordCircuitBreakerReset()
			}
		}
	}
}

// heartbeatLoop periodically sends heartbeats to child process
func (pw *ParentWorker) heartbeatLoop() {
	// Wait for initial connection
	time.Sleep(1 * time.Second)

	for pw.running && pw.heartbeatRunning {
		select {
		case <-pw.heartbeatStopChan:
			return
		default:
		}

		// Only send heartbeats in spawn mode (we own the child)
		if !pw.isSpawnMode {
			time.Sleep(pw.HeartbeatInterval)
			continue
		}

		// Check if child is still running
		if pw.process != nil && pw.process.Process != nil {
			// Check if process has exited
			if pw.process.ProcessState != nil && pw.process.ProcessState.Exited() {
				// Child died, let message loop handle it
				return
			}
		}

		// Create heartbeat message
		heartbeat := CreateHeartbeat("")

		// Create channel for response
		responseChan := make(chan bool, 1)
		pw.mu.Lock()
		pw.pendingHeartbeats[heartbeat.ID] = responseChan
		pw.mu.Unlock()

		// Send heartbeat
		data, _ := heartbeat.Pack()
		zmqMsg := zmq.NewMsgFrom([]byte{}, data)
		if err := pw.sendMessageWithRetry(zmqMsg, 3); err != nil {
			// Send failed, count as miss
			pw.mu.Lock()
			delete(pw.pendingHeartbeats, heartbeat.ID)
			pw.mu.Unlock()

			pw.consecutiveHeartbeatMisses++
			fmt.Printf("Heartbeat send failed (%d/%d)\n", pw.consecutiveHeartbeatMisses, pw.HeartbeatMaxMisses)

			if pw.consecutiveHeartbeatMisses >= pw.HeartbeatMaxMisses {
				fmt.Printf("Heartbeat timeout after %d consecutive misses\n", pw.consecutiveHeartbeatMisses)
				pw.recordFailure()
				return
			}

			time.Sleep(pw.HeartbeatInterval)
			continue
		}

		// Wait for response with timeout
		select {
		case <-responseChan:
			// Success - RTT already recorded in handleHeartbeatResponse
		case <-time.After(pw.HeartbeatTimeout):
			// Heartbeat timed out
			pw.mu.Lock()
			delete(pw.pendingHeartbeats, heartbeat.ID)
			pw.mu.Unlock()

			pw.consecutiveHeartbeatMisses++
			if pw.metrics != nil {
				pw.metrics.RecordHeartbeatMiss()
			}

			fmt.Printf("Heartbeat missed (%d/%d)\n", pw.consecutiveHeartbeatMisses, pw.HeartbeatMaxMisses)

			// Check if too many misses
			if pw.consecutiveHeartbeatMisses >= pw.HeartbeatMaxMisses {
				fmt.Printf("Heartbeat timeout after %d consecutive misses\n", pw.consecutiveHeartbeatMisses)
				pw.recordFailure()
				return
			}
		}

		// Wait for next interval
		time.Sleep(pw.HeartbeatInterval)
	}
}

// Close stops the worker and cleans up resources
func (pw *ParentWorker) Close() error {
	if pw.closed {
		return nil
	}
	pw.closed = true
	pw.running = false
	pw.heartbeatRunning = false

	// Stop heartbeat loop
	close(pw.heartbeatStopChan)

	// Cancel pending heartbeats
	pw.mu.Lock()
	for _, hbChan := range pw.pendingHeartbeats {
		close(hbChan)
	}
	pw.pendingHeartbeats = make(map[string]chan bool)
	pw.mu.Unlock()

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

// IsHealthy returns whether the worker is healthy (circuit breaker not tripped)
func (pw *ParentWorker) IsHealthy() bool {
	return !pw.circuitOpen && pw.running
}

// CircuitOpen returns whether the circuit breaker is open
func (pw *ParentWorker) CircuitOpen() bool {
	return pw.circuitOpen
}

// LastHeartbeatRttMs returns the last heartbeat round-trip time in milliseconds
func (pw *ParentWorker) LastHeartbeatRttMs() float64 {
	return pw.lastHeartbeatRttMs
}

// Metrics returns the metrics collector
func (pw *ParentWorker) Metrics() *Metrics {
	return pw.metrics
}

// GetPort returns the current port
func (pw *ParentWorker) GetPort() int {
	return pw.port
}

// IsRunning returns whether the worker is running
func (pw *ParentWorker) IsRunning() bool {
	return pw.running
}

// IsSpawnMode returns whether the worker was spawned (vs connect mode)
func (pw *ParentWorker) IsSpawnMode() bool {
	return pw.isSpawnMode
}

// ScriptPath returns the script path for spawned workers
func (pw *ParentWorker) ScriptPath() string {
	return pw.scriptPath
}

// ServiceID returns the service ID for connect mode workers
func (pw *ParentWorker) ServiceID() string {
	return pw.serviceID
}

// ConsecutiveFailures returns the number of consecutive failures
func (pw *ParentWorker) ConsecutiveFailures() int {
	return pw.consecutiveFailures
}

// ConsecutiveHeartbeatMisses returns the number of consecutive heartbeat misses
func (pw *ParentWorker) ConsecutiveHeartbeatMisses() int {
	return pw.consecutiveHeartbeatMisses
}

// SyncProxy provides synchronous method calling
type SyncProxy struct {
	worker *ParentWorker
}

// Call calls a function synchronously
func (p *SyncProxy) Call(functionName string, args ...any) (any, error) {
	return p.worker.CallFunction(context.Background(), functionName, args...)
}

// CallWithTimeout calls a function with a timeout
func (p *SyncProxy) CallWithTimeout(functionName string, timeout time.Duration, args ...any) (any, error) {
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	return p.worker.CallFunction(ctx, functionName, args...)
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
