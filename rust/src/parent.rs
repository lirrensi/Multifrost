use crate::error::{MultifrostError, Result};
use crate::message::{Message, MessageType};
use crate::registry::ServiceRegistry;
use crate::metrics::Metrics;
use crate::logging::{StructuredLogger, LogLevel};
use bytes::Bytes;
use std::collections::HashMap;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex, RwLock};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Result<serde_json::Value>>>>>;
type PendingHeartbeats = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Configuration options for ParentWorker.
#[derive(Clone)]
pub struct ParentWorkerConfig {
    pub script_path: Option<String>,
    pub executable: String,
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub is_spawn_mode: bool,
    pub auto_restart: bool,
    pub max_restart_attempts: usize,
    pub default_timeout: Option<Duration>,
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    pub heartbeat_max_misses: usize,
    pub enable_metrics: bool,
}

impl Default for ParentWorkerConfig {
    fn default() -> Self {
        Self {
            script_path: None,
            executable: String::new(),
            service_id: None,
            port: None,
            is_spawn_mode: false,
            auto_restart: false,
            max_restart_attempts: 5,
            default_timeout: None,
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(3),
            heartbeat_max_misses: 3,
            enable_metrics: true,
        }
    }
}

pub struct ParentWorker {
    config: ParentWorkerConfig,
    socket: Arc<Mutex<Option<DealerSocket>>>,
    process: Option<Child>,
    running: Arc<RwLock<bool>>,
    pending: PendingRequests,
    pending_heartbeats: PendingHeartbeats,
    consecutive_failures: Arc<RwLock<usize>>,
    circuit_open: Arc<RwLock<bool>>,
    consecutive_heartbeat_misses: Arc<RwLock<usize>>,
    last_heartbeat_rtt_ms: Arc<RwLock<Option<f64>>>,
    metrics: Option<Metrics>,
    logger: StructuredLogger,
    restart_count: Arc<RwLock<usize>>,
}

impl ParentWorker {
    /// Create a ParentWorker in spawn mode (owns the child process).
    pub fn spawn(script_path: &str, executable: &str) -> Result<Self> {
        let port = find_free_port_sync();
        Ok(Self::with_config(ParentWorkerConfig {
            script_path: Some(script_path.to_string()),
            executable: executable.to_string(),
            port: Some(port),
            is_spawn_mode: true,
            ..Default::default()
        }))
    }

    /// Create a ParentWorker in spawn mode with custom options.
    pub fn spawn_with_options(
        script_path: &str,
        executable: &str,
        options: SpawnOptions,
    ) -> Result<Self> {
        let port = find_free_port_sync();
        Ok(Self::with_config(ParentWorkerConfig {
            script_path: Some(script_path.to_string()),
            executable: executable.to_string(),
            port: Some(port),
            is_spawn_mode: true,
            auto_restart: options.auto_restart,
            max_restart_attempts: options.max_restart_attempts,
            default_timeout: options.default_timeout.map(Duration::from_millis),
            heartbeat_interval: Duration::from_secs_f64(options.heartbeat_interval),
            heartbeat_timeout: Duration::from_secs_f64(options.heartbeat_timeout),
            heartbeat_max_misses: options.heartbeat_max_misses,
            enable_metrics: options.enable_metrics,
            ..Default::default()
        }))
    }

    /// Create a ParentWorker in connect mode (connects to existing service).
    pub async fn connect(service_id: &str, timeout_ms: u64) -> Result<Self> {
        let port = ServiceRegistry::discover(service_id, timeout_ms).await?;
        Ok(Self::with_config(ParentWorkerConfig {
            service_id: Some(service_id.to_string()),
            port: Some(port),
            is_spawn_mode: false,
            ..Default::default()
        }))
    }

