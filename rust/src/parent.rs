use crate::error::{MultifrostError, Result};
use crate::message::{Message, MessageType};
use crate::registry::ServiceRegistry;
use bytes::Bytes;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Result<serde_json::Value>>>>>;
type ChildIdentity = Arc<Mutex<Option<Vec<u8>>>>;

pub struct ParentWorker {
    script_path: Option<String>,
    executable: String,
    service_id: Option<String>,
    port: u16,
    is_spawn_mode: bool,
    socket: Arc<Mutex<Option<RouterSocket>>>,
    process: Option<Child>,
    running: Arc<Mutex<bool>>,
    pending: PendingRequests,
    child_identity: ChildIdentity,
}

impl ParentWorker {
    /// Create a ParentWorker in spawn mode (owns the child process)
    pub fn spawn(script_path: &str, executable: &str) -> Result<Self> {
        let port = find_free_port_sync();
        Ok(Self {
            script_path: Some(script_path.to_string()),
            executable: executable.to_string(),
            service_id: None,
            port,
            is_spawn_mode: true,
            socket: Arc::new(Mutex::new(None)),
            process: None,
            running: Arc::new(Mutex::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            child_identity: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a ParentWorker in spawn mode with just a command (for Rust binaries/examples)
    pub fn spawn_command(command: &str) -> Result<Self> {
        let port = find_free_port_sync();
        Ok(Self {
            script_path: None,
            executable: command.to_string(),
            service_id: None,
            port,
            is_spawn_mode: true,
            socket: Arc::new(Mutex::new(None)),
            process: None,
            running: Arc::new(Mutex::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            child_identity: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a ParentWorker in connect mode (connects to existing service)
    pub async fn connect(service_id: &str, timeout_ms: u64) -> Result<Self> {
        let port = ServiceRegistry::discover(service_id, timeout_ms).await?;
        Ok(Self {
            script_path: None,
            executable: String::new(),
            service_id: Some(service_id.to_string()),
            port,
            is_spawn_mode: false,
            socket: Arc::new(Mutex::new(None)),
            process: None,
            running: Arc::new(Mutex::new(false)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            child_identity: Arc::new(Mutex::new(None)),
        })
    }

    /// Start the worker
    pub async fn start(&mut self) -> Result<()> {
        let mut socket = RouterSocket::new();
        
        if self.is_spawn_mode {
            eprintln!("Parent: Binding to tcp://0.0.0.0:{}", self.port);
            socket.bind(&format!("tcp://0.0.0.0:{}", self.port))
                .await
                .map_err(|e| {
                    eprintln!("Parent: Bind failed: {}", e);
                    MultifrostError::ZmqError(e.to_string())
                })?;
            eprintln!("Parent: Bind successful, starting child process");
            self.start_child_process()?;
        } else {
            eprintln!("Parent: Connecting to tcp://127.0.0.1:{}", self.port);
            socket.connect(&format!("tcp://127.0.0.1:{}", self.port))
                .await
                .map_err(|e| {
                    eprintln!("Parent: Connect failed: {}", e);
                    MultifrostError::ZmqError(e.to_string())
                })?;
        }
        
        *self.socket.lock().await = Some(socket);
        *self.running.lock().await = true;
        
        // Start message loop in background
        let socket = Arc::clone(&self.socket);
        let pending = Arc::clone(&self.pending);
        let running = Arc::clone(&self.running);
        let child_identity = Arc::clone(&self.child_identity);
        
        tokio::spawn(async move {
            message_loop(socket, pending, running, child_identity).await;
        });
        
        // Give child time to start and send ready message
        eprintln!("Parent: Waiting for child to be ready...");
        tokio::time::sleep(Duration::from_millis(10000)).await;
        
        // Check if child identity was received
        let id_guard = self.child_identity.lock().await;
        if id_guard.is_some() {
            eprintln!("Parent: Child identity received, ready!");
        } else {
            eprintln!("Parent: Warning - no child identity received yet");
        }
        
        Ok(())
    }

    fn start_child_process(&mut self) -> Result<()> {
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            if let Some(ref script) = self.script_path {
                c.args(["/C", &self.executable, script]);
            } else {
                c.args(["/C", &self.executable]);
            }
            c
        } else {
            let mut c = Command::new(&self.executable);
            if let Some(ref script) = self.script_path {
                c.arg(script);
            }
            c
        };
        
        cmd.env("COMLINK_ZMQ_PORT", self.port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        eprintln!("Parent: Spawning child: {:?}", cmd);
        self.process = Some(cmd.spawn()?);
        Ok(())
    }

    /// Call a remote function
    pub async fn call<T: serde::de::DeserializeOwned>(
        &self,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<T> {
        self.call_with_timeout(function, args, None).await
    }

    /// Call a remote function with timeout
    pub async fn call_with_timeout<T: serde::de::DeserializeOwned>(
        &self,
        function: &str,
        args: Vec<serde_json::Value>,
        timeout_ms: Option<u64>,
    ) -> Result<T> {
        if !*self.running.lock().await {
            return Err(MultifrostError::NotRunningError);
        }
        
        // Wait for child identity if in spawn mode
        let identity = {
            let id_guard = self.child_identity.lock().await;
            if id_guard.is_none() && self.is_spawn_mode {
                return Err(MultifrostError::NotRunningError);
            }
            id_guard.clone()
        };
        
        let message = Message::create_call(function, args);
        let msg_id = message.id.clone();
        
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(msg_id.clone(), tx);
        }
        
        // Send message - ROUTER needs [identity, empty, message]
        let packed = message.pack()?;
        eprintln!("Parent: Sending Call message, msg_id={}, packed_len={}", msg_id, packed.len());
        
        {
            let mut socket_guard = self.socket.lock().await;
            if let Some(ref mut socket) = *socket_guard {
                let zmq_msg: ZmqMessage = if let Some(ref id) = identity {
                    eprintln!("Parent: Sending to identity {:?} (len={})", id, id.len());
                    vec![
                        Bytes::from(id.clone()),
                        Bytes::from(vec![]),
                        Bytes::from(packed),
                    ].try_into()
                        .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?
                } else {
                    vec![
                        Bytes::from(vec![]),
                        Bytes::from(packed),
                    ].try_into()
                        .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?
                };
                
                socket.send(zmq_msg).await
                    .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
            } else {
                return Err(MultifrostError::NotRunningError);
            }
        }
        
        // Wait for response
        let inner_result: serde_json::Value = if let Some(timeout) = timeout_ms {
            match tokio::time::timeout(
                Duration::from_millis(timeout),
                rx
            ).await {
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
        
        serde_json::from_value(inner_result)
            .map_err(MultifrostError::JsonError)
    }

    /// Stop the worker
    pub async fn stop(&mut self) {
        *self.running.lock().await = false;
        
        // Cancel pending requests
        {
            let mut pending = self.pending.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(MultifrostError::NotRunningError));
            }
        }
        
        // Close socket
        let mut socket_guard = self.socket.lock().await;
        if let Some(socket) = socket_guard.take() {
            drop(socket);
        }
        
        // Kill child process
        if let Some(ref mut child) = self.process {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn find_free_port_sync() -> u16 {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr().map(|a| a.port()))
        .unwrap_or(5555)
}

async fn message_loop(
    socket: Arc<Mutex<Option<RouterSocket>>>,
    pending: PendingRequests,
    running: Arc<Mutex<bool>>,
    child_identity: ChildIdentity,
) {
    eprintln!("Parent: Message loop started");
    loop {
        if !*running.lock().await {
            eprintln!("Parent: Message loop stopping (not running)");
            break;
        }
        
        // Use timeout to avoid holding the lock indefinitely
        let msg_result = {
            let mut socket_guard = socket.lock().await;
            if let Some(ref mut sock) = *socket_guard {
                // Release lock after timeout so send can acquire it
                tokio::time::timeout(
                    Duration::from_millis(100),
                    sock.recv()
                ).await
            } else {
                eprintln!("Parent: Socket is None, breaking");
                break;
            }
        };
        
        // Handle timeout - just continue the loop
        let zmq_msg = match msg_result {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => {
                eprintln!("ZMQ receive error: {}", e);
                continue;
            }
            Err(_) => {
                // Timeout - continue loop to check running flag
                continue;
            }
        };
        
        let frames: Vec<_> = zmq_msg.into_vec();
        eprintln!("Parent: Received {} frames", frames.len());
        
        // ROUTER receives: [identity, empty, message_data]
        if frames.len() < 3 {
            eprintln!("Parent: Not enough frames, skipping");
            continue;
        }
        
        // Store child identity for future sends
        {
            let mut id_guard = child_identity.lock().await;
            if id_guard.is_none() {
                *id_guard = Some(frames[0].to_vec());
                eprintln!("Parent: Stored child identity");
            }
        }
        
        let message_data = frames[2].to_vec();
        
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
                    MessageType::Ready => {
                        // Child is ready - identity already stored above
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Failed to unpack message: {}", e);
            }
        }
    }
}
