//! Metrics collection for observability.
//!
//! Tracks request latency, error rates, queue depth, and circuit breaker events.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Metrics for a single completed request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetrics {
    pub request_id: String,
    pub function: String,
    pub namespace: String,
    pub success: bool,
    pub latency_ms: f64,
    pub error: Option<String>,
    pub timestamp: f64,
}

/// Point-in-time snapshot of all metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    // Counters
    pub requests_total: usize,
    pub requests_success: usize,
    pub requests_failed: usize,

    // Latency (milliseconds)
    pub latency_avg_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_min_ms: f64,
    pub latency_max_ms: f64,

    // Queue
    pub queue_depth: usize,
    pub queue_max_depth: usize,

    // Circuit breaker
    pub circuit_breaker_trips: usize,
    pub circuit_breaker_state: String,

    // Heartbeat
    pub heartbeat_rtt_avg_ms: f64,
    pub heartbeat_rtt_last_ms: f64,
    pub heartbeat_misses: usize,

    // Time window
    pub window_seconds: f64,

    // Timestamp
    pub timestamp: f64,
}

/// Thread-safe metrics collector for ParentWorker.
///
/// # Example
///
/// ```rust,no_run
/// use multifrost::metrics::Metrics;
///
/// let metrics = Metrics::new();
///
/// // Start tracking a request
/// let start = metrics.start_request("req-123", "myFunc", "default");
/// // ... do work ...
/// metrics.end_request(start, "req-123", true);
///
/// // Get snapshot
/// let snapshot = metrics.snapshot().await;
/// println!("Avg latency: {}ms", snapshot.latency_avg_ms);
/// ```
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<RwLock<MetricsInner>>,
}

struct MetricsInner {
    max_latency_samples: usize,
    window_seconds: f64,

    // Counters
    requests_total: usize,
    requests_success: usize,
    requests_failed: usize,
    circuit_breaker_trips: usize,
    circuit_breaker_state: String,

    // Queue tracking
    queue_depth: usize,
    queue_max_depth: usize,

    // Latency samples (circular buffer)
    latencies: VecDeque<f64>,

    // Heartbeat RTT samples
    heartbeat_rtts: VecDeque<f64>,
    heartbeat_misses: usize,
}

impl Metrics {
    /// Create a new Metrics collector.
    pub fn new() -> Self {
        Self::with_options(1000, 60.0)
    }