    fn with_config(config: ParentWorkerConfig) -> Self {
        let metrics = if config.enable_metrics {
            Some(Metrics::new())
        } else {
            None
        };

        let worker_id = format!("{:x}", md5::compute(format!("{}-{}", config.script_path.as_deref().unwrap_or("connect"), std::process::id())));
        let service_id = config.service_id.clone();

        Self {
            config,
            socket: Arc::new(Mutex::new(None)),
            process: None,
            running: Arc::new(RwLock::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            pending_heartbeats: Arc::new(Mutex::new(HashMap::new())),
            consecutive_failures: Arc::new(RwLock::new(0)),
            circuit_open: Arc::new(RwLock::new(false)),
            consecutive_heartbeat_misses: Arc::new(RwLock::new(0)),
            last_heartbeat_rtt_ms: Arc::new(RwLock::new(None)),
            metrics,
            logger: StructuredLogger::new(
                Some(Arc::new(|entry| println!("{}", entry.to_json()))),
                LogLevel::Info,
                Some(worker_id),
                service_id,
            ),
            restart_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Check if the worker is healthy (circuit breaker not tripped).
    pub async fn is_healthy(&self) -> bool {
        !*self.circuit_open.read().await && *self.running.read().await
    }

    /// Check if circuit breaker is open.
    pub async fn circuit_open(&self) -> bool {
        *self.circuit_open.read().await
    }

    /// Get the last heartbeat round-trip time in milliseconds.
    pub async fn last_heartbeat_rtt_ms(&self) -> Option<f64> {
        *self.last_heartbeat_rtt_ms.read().await
    }

    /// Get the metrics collector.
    pub fn metrics(&self) -> Option<&Metrics> {
        self.metrics.as_ref()
    }

    /// Set a custom log handler.
    pub fn set_log_handler(&mut self, handler: Arc<dyn Fn(&crate::logging::LogEntry) + Send + Sync>) {
        self.logger.set_handler(handler);
    }

    /// Start the worker.
    pub async fn start(&mut self) -> Result<()> {
        let mut socket = DealerSocket::new();

        if self.config.is_spawn_mode {
            socket.bind(&format!("tcp://0.0.0.0:{}", self.config.port.unwrap()))
                .await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
            self.start_child_process()?;
        } else {
            socket.connect(&format!("tcp://127.0.0.1:{}", self.config.port.unwrap()))
                .await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        }

        *self.socket.lock().await = Some(socket);
        *self.running.write().await = true;

        // Start message loop in background
        let socket = Arc::clone(&self.socket);
        let pending = Arc::clone(&self.pending);
        let pending_heartbeats = Arc::clone(&self.pending_heartbeats);
        let running = Arc::clone(&self.running);
        let consecutive_failures = Arc::clone(&self.consecutive_failures);
        let circuit_open = Arc::clone(&self.circuit_open);
        let consecutive_heartbeat_misses = Arc::clone(&self.consecutive_heartbeat_misses);
        let last_heartbeat_rtt_ms = Arc::clone(&self.last_heartbeat_rtt_ms);
        let metrics = self.metrics.clone();
        let logger = self.logger.clone();
        let is_spawn_mode = self.config.is_spawn_mode;
        let heartbeat_interval = self.config.heartbeat_interval;
        let heartbeat_timeout = self.config.heartbeat_timeout;
        let heartbeat_max_misses = self.config.heartbeat_max_misses;
        let max_restart_attempts = self.config.max_restart_attempts;
        let auto_restart = self.config.auto_restart;
        let restart_count = Arc::clone(&self.restart_count);
        let script_path = self.config.script_path.clone();
        let executable = self.config.executable.clone();
        let port = self.config.port.unwrap();

        tokio::spawn(async move {
            message_loop(
                socket, pending, pending_heartbeats, running,
                consecutive_failures, circuit_open, consecutive_heartbeat_misses,
                last_heartbeat_rtt_ms, metrics, logger, is_spawn_mode,
                heartbeat_interval, heartbeat_timeout, heartbeat_max_misses,
                max_restart_attempts, auto_restart, restart_count,
                script_path, executable, port,
            ).await;
        });

        // Log worker start
        let mode = if self.config.is_spawn_mode { "spawn" } else { "connect" };
        self.logger.worker_start(mode);

        // Wait a bit for connection to establish
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    fn start_child_process(&mut self) -> Result<()> {
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            if let Some(ref script) = self.config.script_path {
                c.args(["/C", &self.config.executable, script]);
            } else {
                c.args(["/C", &self.config.executable]);
            }
            c
        } else {
            let mut c = Command::new(&self.config.executable);
            if let Some(ref script) = self.config.script_path {
                c.arg(script);
            }
            c
        };

        cmd.env("COMLINK_ZMQ_PORT", self.config.port.unwrap().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        self.process = Some(cmd.spawn()?);
        Ok(())
    }

    /// Call a remote function with typed result.
    pub async fn call<T: serde::de::DeserializeOwned>(
        &self,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<T> {
        let value = self.call_raw(function, args).await?;
        serde_json::from_value(value)
            .map_err(MultifrostError::JsonError)
    }

    /// Call a remote function returning raw JSON Value.
    pub async fn call_raw(
        &self,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.call_with_timeout(function, args, None).await
    }

    /// Call a remote function with timeout.
    pub async fn call_with_timeout(
        &self,
        function: &str,
        args: Vec<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<serde_json::Value> {
        // Check circuit breaker
        if *self.circuit_open.read().await {
            let failures = *self.consecutive_failures.read().await;
            return Err(MultifrostError::CircuitOpenError(failures));
        }

        if !*self.running.read().await {
            return Err(MultifrostError::NotRunningError);
        }

        let effective_timeout = timeout.or(self.config.default_timeout);

        let message = Message::create_call(function, args);
        let msg_id = message.id.clone();

        // Start metrics tracking
        let start_time = if let Some(ref metrics) = self.metrics {
            Some(metrics.start_request(&msg_id, function, "default").await)
        } else {
            None
        };

        // Log request start
        self.logger.request_start(&msg_id, function, "default", None, None);

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(msg_id.clone(), tx);
        }

        // Send message - DEALER sends: [empty, message]
        let packed = message.pack()?;

        {
            let mut socket_guard = self.socket.lock().await;
            if let Some(ref mut socket) = *socket_guard {
                let zmq_msg: ZmqMessage = vec![
                    Bytes::from(vec![]),
                    Bytes::from(packed),
                ].try_into()
                    .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?;

                socket.send(zmq_msg).await
                    .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
            } else {
                return Err(MultifrostError::NotRunningError);
            }
        }

        // Wait for response
        let result: serde_json::Value = if let Some(timeout) = effective_timeout {
            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(Ok(value))) => value,
                Ok(Ok(Err(e))) => return Err(e),
                Ok(Err(_)) => return Err(MultifrostError::TimeoutError),
                Err(_) => return Err(MultifrostError::TimeoutError),
            }
        } else {
            match rx.await {
                Ok(Ok(value)) => value,
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(MultifrostError::TimeoutError),
            }
        };

        // Record success
        if let (Some(start), Some(ref metrics)) = (start_time, &self.metrics) {
            let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            metrics.end_request(start, &msg_id, true, None).await;
            self.logger.request_end(&msg_id, function, duration_ms, true, None, None);
        }

        // Reset consecutive failures on success
        {
            let mut failures = self.consecutive_failures.write().await;
            if *failures > 0 {
                *failures = 0;
                if *self.circuit_open.read().await {
                    *self.circuit_open.write().await = false;
                    self.logger.circuit_close();
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_circuit_breaker_reset().await;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Stop the worker gracefully.
    pub async fn stop(&mut self) {
        self.logger.worker_stop("shutdown");

        *self.running.write().await = false;

        // Cancel pending requests
        {
            let mut pending = self.pending.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(MultifrostError::NotRunningError));
            }
        }

        // Cancel pending heartbeats
        {
            let mut pending = self.pending_heartbeats.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(false);
            }
        }

        // Close socket
        let mut socket_guard = self.socket.lock().await;
        if let Some(socket) = socket_guard.take() {
            drop(socket);
        }

        // Kill child process if still running
        if let Some(ref mut child) = self.process {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Options for spawning a ParentWorker.
#[derive(Clone)]
pub struct SpawnOptions {
    pub auto_restart: bool,
    pub max_restart_attempts: usize,
    pub default_timeout: Option<u64>, // milliseconds
    pub heartbeat_interval: f64, // seconds
    pub heartbeat_timeout: f64, // seconds
    pub heartbeat_max_misses: usize,
    pub enable_metrics: bool,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            auto_restart: false,
            max_restart_attempts: 5,
            default_timeout: None,
            heartbeat_interval: 5.0,
            heartbeat_timeout: 3.0,
            heartbeat_max_misses: 3,
            enable_metrics: true,
        }
    }
}

/// Options for connecting to an existing service.
#[derive(Clone)]
pub struct ConnectOptions {
    pub default_timeout: Option<u64>, // milliseconds
    pub heartbeat_interval: f64, // seconds
    pub heartbeat_timeout: f64, // seconds
    pub heartbeat_max_misses: usize,
    pub enable_metrics: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            default_timeout: None,
            heartbeat_interval: 5.0,
            heartbeat_timeout: 3.0,
            heartbeat_max_misses: 3,
            enable_metrics: true,
        }
    }
}

/// Builder pattern for creating ParentWorker instances.
///
/// # Example
///
/// ```rust,no_run
/// use multifrost::{ParentWorkerBuilder, SpawnOptions};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let worker = ParentWorkerBuilder::spawn("worker.py", "python worker.py")
///         .auto_restart(true)
///         .default_timeout(Duration::from_secs(30))
///         .build()
///         .await?;
///
///     worker.start().await?;
///     // ...
///     Ok(())
/// }
/// ```
pub struct ParentWorkerBuilder {
    script_path: Option<String>,
    executable: String,
    options: SpawnOptions,
}

impl ParentWorkerBuilder {
    /// Create a new builder for spawn mode.
    pub fn spawn(script_path: &str, executable: &str) -> Self {
        Self {
            script_path: Some(script_path.to_string()),
            executable: executable.to_string(),
            options: SpawnOptions::default(),
        }
    }

    /// Create a new builder for connect mode.
    pub fn connect(executable: &str) -> Self {
        Self {
            script_path: None,
            executable: executable.to_string(),
            options: SpawnOptions::default(),
        }
    }

    /// Enable automatic restart on failure.
    pub fn auto_restart(mut self, enable: bool) -> Self {
        self.options.auto_restart = enable;
        self
    }

    /// Set maximum restart attempts.
    pub fn max_restart_attempts(mut self, attempts: usize) -> Self {
        self.options.max_restart_attempts = attempts;
        self
    }

    /// Set default timeout for calls (in milliseconds).
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.options.default_timeout = Some(timeout.as_millis() as u64);
        self
    }

    /// Set heartbeat interval in seconds.
    pub fn heartbeat_interval(mut self, interval: f64) -> Self {
        self.options.heartbeat_interval = interval;
        self
    }

    /// Set heartbeat timeout in seconds.
    pub fn heartbeat_timeout(mut self, timeout: f64) -> Self {
        self.options.heartbeat_timeout = timeout;
        self
    }

    /// Set maximum missed heartbeats before considering worker dead.
    pub fn heartbeat_max_misses(mut self, misses: usize) -> Self {
        self.options.heartbeat_max_misses = misses;
        self
    }

    /// Enable or disable metrics collection.
    pub fn enable_metrics(mut self, enable: bool) -> Self {
        self.options.enable_metrics = enable;
        self
    }

    /// Build the ParentWorker in spawn mode.
    pub async fn build_spawn(self) -> Result<ParentWorker> {
        ParentWorker::spawn_with_options(
            self.script_path.as_deref().unwrap_or(""),
            &self.executable,
            self.options,
        )
    }

    /// Build the ParentWorker in connect mode.
    pub async fn build_connect(self, service_id: &str, timeout_ms: u64) -> Result<ParentWorker> {
        let config = ParentWorkerConfig {
            script_path: self.script_path,
            executable: self.executable,
            service_id: Some(service_id.to_string()),
            port: None,
            is_spawn_mode: false,
            auto_restart: self.options.auto_restart,
            max_restart_attempts: self.options.max_restart_attempts,
            default_timeout: self.options.default_timeout.map(Duration::from_millis),
            heartbeat_interval: Duration::from_secs_f64(self.options.heartbeat_interval),
            heartbeat_timeout: Duration::from_secs_f64(self.options.heartbeat_timeout),
            heartbeat_max_misses: self.options.heartbeat_max_misses,
            enable_metrics: self.options.enable_metrics,
        };
        let port = ServiceRegistry::discover(service_id, timeout_ms).await?;
        let mut config = config;
        config.port = Some(port);
        Ok(ParentWorker::with_config(config))
    }
}

fn find_free_port_sync() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr().map(|a| a.port()))
        .unwrap_or(5555)
}

