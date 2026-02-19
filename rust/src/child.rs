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
        self.functions
            .insert(name.to_string(), description.to_string());
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
            let port: u16 = port_str
                .parse()
                .map_err(|_| MultifrostError::InvalidMessage("Invalid port".to_string()))?;
            if !(1024..=65535).contains(&port) {
                return Err(MultifrostError::InvalidMessage(format!(
                    "Port {} out of valid range (1024-65535)",
                    port
                )));
            }
            socket
                .connect(&format!("tcp://127.0.0.1:{}", port))
                .await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        } else if let Some(ref service_id) = ctx_guard.service_id {
            // Service mode: register service and bind
            let port = ServiceRegistry::register(service_id).await?;
            socket
                .bind(&format!("tcp://0.0.0.0:{}", port))
                .await
                .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
        } else {
            return Err(MultifrostError::InvalidMessage(
                "Need COMLINK_ZMQ_PORT env or service_id".to_string(),
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
            Bytes::from(vec![]), // Empty frame for DEALER
            Bytes::from(packed),
        ]
        .try_into()
        .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?;

        let mut ctx_guard = ctx.write().await;
        if let Some(ref mut socket) = ctx_guard.socket {
            socket
                .send(zmq_msg)
                .await
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
        return Err(MultifrostError::InvalidMessage(
            "Not enough frames".to_string(),
        ));
    }

    let sender_id = frames[0].to_vec();
    let message_data = frames[2].to_vec();
    let message = Message::unpack(&message_data)?;

    if !message.is_valid() {
        return Err(MultifrostError::InvalidMessage(
            "Invalid message".to_string(),
        ));
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
            let original_ts = message
                .metadata
                .as_ref()
                .and_then(|m| m.get("hb_timestamp"))
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| current_timestamp());
            Message::create_heartbeat_response(&msg_id, original_ts)
        }
        MessageType::Ready
        | MessageType::Response
        | MessageType::Error
        | MessageType::Stdout
        | MessageType::Stderr
        | MessageType::Unknown => {
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
        ]
        .try_into()
        .map_err(|_| MultifrostError::InvalidMessage("Empty message".to_string()))?;
        socket
            .send(reply)
            .await
            .map_err(|e| MultifrostError::ZmqError(e.to_string()))?;
    }

    Ok(())
}

