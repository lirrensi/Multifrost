package multifrost

import (
	"sort"
	"sync"
	"time"
)

// MetricsSnapshot represents a point-in-time snapshot of all metrics
type MetricsSnapshot struct {
	// Counters
	RequestsTotal   int `json:"requests_total"`
	RequestsSuccess int `json:"requests_success"`
	RequestsFailed  int `json:"requests_failed"`

	// Latency (milliseconds)
	LatencyAvgMs float64 `json:"latency_avg_ms"`
	LatencyP50Ms float64 `json:"latency_p50_ms"`
	LatencyP95Ms float64 `json:"latency_p95_ms"`
	LatencyP99Ms float64 `json:"latency_p99_ms"`
	LatencyMinMs float64 `json:"latency_min_ms"`
	LatencyMaxMs float64 `json:"latency_max_ms"`

	// Queue
	QueueDepth    int `json:"queue_depth"`
	QueueMaxDepth int `json:"queue_max_depth"`

	// Circuit breaker
	CircuitBreakerTrips int    `json:"circuit_breaker_trips"`
	CircuitBreakerState string `json:"circuit_breaker_state"`

	// Heartbeat
	HeartbeatRttAvgMs  float64 `json:"heartbeat_rtt_avg_ms"`
	HeartbeatRttLastMs float64 `json:"heartbeat_rtt_last_ms"`
	HeartbeatMisses    int     `json:"heartbeat_misses"`

	// Time window
	WindowSeconds float64 `json:"window_seconds"`

	// Timestamp
	Timestamp time.Time `json:"timestamp"`
}

// Metrics is a thread-safe metrics collector for ParentWorker
type Metrics struct {
	mu sync.RWMutex

	// Configuration
	maxLatencySamples int
	windowSeconds     float64

	// Counters
	requestsTotal       int
	requestsSuccess     int
	requestsFailed      int
	circuitBreakerTrips int
	circuitBreakerState string

	// Queue tracking
	queueDepth    int
	queueMaxDepth int

	// Latency samples (circular buffer via slice)
	latencies []float64

	// Heartbeat RTT samples
	heartbeatRtts   []float64
	heartbeatMisses int
}

// NewMetrics creates a new Metrics instance
func NewMetrics(maxLatencySamples int, windowSeconds float64) *Metrics {
	if maxLatencySamples <= 0 {
		maxLatencySamples = 1000
	}
	if windowSeconds <= 0 {
		windowSeconds = 60.0
	}

	return &Metrics{
		maxLatencySamples:   maxLatencySamples,
		windowSeconds:       windowSeconds,
		circuitBreakerState: "closed",
		latencies:           make([]float64, 0, maxLatencySamples),
		heartbeatRtts:       make([]float64, 0, 100),
	}
}

// StartRequest starts tracking a request
// Returns start timestamp for later EndRequest() call
func (m *Metrics) StartRequest(requestID, function, namespace string) time.Time {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.requestsTotal++
	m.queueDepth++
	if m.queueDepth > m.queueMaxDepth {
		m.queueMaxDepth = m.queueDepth
	}

	return time.Now()
}

// EndRequest ends tracking a request
// Returns latency in milliseconds
func (m *Metrics) EndRequest(startTime time.Time, requestID string, success bool, errorMsg string) float64 {
	latencyMs := float64(time.Since(startTime).Milliseconds())

	m.mu.Lock()
	defer m.mu.Unlock()

	m.queueDepth--

	if success {
		m.requestsSuccess++
	} else {
		m.requestsFailed++
	}

	// Store latency sample (circular buffer)
	if len(m.latencies) >= m.maxLatencySamples {
		// Remove oldest (shift left)
		m.latencies = m.latencies[1:]
	}
	m.latencies = append(m.latencies, latencyMs)

	return latencyMs
}

// RecordCircuitBreakerTrip records a circuit breaker trip event
func (m *Metrics) RecordCircuitBreakerTrip() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.circuitBreakerTrips++
	m.circuitBreakerState = "open"
}

// RecordCircuitBreakerReset records circuit breaker reset (closed)
func (m *Metrics) RecordCircuitBreakerReset() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.circuitBreakerState = "closed"
}

// RecordCircuitBreakerHalfOpen records circuit breaker entering half-open state
func (m *Metrics) RecordCircuitBreakerHalfOpen() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.circuitBreakerState = "half-open"
}

// RecordHeartbeatRtt records a heartbeat round-trip time
func (m *Metrics) RecordHeartbeatRtt(rttMs float64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Keep last 100 samples
	if len(m.heartbeatRtts) >= 100 {
		m.heartbeatRtts = m.heartbeatRtts[1:]
	}
	m.heartbeatRtts = append(m.heartbeatRtts, rttMs)
}

// RecordHeartbeatMiss records a missed heartbeat (timeout)
func (m *Metrics) RecordHeartbeatMiss() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.heartbeatMisses++
}

// Snapshot returns a point-in-time snapshot of all metrics
func (m *Metrics) Snapshot() MetricsSnapshot {
	m.mu.RLock()
	defer m.mu.RUnlock()

	snapshot := MetricsSnapshot{
		RequestsTotal:       m.requestsTotal,
		RequestsSuccess:     m.requestsSuccess,
		RequestsFailed:      m.requestsFailed,
		QueueDepth:          m.queueDepth,
		QueueMaxDepth:       m.queueMaxDepth,
		CircuitBreakerTrips: m.circuitBreakerTrips,
		CircuitBreakerState: m.circuitBreakerState,
		HeartbeatMisses:     m.heartbeatMisses,
		WindowSeconds:       m.windowSeconds,
		Timestamp:           time.Now(),
	}

	// Calculate latency percentiles
	if len(m.latencies) > 0 {
		latencies := make([]float64, len(m.latencies))
		copy(latencies, m.latencies)
		sort.Float64s(latencies)

		n := len(latencies)
		snapshot.LatencyMinMs = latencies[0]
		snapshot.LatencyMaxMs = latencies[n-1]

		// Calculate average
		sum := 0.0
		for _, v := range latencies {
			sum += v
		}
		snapshot.LatencyAvgMs = sum / float64(n)

		// Calculate percentiles
		snapshot.LatencyP50Ms = latencies[n*50/100]
		snapshot.LatencyP95Ms = latencies[n*95/100]
		snapshot.LatencyP99Ms = latencies[n*99/100]
	}

	// Calculate heartbeat RTT average
	if len(m.heartbeatRtts) > 0 {
		sum := 0.0
		for _, v := range m.heartbeatRtts {
			sum += v
		}
		snapshot.HeartbeatRttAvgMs = sum / float64(len(m.heartbeatRtts))
		snapshot.HeartbeatRttLastMs = m.heartbeatRtts[len(m.heartbeatRtts)-1]
	}

	return snapshot
}

// Reset resets all metrics
func (m *Metrics) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.requestsTotal = 0
	m.requestsSuccess = 0
	m.requestsFailed = 0
	m.circuitBreakerTrips = 0
	m.circuitBreakerState = "closed"
	m.queueDepth = 0
	m.queueMaxDepth = 0
	m.latencies = make([]float64, 0, m.maxLatencySamples)
	m.heartbeatRtts = make([]float64, 0, 100)
	m.heartbeatMisses = 0
}