async fn message_loop(
    socket: Arc<Mutex<Option<DealerSocket>>>,
    pending: PendingRequests,
    pending_heartbeats: PendingHeartbeats,
    running: Arc<RwLock<bool>>,
    consecutive_failures: Arc<RwLock<usize>>,
    circuit_open: Arc<RwLock<bool>>,
    consecutive_heartbeat_misses: Arc<RwLock<usize>>,
    last_heartbeat_rtt_ms: Arc<RwLock<Option<f64>>>,
    metrics: Option<Metrics>,
    logger: StructuredLogger,
    is_spawn_mode: bool,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    heartbeat_max_misses: usize,
    _max_restart_attempts: usize,
    _auto_restart: bool,
    _restart_count: Arc<RwLock<usize>>,
    _script_path: Option<String>,
    _executable: String,
    _port: u16,
) {
    // Start heartbeat loop if in spawn mode
    if is_spawn_mode && heartbeat_interval.as_secs() > 0 {
        let socket_clone = Arc::clone(&socket);
        let pending_heartbeats_clone = Arc::clone(&pending_heartbeats);
        let running_clone = Arc::clone(&running);
        let consecutive_heartbeat_misses_clone = Arc::clone(&consecutive_heartbeat_misses);
        let last_heartbeat_rtt_ms_clone = Arc::clone(&last_heartbeat_rtt_ms);
        let metrics_clone = metrics.clone();
        let logger_clone = logger.clone();
        let consecutive_failures_clone = Arc::clone(&consecutive_failures);
        let circuit_open_clone = Arc::clone(&circuit_open);

        tokio::spawn(async move {
            heartbeat_loop(
                socket_clone, pending_heartbeats_clone, running_clone,
                consecutive_heartbeat_misses_clone, last_heartbeat_rtt_ms_clone,
                metrics_clone, logger_clone, heartbeat_interval, heartbeat_timeout,
                heartbeat_max_misses, consecutive_failures_clone, circuit_open_clone,
            ).await;
        });
    }

    loop {
        if !*running.read().await {
            break;
        }

        // Use timeout to avoid holding the lock indefinitely
        let msg_result = {
            let mut socket_guard = socket.lock().await;
            if let Some(ref mut sock) = *socket_guard {
                tokio::time::timeout(Duration::from_millis(100), sock.recv()).await
            } else {
                break;
            }
        };

        let zmq_msg = match msg_result {
            Ok(Ok(msg)) => msg,
            Ok(Err(_)) | Err(_) => continue,
        };

        // DEALER receives: [empty, message_data]
        let frames: Vec<_> = zmq_msg.into_vec();

        if frames.len() < 2 {
            continue;
        }

        let message_data = frames[1].to_vec();

        match Message::unpack(&message_data) {
            Ok(message) => {
                if !message.is_valid() {
                    continue;
                }

                match message.msg_type {
                    MessageType::Response => {
                        if let Some(result) = message.result {
                            let mut pending_guard = pending.lock().await;
                            if let Some(tx) = pending_guard.remove(&message.id) {
                                let _ = tx.send(Ok(result));
                            }
                        }
                    }
                    MessageType::Error => {
                        let error = message.error.unwrap_or_else(|| "Unknown error".to_string());
                        let mut pending_guard = pending.lock().await;
                        if let Some(tx) = pending_guard.remove(&message.id) {
                            let _ = tx.send(Err(MultifrostError::RemoteCallError(error)));
                        }
                    }
                    MessageType::Stdout => {
                        if let Some(ref output) = message.output {
                            println!("[worker STDOUT]: {}", output);
                        }
                    }
                    MessageType::Stderr => {
                        if let Some(ref output) = message.output {
                            eprintln!("[worker STDERR]: {}", output);
                        }
                    }
                    MessageType::Heartbeat => {
                        // Handle heartbeat response
                        let mut pending_guard = pending_heartbeats.lock().await;
                        if let Some(tx) = pending_guard.remove(&message.id) {
                            // Calculate RTT from timestamp in metadata
                            if let Some(ref metadata) = message.metadata {
                                if let Some(hb_timestamp) = metadata.get("hb_timestamp").and_then(|v| v.as_f64()) {
                                    let rtt_ms = (current_timestamp() - hb_timestamp) * 1000.0;
                                    *last_heartbeat_rtt_ms.write().await = Some(rtt_ms);

                                    if let Some(ref metrics) = metrics {
                                        metrics.record_heartbeat_rtt(rtt_ms).await;
                                    }
                                }
                            }

                            // Reset consecutive misses on successful response
                            *consecutive_heartbeat_misses.write().await = 0;
                            let _ = tx.send(true);
                        }
                    }
                    MessageType::Call | MessageType::Shutdown | MessageType::Ready | MessageType::Unknown => {}
                }
            }
            Err(_) => {}
        }
    }
}

