use crate::error::{MultifrostError, Result};
use crate::message::{Message, MessageType};
use crate::registry::ServiceRegistry;
use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

fn current_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Async child worker trait - for workers that need async processing
#[async_trait]
pub trait ChildWorker: Send + Sync + 'static {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value>;
}

/// Sync child worker trait - for simple CPU-bound workers
/// Implement this trait if your worker doesn't need async operations
pub trait SyncChildWorker: Send + Sync + 'static {
    fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value>;

    /// Get list of available functions for introspection
    fn list_functions(&self) -> Vec<String> {
        vec![]
    }
}

/// Adapter to allow SyncChildWorker to be used where ChildWorker is expected
struct SyncToAsyncAdapter<T: SyncChildWorker>(T);

#[async_trait]
impl<T: SyncChildWorker> ChildWorker for SyncToAsyncAdapter<T> {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        // SyncChildWorker::handle_call is sync, so we just call it directly
        // It's marked async to satisfy the async_trait bound, but doesn't actually await
        self.0.handle_call(function, args)
    }
}

pub struct ChildWorkerContext {
    namespace: String,
    service_id: Option<String>,
    running: bool,
    socket: Option<RouterSocket>,
    functions: HashMap<String, String>, // function_name -> description
}

impl ChildWorkerContext {
    pub fn new() -> Self {
        Self {
            namespace: "default".to_string(),
            service_id: None,
            running: false,
            socket: None,
            functions: HashMap::new(),
        }
    }

    pub fn with_service_id(mut self, service_id: &str) -> Self {
        self.service_id = Some(service_id.to_string());
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_string();
        self
    }

    /// Register a function for introspection
    pub fn register_function(mut self, name: &str, description: &str) -> Self {
        self.functions.insert(name.to_string(), description.to_string());
        self
    }

    /// Get list of registered functions
    pub fn list_functions(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }
}

impl Default for ChildWorkerContext {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run_worker<W: ChildWorker>(worker: W, ctx: ChildWorkerContext) {
    if let Err(e) = run_worker_async(worker, ctx).await {
        eprintln!("Worker error: {}", e);
        std::process::exit(1);
    }
}

/// Run a sync worker (CPU-bound, no async needed)
pub fn run_worker_sync<W: SyncChildWorker>(worker: W, ctx: ChildWorkerContext) {
    if let Err(e) = run_worker_sync_inner(worker, ctx) {
        eprintln!("Worker error: {}", e);
        std::process::exit(1);
    }
}

fn run_worker_sync_inner<W: SyncChildWorker>(worker: W, ctx: ChildWorkerContext) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Wrap sync worker in adapter to implement ChildWorker
        let adapter = SyncToAsyncAdapter(worker);
        run_worker_async(adapter, ctx).await
    })
}

async fn run_worker_async<W: ChildWorker>(worker: W, ctx: ChildWorkerContext) -> Result<()> {
    let ctx = Arc::new(RwLock::new(ctx));
    let is_spawn_mode;

    // Setup ZMQ socket
    {
        let mut ctx_guard = ctx.write().await;
        let port_env = env::var("COMLINK_ZMQ_PORT").ok();
        is_spawn_mode = port_env.is_some();

        let mut socket = RouterSocket::new();

        if let Some(port_str) = port_env {
            // Spawn mode: connect to parent's port
            let port: u16 = port_str.parse()
                .map_err(|_| MultifrostError::InvalidMessage("Invalid port".to_string()))?;
            if !(1024..=65535).contains(&port) {
                return Err(MultifrostError::InvalidMessage(
                    format!("Port {} out of valid range (1024-65535)", port)
                ));
            }
            socket.connect(&format!("tcp://127.0.0.1:{}", port)).await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        } else if let Some(ref service_id) = ctx_guard.service_id {
            // Service mode: register service and bind
            let port = ServiceRegistry::register(service_id).await?;
            socket.bind(&format!("tcp://0.0.0.0:{}", port)).await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        } else {
            return Err(MultifrostError::InvalidMessage(
                "Need COMLINK_ZMQ_PORT env or service_id".to_string()
            ));
        }

        ctx_guard.socket = Some(socket);
        ctx_guard.running = true;
    }

    // Send ready message in spawn mode
    if is_spawn_mode {
        let ready_msg = Message::create_ready();
        let packed = ready_msg.pack()?;

        // ROUTER sends: [empty, message] when connecting to parent's DEALER
        let zmq_msg: ZmqMessage = vec![
            Bytes::from(vec![]),  // Empty frame for DEALER
            Bytes::from(packed),
        ].try_into()
            .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?;

        let mut ctx_guard = ctx.write().await;
        if let Some(ref mut socket) = ctx_guard.socket {
            socket.send(zmq_msg).await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        }
    }

    // Message loop
    let worker = Arc::new(worker);

    loop {
        let ctx_clone = Arc::clone(&ctx);
        let worker_clone = Arc::clone(&worker);

        let msg_result = {
            let mut ctx_guard = ctx_clone.write().await;
            if !ctx_guard.running {
                break;
            }
            if let Some(ref mut socket) = ctx_guard.socket {
                socket.recv().await
            } else {
                break;
            }
        };

        match msg_result {
            Ok(zmq_msg) => {
                if let Err(e) = handle_message(ctx_clone, worker_clone, zmq_msg).await {
                    eprintln!("Error handling message: {}", e);
                }
            }
            Err(e) => {
                eprintln!("ZMQ receive error: {}", e);
            }
        }
    }

    // Cleanup
    {
        let mut ctx_guard = ctx.write().await;
        ctx_guard.running = false;
        if let Some(ref service_id) = ctx_guard.service_id {
            let _ = ServiceRegistry::unregister(service_id).await;
        }
    }

    Ok(())
}