    /// Create a new Metrics collector with custom options.
    pub fn with_options(max_latency_samples: usize, window_seconds: f64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(MetricsInner {
                max_latency_samples,
                window_seconds,
                requests_total: 0,
                requests_success: 0,
                requests_failed: 0,
                circuit_breaker_trips: 0,
                circuit_breaker_state: "closed".to_string(),
                queue_depth: 0,
                queue_max_depth: 0,
                latencies: VecDeque::with_capacity(max_latency_samples),
                heartbeat_rtts: VecDeque::with_capacity(100),
                heartbeat_misses: 0,
            })),
        }
    }

    /// Start tracking a request.
    ///
    /// Returns start timestamp for later end_request() call.
    pub async fn start_request(&self, _request_id: &str, _function: &str, _namespace: &str) -> Instant {
        let mut inner = self.inner.write().await;
        inner.requests_total += 1;
        inner.queue_depth += 1;
        inner.queue_max_depth = inner.queue_max_depth.max(inner.queue_depth);
        Instant::now()
    }

    /// End tracking a request.
    ///
    /// Returns latency in milliseconds.
    pub async fn end_request(&self, start_time: Instant, _request_id: &str, success: bool, _error: Option<String>) -> f64 {
        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        let mut inner = self.inner.write().await;
        inner.queue_depth = inner.queue_depth.saturating_sub(1);

        if success {
            inner.requests_success += 1;
        } else {
            inner.requests_failed += 1;
        }

        // Store latency sample (circular buffer behavior)
        inner.latencies.push_back(latency_ms);
        if inner.latencies.len() > inner.max_latency_samples {
            inner.latencies.pop_front();
        }

        latency_ms
    }

    /// Record a circuit breaker trip event.
    pub async fn record_circuit_breaker_trip(&self) {
        let mut inner = self.inner.write().await;
        inner.circuit_breaker_trips += 1;
        inner.circuit_breaker_state = "open".to_string();
    }

    /// Record circuit breaker reset (closed).
    pub async fn record_circuit_breaker_reset(&self) {
        let mut inner = self.inner.write().await;
        inner.circuit_breaker_state = "closed".to_string();
    }

    /// Record circuit breaker entering half-open state.
    pub async fn record_circuit_breaker_half_open(&self) {
        let mut inner = self.inner.write().await;
        inner.circuit_breaker_state = "half-open".to_string();
    }

    /// Record a heartbeat round-trip time.
    pub async fn record_heartbeat_rtt(&self, rtt_ms: f64) {
        let mut inner = self.inner.write().await;
        inner.heartbeat_rtts.push_back(rtt_ms);
        if inner.heartbeat_rtts.len() > 100 {
            inner.heartbeat_rtts.pop_front();
        }
    }

    /// Record a missed heartbeat (timeout).
    pub async fn record_heartbeat_miss(&self) {
        let mut inner = self.inner.write().await;
        inner.heartbeat_misses += 1;
    }

    /// Get a point-in-time snapshot of all metrics.
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.read().await;

        // Calculate percentiles
        let mut latencies: Vec<f64> = inner.latencies.iter().cloned().collect();
        let (latency_avg, latency_p50, latency_p95, latency_p99, latency_min, latency_max) =
            if !latencies.is_empty() {
                latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let n = latencies.len();
                let p50_idx = (n as f64 * 0.50) as usize;
                let p95_idx = (n as f64 * 0.95) as usize;
                let p99_idx = (n as f64 * 0.99) as usize;

                let latency_avg = latencies.iter().sum::<f64>() / n as f64;
                let latency_p50 = latencies[p50_idx.min(n - 1)];
                let latency_p95 = latencies[p95_idx.min(n - 1)];
                let latency_p99 = latencies[p99_idx.min(n - 1)];
                let latency_min = latencies[0];
                let latency_max = latencies[n - 1];

                (latency_avg, latency_p50, latency_p95, latency_p99, latency_min, latency_max)
            } else {
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            };

        // Calculate heartbeat RTT average
        let heartbeat_rtts: Vec<f64> = inner.heartbeat_rtts.iter().cloned().collect();
        let (heartbeat_rtt_avg, heartbeat_rtt_last) = if !heartbeat_rtts.is_empty() {
            let avg = heartbeat_rtts.iter().sum::<f64>() / heartbeat_rtts.len() as f64;
            let last = *heartbeat_rtts.last().unwrap();
            (avg, last)
        } else {
            (0.0, 0.0)
        };

        MetricsSnapshot {
            requests_total: inner.requests_total,
            requests_success: inner.requests_success,
            requests_failed: inner.requests_failed,
            latency_avg_ms: latency_avg,
            latency_p50_ms: latency_p50,
            latency_p95_ms: latency_p95,
            latency_p99_ms: latency_p99,
            latency_min_ms: latency_min,
            latency_max_ms: latency_max,
            queue_depth: inner.queue_depth,
            queue_max_depth: inner.queue_max_depth,
            circuit_breaker_trips: inner.circuit_breaker_trips,
            circuit_breaker_state: inner.circuit_breaker_state.clone(),
            heartbeat_rtt_avg_ms: heartbeat_rtt_avg,
            heartbeat_rtt_last_ms: heartbeat_rtt_last,
            heartbeat_misses: inner.heartbeat_misses,
            window_seconds: inner.window_seconds,
            timestamp: current_timestamp(),
        }
    }

    /// Reset all metrics.
    pub async fn reset(&self) {
        let mut inner = self.inner.write().await;
        inner.requests_total = 0;
        inner.requests_success = 0;
        inner.requests_failed = 0;
        inner.circuit_breaker_trips = 0;
        inner.circuit_breaker_state = "closed".to_string();
        inner.queue_depth = 0;
        inner.queue_max_depth = 0;
        inner.latencies.clear();
        inner.heartbeat_rtts.clear();
        inner.heartbeat_misses = 0;
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

fn current_timestamp() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_creation() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.requests_total, 0);
        assert_eq!(snapshot.requests_success, 0);
        assert_eq!(snapshot.requests_failed, 0);
        assert_eq!(snapshot.latency_avg_ms, 0.0);
        assert_eq!(snapshot.queue_depth, 0);
    }

    #[tokio::test]
    async fn test_metrics_creation_with_options() {
        let metrics = Metrics::with_options(100, 30.0);
        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.window_seconds, 30.0);
        assert_eq!(snapshot.requests_total, 0);
    }

    #[tokio::test]
    async fn test_record_request_success() {
        let metrics = Metrics::new();
        let start = metrics.start_request("req-1", "add", "default").await;

        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let latency = metrics.end_request(start, "req-1", true, None).await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.requests_success, 1);
        assert_eq!(snapshot.requests_failed, 0);
        assert!(latency >= 10.0);
        assert!(snapshot.latency_avg_ms > 0.0);
    }

    #[tokio::test]
    async fn test_record_request_failure() {
        let metrics = Metrics::new();
        let start = metrics.start_request("req-1", "fail", "default").await;

        let latency = metrics.end_request(start, "req-1", false, Some("Test error".to_string())).await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.requests_success, 0);
        assert_eq!(snapshot.requests_failed, 1);
        assert!(latency >= 0.0);
    }

    #[tokio::test]
    async fn test_latency_percentiles() {
        let metrics = Metrics::new();

        // Record multiple requests with different latencies
        for i in 0..100 {
            let start = metrics.start_request(&format!("req-{}", i), "test", "default").await;
            // Simulate varying latencies
            tokio::time::sleep(std::time::Duration::from_millis(i)).await;
            metrics.end_request(start, &format!("req-{}", i), true, None).await;
        }

        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.requests_total, 100);
        assert!(snapshot.latency_p50_ms > 0.0);
        assert!(snapshot.latency_p95_ms > snapshot.latency_p50_ms);
        assert!(snapshot.latency_p99_ms >= snapshot.latency_p95_ms);
        assert!(snapshot.latency_min_ms <= snapshot.latency_p50_ms);
        assert!(snapshot.latency_max_ms >= snapshot.latency_p99_ms);
    }

    #[tokio::test]
    async fn test_circuit_breaker_tracking() {
        let metrics = Metrics::new();

        metrics.record_circuit_breaker_trip().await;
        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.circuit_breaker_trips, 1);
        assert_eq!(snapshot.circuit_breaker_state, "open");

        metrics.record_circuit_breaker_reset().await;
        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.circuit_breaker_state, "closed");

        metrics.record_circuit_breaker_half_open().await;
        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.circuit_breaker_state, "half-open");
    }

    #[tokio::test]
    async fn test_queue_depth_tracking() {
        let metrics = Metrics::new();

        // Start multiple requests without ending them
        let _start1 = metrics.start_request("req-1", "test", "default").await;
        let _start2 = metrics.start_request("req-2", "test", "default").await;
        let _start3 = metrics.start_request("req-3", "test", "default").await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.queue_depth, 3);
        assert_eq!(snapshot.queue_max_depth, 3);

        // End one request
        metrics.end_request(_start1, "req-1", true, None).await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.queue_depth, 2);
        assert_eq!(snapshot.queue_max_depth, 3);
    }

    #[tokio::test]
    async fn test_heartbeat_rtt_tracking() {
        let metrics = Metrics::new();

        metrics.record_heartbeat_rtt(10.5).await;
        metrics.record_heartbeat_rtt(15.2).await;
        metrics.record_heartbeat_rtt(12.8).await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.heartbeat_rtt_last_ms, 12.8);
        assert!((snapshot.heartbeat_rtt_avg_ms - 12.83).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_heartbeat_miss_tracking() {
        let metrics = Metrics::new();

        metrics.record_heartbeat_miss().await;
        metrics.record_heartbeat_miss().await;
        metrics.record_heartbeat_miss().await;

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.heartbeat_misses, 3);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let metrics = Metrics::new();

        let start = metrics.start_request("req-1", "test", "default").await;
        metrics.end_request(start, "req-1", true, None).await;

        let snapshot = metrics.snapshot().await;

        assert!(snapshot.timestamp > 0.0);
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.requests_success, 1);
        assert_eq!(snapshot.requests_failed, 0);
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let metrics = Metrics::new();

        // Record some data
        let start = metrics.start_request("req-1", "test", "default").await;
        metrics.end_request(start, "req-1", true, None).await;
        metrics.record_circuit_breaker_trip().await;
        metrics.record_heartbeat_rtt(10.0).await;
        metrics.record_heartbeat_miss().await;

        // Reset
        metrics.reset().await;

        // Verify reset
        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.requests_total, 0);
        assert_eq!(snapshot.requests_success, 0);
        assert_eq!(snapshot.requests_failed, 0);
        assert_eq!(snapshot.circuit_breaker_trips, 0);
        assert_eq!(snapshot.circuit_breaker_state, "closed");
        assert_eq!(snapshot.queue_depth, 0);
        assert_eq!(snapshot.queue_max_depth, 0);
        assert_eq!(snapshot.heartbeat_misses, 0);
        assert_eq!(snapshot.latency_avg_ms, 0.0);
        assert_eq!(snapshot.heartbeat_rtt_avg_ms, 0.0);
    }

    #[tokio::test]
    async fn test_metrics_default() {
        let metrics = Metrics::default();
        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.requests_total, 0);
    }

    #[tokio::test]
    async fn test_metrics_clone() {
        let metrics1 = Metrics::new();
        let metrics2 = metrics1.clone();

        let start = metrics1.start_request("req-1", "test", "default").await;
        metrics1.end_request(start, "req-1", true, None).await;

        let snapshot1 = metrics1.snapshot().await;
        let snapshot2 = metrics2.snapshot().await;

        assert_eq!(snapshot1.requests_total, snapshot2.requests_total);
    }

    #[tokio::test]
    async fn test_latency_circular_buffer() {
        let metrics = Metrics::with_options(5, 60.0);

        // Record more than max samples
        for i in 0..10 {
            let start = metrics.start_request(&format!("req-{}", i), "test", "default").await;
            metrics.end_request(start, &format!("req-{}", i), true, None).await;
        }

        let snapshot = metrics.snapshot().await;
        // Should only have 5 samples in the circular buffer
        assert_eq!(snapshot.requests_total, 10);
        assert!(snapshot.latency_avg_ms > 0.0);
    }

    #[tokio::test]
    async fn test_heartbeat_rtt_circular_buffer() {
        let metrics = Metrics::new();

        // Record more than 100 RTT samples
        for i in 0..150 {
            metrics.record_heartbeat_rtt(i as f64).await;
        }

        let snapshot = metrics.snapshot().await;
        // Should only have last 100 samples
        assert_eq!(snapshot.heartbeat_rtt_last_ms, 149.0);
        assert!(snapshot.heartbeat_rtt_avg_ms > 0.0);
    }

    #[tokio::test]
    async fn test_empty_latency_percentiles() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.latency_avg_ms, 0.0);
        assert_eq!(snapshot.latency_p50_ms, 0.0);
        assert_eq!(snapshot.latency_p95_ms, 0.0);
        assert_eq!(snapshot.latency_p99_ms, 0.0);
        assert_eq!(snapshot.latency_min_ms, 0.0);
        assert_eq!(snapshot.latency_max_ms, 0.0);
    }

    #[tokio::test]
    async fn test_empty_heartbeat_rtt() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.heartbeat_rtt_avg_ms, 0.0);
        assert_eq!(snapshot.heartbeat_rtt_last_ms, 0.0);
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let metrics = Metrics::new();
        let mut handles = vec![];

        // Start multiple concurrent requests
        for i in 0..10 {
            let metrics_clone = metrics.clone();
            let handle = tokio::spawn(async move {
                let start = metrics_clone.start_request(&format!("req-{}", i), "test", "default").await;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                metrics_clone.end_request(start, &format!("req-{}", i), true, None).await;
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let snapshot = metrics.snapshot().await;
        assert_eq!(snapshot.requests_total, 10);
        assert_eq!(snapshot.requests_success, 10);
    }
}