async fn heartbeat_loop(
    socket: Arc<Mutex<Option<DealerSocket>>>,
    pending_heartbeats: PendingHeartbeats,
    running: Arc<RwLock<bool>>,
    consecutive_heartbeat_misses: Arc<RwLock<usize>>,
    _last_heartbeat_rtt_ms: Arc<RwLock<Option<f64>>>,
    metrics: Option<Metrics>,
    logger: StructuredLogger,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    heartbeat_max_misses: usize,
    consecutive_failures: Arc<RwLock<usize>>,
    circuit_open: Arc<RwLock<bool>>,
) {
    // Wait for initial connection
    tokio::time::sleep(Duration::from_secs(1)).await;

    while *running.read().await {
        tokio::time::sleep(heartbeat_interval).await;

        if !*running.read().await {
            break;
        }

        // Create heartbeat message
        let heartbeat = Message::create_heartbeat(None, Some(current_timestamp()));
        let heartbeat_id = heartbeat.id.clone();

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_heartbeats.lock().await;
            pending.insert(heartbeat_id.clone(), tx);
        }

        // Send heartbeat
        let packed = match heartbeat.pack() {
            Ok(p) => p,
            Err(_) => continue,
        };

        {
            let mut socket_guard = socket.lock().await;
            if let Some(ref mut socket) = *socket_guard {
                let zmq_msg_result: std::result::Result<ZmqMessage, _> = vec![
                    Bytes::from(vec![]),
                    Bytes::from(packed),
                ].try_into();

                if let Ok(msg) = zmq_msg_result {
                    let _ = socket.send(msg).await;
                }
            }
        }

        // Wait for response with timeout
        match tokio::time::timeout(heartbeat_timeout, rx).await {
            Ok(Ok(_)) => {
                // Success - RTT already recorded in message loop
            }
            Ok(Err(_)) | Err(_) => {
                // Heartbeat timed out
                let mut misses = consecutive_heartbeat_misses.write().await;
                *misses += 1;

                logger.heartbeat_missed(*misses, heartbeat_max_misses);

                // Check if too many misses
                if *misses >= heartbeat_max_misses {
                    logger.heartbeat_timeout(*misses);

                    // Record failure for circuit breaker
                    let mut failures = consecutive_failures.write().await;
                    *failures += 1;
                    if *failures >= 5 {
                        *circuit_open.write().await = true;
                        logger.circuit_open(*failures);
                        if let Some(ref metrics) = metrics {
                            metrics.record_circuit_breaker_trip().await;
                        }
                    }
                }
            }
        }
    }
}

fn current_timestamp() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