async fn handle_message<W: ChildWorker>(
    ctx: Arc<RwLock<ChildWorkerContext>>,
    worker: Arc<W>,
    zmq_msg: ZmqMessage,
) -> Result<()> {
    // ROUTER receives: [sender_id, empty, message_data]
    let frames: Vec<_> = zmq_msg.into_vec();

    if frames.len() < 3 {
        return Err(MultifrostError::InvalidMessage("Not enough frames".to_string()));
    }

    let sender_id = frames[0].to_vec();
    let message_data = frames[2].to_vec();
    let message = Message::unpack(&message_data)?;

    if !message.is_valid() {
        return Err(MultifrostError::InvalidMessage("Invalid message".to_string()));
    }

    // Check namespace
    {
        let ctx_guard = ctx.read().await;
        if let Some(ref ns) = message.namespace {
            if ns != &ctx_guard.namespace {
                return Ok(()); // Ignore wrong namespace
            }
        }
    }

    let response = match message.msg_type {
        MessageType::Call => {
            // Check for introspection calls
            if message.function.as_ref().map(|s| s.as_str()) == Some("listFunctions") {
                let ctx_guard = ctx.read().await;
                let functions = ctx_guard.list_functions();
                Message::create_response(serde_json::json!(functions), &message.id)
            } else {
                handle_function_call(worker, &message).await
            }
        }
        MessageType::Shutdown => {
            let mut ctx_guard = ctx.write().await;
            ctx_guard.running = false;
            return Ok(());
        }
        MessageType::Heartbeat => {
            // Handle heartbeat - echo back with response
            let msg_id = message.id.clone();
            let original_ts = message.metadata
                .as_ref()
                .and_then(|m| m.get("hb_timestamp"))
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| current_timestamp());
            Message::create_heartbeat_response(&msg_id, original_ts)
        }
        MessageType::Ready | MessageType::Response | MessageType::Error | MessageType::Stdout | MessageType::Stderr | MessageType::Unknown => {
            return Ok(());
        }
    };

    // Send response - ROUTER sends: [sender_id, empty, message]
    let response_data = response.pack()?;

    let mut ctx_guard = ctx.write().await;
    if let Some(ref mut socket) = ctx_guard.socket {
        let reply: ZmqMessage = vec![
            Bytes::from(sender_id),
            Bytes::from(vec![]),
            Bytes::from(response_data),
        ].try_into()
            .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?;
        socket.send(reply).await
            .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
    }

    Ok(())
}

async fn handle_function_call<W: ChildWorker>(
    worker: Arc<W>,
    message: &Message,
) -> Message {
    let msg_id = message.id.clone();

    match message.function {
        Some(ref func) => {
            let args = message.args.clone().unwrap_or_default();

            // Prevent calling private methods
            if func.starts_with('_') {
                return Message::create_error(
                    &format!("Cannot call private method '{}'", func),
                    &msg_id,
                );
            }

            match worker.handle_call(func, args).await {
                Ok(result) => Message::create_response(result, &msg_id),
                Err(e) => Message::create_error(&e.to_string(), &msg_id),
            }
        }
        None => Message::create_error("Missing function name", &msg_id),
    }
}