async fn handle_function_call<W: ChildWorker>(worker: Arc<W>, message: &Message) -> Message {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Test implementation of SyncChildWorker
    struct TestSyncWorker;

    impl SyncChildWorker for TestSyncWorker {
        fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
            match function {
                "add" => {
                    let a = args.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                    let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                    Ok(json!(a + b))
                }
                "echo" => Ok(args.get(0).cloned().unwrap_or(json!(null))),
                "fail" => Err(MultifrostError::RemoteCallError(
                    "Intentional failure".to_string(),
                )),
                _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
            }
        }

        fn list_functions(&self) -> Vec<String> {
            vec!["add".to_string(), "echo".to_string(), "fail".to_string()]
        }
    }

    // Test implementation of async ChildWorker
    struct TestAsyncWorker;

    #[async_trait::async_trait]
    impl ChildWorker for TestAsyncWorker {
        async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
            match function {
                "multiply" => {
                    let a = args.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                    let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                    Ok(json!(a * b))
                }
                "delay" => {
                    let ms = args.get(0).and_then(|v| v.as_u64()).unwrap_or(10);
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                    Ok(json!("done"))
                }
                _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
            }
        }
    }

    #[test]
    fn test_child_worker_context_new() {
        let ctx = ChildWorkerContext::new();
        assert_eq!(ctx.namespace, "default");
        assert!(ctx.service_id.is_none());
        assert!(!ctx.running);
        assert!(ctx.socket.is_none());
        assert!(ctx.functions.is_empty());
    }

    #[test]
    fn test_child_worker_context_with_service_id() {
        let ctx = ChildWorkerContext::new().with_service_id("test-service");
        assert_eq!(ctx.service_id, Some("test-service".to_string()));
    }

    #[test]
    fn test_child_worker_context_with_namespace() {
        let ctx = ChildWorkerContext::new().with_namespace("custom-namespace");
        assert_eq!(ctx.namespace, "custom-namespace");
    }

    #[test]
    fn test_child_worker_context_register_function() {
        let ctx = ChildWorkerContext::new()
            .register_function("add", "Adds two numbers")
            .register_function("multiply", "Multiplies two numbers");

        assert!(ctx.functions.contains_key("add"));
        assert!(ctx.functions.contains_key("multiply"));
        assert_eq!(
            ctx.functions.get("add"),
            Some(&"Adds two numbers".to_string())
        );
    }

    #[test]
    fn test_child_worker_context_list_functions() {
        let ctx = ChildWorkerContext::new()
            .register_function("add", "Adds two numbers")
            .register_function("multiply", "Multiplies two numbers");

        let functions = ctx.list_functions();
        assert_eq!(functions.len(), 2);
        assert!(functions.contains(&"add".to_string()));
        assert!(functions.contains(&"multiply".to_string()));
    }

    #[test]
    fn test_child_worker_context_default() {
        let ctx = ChildWorkerContext::default();
        assert_eq!(ctx.namespace, "default");
        assert!(ctx.service_id.is_none());
    }

    #[test]
    fn test_child_worker_context_builder_chain() {
        let ctx = ChildWorkerContext::new()
            .with_service_id("my-service")
            .with_namespace("my-namespace")
            .register_function("func1", "Description 1")
            .register_function("func2", "Description 2");

        assert_eq!(ctx.service_id, Some("my-service".to_string()));
        assert_eq!(ctx.namespace, "my-namespace");
        assert_eq!(ctx.functions.len(), 2);
    }

    #[tokio::test]
    async fn test_sync_child_worker_add() {
        let worker = TestSyncWorker;
        let result = worker.handle_call("add", vec![json!(5), json!(3)]).unwrap();
        assert_eq!(result, json!(8));
    }

    #[tokio::test]
    async fn test_sync_child_worker_echo() {
        let worker = TestSyncWorker;
        let result = worker.handle_call("echo", vec![json!("hello")]).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[tokio::test]
    async fn test_sync_child_worker_fail() {
        let worker = TestSyncWorker;
        let result = worker.handle_call("fail", vec![]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sync_child_worker_not_found() {
        let worker = TestSyncWorker;
        let result = worker.handle_call("unknown", vec![]);
        assert!(result.is_err());
        assert!(matches!(result, Err(MultifrostError::FunctionNotFound(_))));
    }

    #[tokio::test]
    async fn test_sync_child_worker_list_functions() {
        let worker = TestSyncWorker;
        let functions = worker.list_functions();
        assert_eq!(functions.len(), 3);
        assert!(functions.contains(&"add".to_string()));
        assert!(functions.contains(&"echo".to_string()));
        assert!(functions.contains(&"fail".to_string()));
    }

    #[tokio::test]
    async fn test_async_child_worker_multiply() {
        let worker = TestAsyncWorker;
        let result = worker
            .handle_call("multiply", vec![json!(4), json!(7)])
            .await
            .unwrap();
        assert_eq!(result, json!(28));
    }

    #[tokio::test]
    async fn test_async_child_worker_delay() {
        let worker = TestAsyncWorker;
        let start = std::time::Instant::now();
        let result = worker.handle_call("delay", vec![json!(50)]).await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result, json!("done"));
        assert!(elapsed.as_millis() >= 45);
    }

    #[tokio::test]
    async fn test_async_child_worker_not_found() {
        let worker = TestAsyncWorker;
        let result = worker.handle_call("unknown", vec![]).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MultifrostError::FunctionNotFound(_))));
    }

    #[tokio::test]
    async fn test_handle_function_call_success() {
        let worker = Arc::new(TestAsyncWorker);
        let msg = Message::create_call("multiply", vec![json!(3), json!(5)]);
        let response = handle_function_call(worker, &msg).await;

        assert_eq!(response.msg_type, MessageType::Response);
        assert_eq!(response.result, Some(json!(15)));
    }

    #[tokio::test]
    async fn test_handle_function_call_error() {
        // Wrap sync worker in adapter for async use
        let worker = Arc::new(SyncToAsyncAdapter(TestSyncWorker));
        let msg = Message::create_call("fail", vec![]);
        let response = handle_function_call(worker, &msg).await;

        assert_eq!(response.msg_type, MessageType::Error);
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_handle_function_call_private_method() {
        let worker = Arc::new(TestAsyncWorker);
        let msg = Message::create_call("_private", vec![]);
        let response = handle_function_call(worker, &msg).await;

        assert_eq!(response.msg_type, MessageType::Error);
        assert!(response.error.unwrap().contains("private method"));
    }

    #[tokio::test]
    async fn test_handle_function_call_missing_function() {
        let worker = Arc::new(TestAsyncWorker);
        let mut msg = Message::new(MessageType::Call);
        msg.args = Some(vec![]);
        // No function name set
        let response = handle_function_call(worker, &msg).await;

        assert_eq!(response.msg_type, MessageType::Error);
        assert!(response.error.unwrap().contains("Missing function name"));
    }

    #[tokio::test]
    async fn test_handle_function_call_not_found() {
        let worker = Arc::new(TestAsyncWorker);
        let msg = Message::create_call("nonexistent", vec![]);
        let response = handle_function_call(worker, &msg).await;

        assert_eq!(response.msg_type, MessageType::Error);
        assert!(response.error.unwrap().contains("Function not found"));
    }

    #[tokio::test]
    async fn test_sync_to_async_adapter() {
        let sync_worker = TestSyncWorker;
        let adapter = SyncToAsyncAdapter(sync_worker);

        // The adapter should work like the sync worker
        let result = adapter
            .handle_call("add", vec![json!(10), json!(20)])
            .await
            .unwrap();
        assert_eq!(result, json!(30));
    }
}
