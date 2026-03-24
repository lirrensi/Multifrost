//! FILE: rust/src/child.rs
//! PURPOSE: Own the service-side runtime and method-dispatch surface for the v5 API.
//! OWNS: ServiceWorker, ServiceContext, run_service, run_service_sync.
//! EXPORTS: ServiceWorker, ServiceContext, SyncServiceWorker, run_service, run_service_sync.
//! DOCS: agent_chat/rust_v5_api_surface_2026-03-24.md

use crate::caller_transport::{connect_ws, send_disconnect_frame};
use crate::error::{MultifrostError, Result};
use crate::message::{
    build_error_envelope, build_register_envelope, decode_call_body, decode_frame,
    encode_error_body, encode_frame, encode_register_body, ErrorBody, PeerClass, RegisterBody,
    KIND_CALL, KIND_DISCONNECT, KIND_ERROR, KIND_RESPONSE,
};
use crate::router_bootstrap::{connect_or_bootstrap, RouterBootstrapConfig};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = futures_util::stream::SplitSink<WsStream, Message>;

#[async_trait]
pub trait ServiceWorker: Send + Sync + 'static {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value>;

    fn list_functions(&self) -> Vec<String> {
        vec![]
    }
}

pub trait SyncServiceWorker: Send + Sync + 'static {
    fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value>;

    fn list_functions(&self) -> Vec<String> {
        vec![]
    }
}

struct SyncToAsyncAdapter<T: SyncServiceWorker>(T);

#[async_trait]
impl<T: SyncServiceWorker> ServiceWorker for SyncToAsyncAdapter<T> {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        self.0.handle_call(function, args)
    }

    fn list_functions(&self) -> Vec<String> {
        self.0.list_functions()
    }
}

#[derive(Clone)]
pub struct ServiceContext {
    namespace: String,
    service_id: Option<String>,
    entrypoint_path: Option<PathBuf>,
    router: RouterBootstrapConfig,
    request_timeout: Option<Duration>,
    extra_functions: HashMap<String, String>,
}

impl ServiceContext {
    pub fn new() -> Self {
        Self {
            namespace: "default".to_string(),
            service_id: None,
            entrypoint_path: None,
            router: RouterBootstrapConfig::from_env(),
            request_timeout: Some(Duration::from_secs(30)),
            extra_functions: HashMap::new(),
        }
    }

    pub fn with_service_id(mut self, service_id: &str) -> Self {
        self.service_id = Some(service_id.to_string());
        self
    }

    pub fn with_entrypoint_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.entrypoint_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_string();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn register_function(mut self, name: &str, description: &str) -> Self {
        self.extra_functions
            .insert(name.to_string(), description.to_string());
        self
    }

    pub fn list_functions(&self) -> Vec<String> {
        self.extra_functions.keys().cloned().collect()
    }

    fn resolve_peer_id(&self) -> Result<String> {
        if let Some(id) = &self.service_id {
            return Ok(id.clone());
        }

        if let Ok(explicit) = env::var("MULTIFROST_SERVICE_PEER_ID") {
            if !explicit.trim().is_empty() {
                return Ok(explicit);
            }
        }

        let path = self
            .entrypoint_path
            .clone()
            .or_else(|| env::var_os("MULTIFROST_ENTRYPOINT_PATH").map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from(".")));

        Ok(canonical_peer_path(path))
    }
}

impl Default for ServiceContext {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run_service<W: ServiceWorker>(worker: W, ctx: ServiceContext) {
    if let Err(err) = run_service_async(worker, ctx).await {
        eprintln!("Worker error: {err}");
        std::process::exit(1);
    }
}

pub fn run_service_sync<W: SyncServiceWorker>(worker: W, ctx: ServiceContext) {
    if let Err(err) = run_service_sync_inner(worker, ctx) {
        eprintln!("Worker error: {err}");
        std::process::exit(1);
    }
}

fn run_service_sync_inner<W: SyncServiceWorker>(worker: W, ctx: ServiceContext) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move { run_service_async(SyncToAsyncAdapter(worker), ctx).await })
}

async fn run_service_async<W: ServiceWorker>(worker: W, ctx: ServiceContext) -> Result<()> {
    let peer_id = ctx.resolve_peer_id()?;
    let endpoint = ctx.router.endpoint.ws_url();

    connect_or_bootstrap(&ctx.router).await?;
    let websocket = connect_ws(&endpoint).await?;
    let (sink, mut source) = websocket.split();
    let sink = Arc::new(Mutex::new(sink));

    let register = RegisterBody {
        peer_id: peer_id.clone(),
        class: PeerClass::Service,
    };
    let register_envelope = build_register_envelope(&peer_id, PeerClass::Service);
    let register_frame = encode_frame(&register_envelope, &encode_register_body(&register)?)?;
    send_ws_frame(&sink, register_frame).await?;

    let ack = match source.next().await {
        Some(Ok(Message::Binary(bytes))) => {
            let frame = decode_frame(&bytes)?;
            if frame.envelope.kind != KIND_RESPONSE {
                return Err(MultifrostError::RegisterRejected(format!(
                    "unexpected register reply kind: {}",
                    frame.envelope.kind
                )));
            }
            crate::message::decode_register_ack_body(&frame.body_bytes)?
        }
        Some(Ok(other)) => {
            return Err(MultifrostError::RegisterRejected(format!(
                "unexpected register reply message: {other:?}"
            )))
        }
        Some(Err(err)) => return Err(MultifrostError::RouterUnavailable(err.to_string())),
        None => return Err(MultifrostError::RegisterRejected("router closed".into())),
    };

    if !ack.accepted {
        return Err(MultifrostError::RegisterRejected(
            ack.reason
                .unwrap_or_else(|| "router rejected service registration".into()),
        ));
    }

    let worker = Arc::new(worker);
    let _ = ctx.list_functions();

    while let Some(message) = source.next().await {
        match message {
            Ok(Message::Binary(bytes)) => {
                let frame = match decode_frame(&bytes) {
                    Ok(frame) => frame,
                    Err(err) => {
                        eprintln!("malformed inbound frame: {err}");
                        continue;
                    }
                };

                if frame.envelope.to != peer_id {
                    continue;
                }

                match frame.envelope.kind.as_str() {
                    KIND_CALL => {
                        if let Err(err) =
                            handle_call_frame(&sink, &peer_id, worker.clone(), frame).await
                        {
                            eprintln!("service call handling error: {err}");
                        }
                    }
                    KIND_DISCONNECT => break,
                    KIND_ERROR | KIND_RESPONSE => continue,
                    _ => continue,
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(_) => continue,
            Err(err) => {
                eprintln!("service websocket error: {err}");
                break;
            }
        }
    }

    let _ = send_disconnect_frame(&sink, &peer_id).await;
    Ok(())
}

async fn handle_call_frame<W: ServiceWorker>(
    sink: &Arc<Mutex<WsSink>>,
    peer_id: &str,
    worker: Arc<W>,
    frame: crate::message::FrameParts,
) -> Result<()> {
    let call = match decode_call_body(&frame.body_bytes) {
        Ok(call) => call,
        Err(err) => {
            send_error(
                sink,
                peer_id,
                frame.envelope.from.as_str(),
                frame.envelope.msg_id.as_str(),
                "malformed_call",
                "protocol",
                err.to_string(),
                None,
            )
            .await?;
            return Ok(());
        }
    };

    if let Some(namespace) = call.namespace.as_deref() {
        if namespace != "default" {
            send_error(
                sink,
                peer_id,
                frame.envelope.from.as_str(),
                frame.envelope.msg_id.as_str(),
                "invalid_namespace",
                "protocol",
                format!("unsupported namespace: {namespace}"),
                None,
            )
            .await?;
            return Ok(());
        }
    }

    let reply = match worker.handle_call(&call.function, call.args).await {
        Ok(result) => {
            let envelope = build_response_envelope(
                peer_id,
                frame.envelope.from.as_str(),
                frame.envelope.msg_id.as_str(),
            );
            let body =
                crate::message::encode_response_body(&crate::message::ResponseBody { result })?;
            encode_frame(&envelope, &body)?
        }
        Err(err) => {
            let envelope = build_error_envelope(
                peer_id,
                frame.envelope.from.as_str(),
                frame.envelope.msg_id.as_str(),
            );
            let body = encode_error_body(&ErrorBody {
                code: "application_error".into(),
                message: err.to_string(),
                kind: "application".into(),
                stack: None,
                details: None,
            })?;
            encode_frame(&envelope, &body)?
        }
    };

    send_ws_frame(sink, reply).await?;
    Ok(())
}

async fn send_error(
    sink: &Arc<Mutex<WsSink>>,
    from_peer: &str,
    to_peer: &str,
    msg_id: &str,
    code: &str,
    kind: &str,
    message: String,
    details: Option<crate::message::WireValue>,
) -> Result<()> {
    let envelope = build_error_envelope(from_peer, to_peer, msg_id);
    let body = encode_error_body(&ErrorBody {
        code: code.into(),
        message,
        kind: kind.into(),
        stack: None,
        details,
    })?;
    send_ws_frame(sink, encode_frame(&envelope, &body)?).await
}

async fn send_ws_frame(sink: &Arc<Mutex<WsSink>>, frame: Vec<u8>) -> Result<()> {
    let mut sink = sink.lock().await;
    sink.send(Message::Binary(frame.into()))
        .await
        .map_err(|err| MultifrostError::WebSocketError(err.to_string()))
}

fn canonical_peer_path(path: PathBuf) -> String {
    path.canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn build_response_envelope(from: &str, to: &str, msg_id: &str) -> crate::message::Envelope {
    crate::message::Envelope {
        v: crate::message::PROTOCOL_VERSION,
        kind: KIND_RESPONSE.to_string(),
        msg_id: msg_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        ts: crate::message::now_ts(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{KIND_REGISTER, ROUTER_PEER_ID};
    use crate::router_bootstrap::{
        router_is_reachable, RouterBootstrapConfig, RouterEndpointConfig, ROUTER_BIN_ENV,
    };
    use crate::Connection;
    use bytes::Bytes;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout, Instant};
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    struct AddWorker;

    #[async_trait]
    impl ServiceWorker for AddWorker {
        async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
            match function {
                "add" => {
                    let a = args
                        .get(0)
                        .and_then(|value| value.as_i64())
                        .ok_or_else(|| MultifrostError::InvalidMessage("missing a".into()))?;
                    let b = args
                        .get(1)
                        .and_then(|value| value.as_i64())
                        .ok_or_else(|| MultifrostError::InvalidMessage("missing b".into()))?;
                    Ok(json!(a + b))
                }
                other => Err(MultifrostError::FunctionNotFound(other.to_string())),
            }
        }
    }

    struct EchoWorker;

    #[async_trait]
    impl ServiceWorker for EchoWorker {
        async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
            match function {
                "echo" => Ok(args.first().cloned().unwrap_or(Value::Null)),
                other => Err(MultifrostError::FunctionNotFound(other.to_string())),
            }
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn set_env_var(key: &'static str, value: &Path) -> EnvVarGuard {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        EnvVarGuard { key, original }
    }

    fn set_env_var_str(key: &'static str, value: &str) -> EnvVarGuard {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        EnvVarGuard { key, original }
    }

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("rust crate to have a parent repo directory")
            .to_path_buf()
    }

    fn write_router_launcher_script(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let repo_root = repo_root();
        let router_binary = repo_root.join("router/target/debug/multifrost-router");
        let script = dir.path().join("router-launcher.sh");
        let script_contents = format!("#!/bin/sh\nset -eu\nexec '{}'\n", router_binary.display());
        fs::write(&script, script_contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&script).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script, permissions).unwrap();
        }
        script
    }

    fn build_router_binary() {
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(repo_root().join("router/Cargo.toml"))
            .arg("--quiet")
            .arg("--bin")
            .arg("multifrost-router")
            .status()
            .unwrap();
        assert!(status.success(), "router binary build failed");
    }

    fn build_math_worker_example() {
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--example")
            .arg("math_worker")
            .arg("--quiet")
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
            .status()
            .unwrap();
        assert!(status.success(), "math_worker example build failed");
    }

    fn router_binary_path() -> std::path::PathBuf {
        repo_root().join("router/target/debug/multifrost-router")
    }

    fn math_worker_entrypoint_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/math_worker.rs")
    }

    fn math_worker_binary_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/examples/math_worker")
    }

    fn service_context(
        service_id: &str,
        port: u16,
        workspace: &tempfile::TempDir,
    ) -> ServiceContext {
        ServiceContext {
            namespace: "default".to_string(),
            service_id: Some(service_id.to_string()),
            entrypoint_path: None,
            router: RouterBootstrapConfig {
                endpoint: RouterEndpointConfig {
                    host: "127.0.0.1".into(),
                    port,
                },
                lock_path: workspace.path().join(format!("{service_id}.lock")),
                log_path: workspace.path().join(format!("{service_id}.log")),
                readiness_timeout: Duration::from_secs(30),
                readiness_poll_interval: Duration::from_millis(100),
            },
            request_timeout: Some(Duration::from_secs(5)),
            extra_functions: HashMap::new(),
        }
    }

    async fn wait_for_router(
        endpoint: &RouterEndpointConfig,
        log_path: &Path,
        worker_task: &mut Option<tokio::task::JoinHandle<Result<()>>>,
    ) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if router_is_reachable(endpoint).await {
                return;
            }
            if worker_task
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(false)
            {
                let result = worker_task.take().unwrap().await.unwrap();
                panic!("service worker exited before router became reachable: {result:?}");
            }
            if Instant::now() >= deadline {
                let log_excerpt = fs::read_to_string(log_path).unwrap_or_default();
                panic!(
                    "router did not become reachable at {}; log output:\n{}",
                    endpoint.ws_url(),
                    log_excerpt
                );
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    async fn wait_for_peer_exists(handle: &crate::Handle, peer_id: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if handle.query_peer_exists(peer_id).await.unwrap() {
                return;
            }
            if Instant::now() >= deadline {
                panic!("peer {peer_id} did not appear in router registry");
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    async fn start_service_router() -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut ws = match accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(_) => return,
                    };

                    let Some(register_frame) = read_binary_frame(&mut ws).await else {
                        return;
                    };
                    assert_eq!(register_frame.envelope.kind, KIND_REGISTER);
                    let register_body =
                        crate::message::decode_register_body(&register_frame.body_bytes).unwrap();
                    assert_eq!(register_body.class, PeerClass::Service);

                    let ack = crate::message::RegisterAckBody {
                        accepted: true,
                        reason: None,
                    };
                    let ack_envelope = crate::message::Envelope {
                        v: 5,
                        kind: KIND_RESPONSE.to_string(),
                        msg_id: register_frame.envelope.msg_id.clone(),
                        from: ROUTER_PEER_ID.to_string(),
                        to: register_body.peer_id.clone(),
                        ts: 0.0,
                    };
                    let ack_frame = crate::message::encode_frame(
                        &ack_envelope,
                        &crate::message::encode_register_ack_body(&ack).unwrap(),
                    )
                    .unwrap();
                    let _ = ws.send(Message::Binary(Bytes::from(ack_frame))).await;

                    let call_envelope =
                        crate::message::build_call_envelope(ROUTER_PEER_ID, &register_body.peer_id);
                    let call_body = crate::message::CallBody {
                        function: "add".into(),
                        args: vec![json!(2), json!(3)],
                        namespace: Some("default".into()),
                    };
                    let call_frame = crate::message::encode_frame(
                        &call_envelope,
                        &crate::message::encode_call_body(&call_body).unwrap(),
                    )
                    .unwrap();
                    let _ = ws.send(Message::Binary(Bytes::from(call_frame))).await;

                    let Some(response_frame) = read_binary_frame(&mut ws).await else {
                        return;
                    };
                    assert_eq!(response_frame.envelope.kind, KIND_RESPONSE);
                    let response =
                        crate::message::decode_response_body(&response_frame.body_bytes).unwrap();
                    assert_eq!(response.result, json!(5));

                    let _ = ws.close(None).await;
                });
            }
        });

        (port, handle)
    }

    async fn read_binary_frame(
        ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> Option<crate::message::FrameParts> {
        let message = match timeout(Duration::from_secs(2), ws.next()).await {
            Ok(Some(Ok(message))) => message,
            _ => return None,
        };
        match message {
            Message::Binary(bytes) => crate::message::decode_frame(bytes.as_ref()).ok(),
            _ => None,
        }
    }

    #[test]
    fn resolves_default_peer_id_from_current_exe() {
        let ctx = ServiceContext::new();
        let peer_id = ctx.resolve_peer_id().unwrap();
        assert!(!peer_id.is_empty());
    }

    #[test]
    fn explicit_service_id_wins_over_other_sources() {
        let ctx = ServiceContext::new().with_service_id("explicit-service");
        assert_eq!(ctx.resolve_peer_id().unwrap(), "explicit-service");
    }

    #[test]
    fn entrypoint_path_is_used_when_service_id_is_missing() {
        let dir = tempdir().unwrap();
        let entrypoint = dir.path().join("service.rs");
        fs::write(&entrypoint, "fn main() {}").unwrap();
        let ctx = ServiceContext::new().with_entrypoint_path(&entrypoint);
        assert_eq!(
            ctx.resolve_peer_id().unwrap(),
            entrypoint.canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn namespace_timeout_and_registered_functions_are_recorded() {
        let ctx = ServiceContext::new()
            .with_namespace("custom")
            .with_timeout(Duration::from_secs(5))
            .register_function("add", "Add two numbers")
            .register_function("mul", "Multiply two numbers");

        assert_eq!(ctx.namespace, "custom");
        assert_eq!(ctx.request_timeout, Some(Duration::from_secs(5)));

        let mut functions = ctx.list_functions();
        functions.sort();
        assert_eq!(functions, vec!["add".to_string(), "mul".to_string()]);
    }

    #[tokio::test]
    async fn run_service_async_handles_calls_over_mock_router() {
        let (port, router_task) = start_service_router().await;
        let ctx = ServiceContext {
            namespace: "default".to_string(),
            service_id: Some("service-1".to_string()),
            entrypoint_path: None,
            router: RouterBootstrapConfig {
                endpoint: crate::router_bootstrap::RouterEndpointConfig {
                    host: "127.0.0.1".into(),
                    port,
                },
                lock_path: std::env::temp_dir().join("multifrost-test.lock"),
                log_path: std::env::temp_dir().join("multifrost-test.log"),
                readiness_timeout: Duration::from_secs(1),
                readiness_poll_interval: Duration::from_millis(50),
            },
            request_timeout: Some(Duration::from_secs(2)),
            extra_functions: HashMap::new(),
        };

        let worker_task = tokio::spawn(async move { run_service_async(AddWorker, ctx).await });
        timeout(Duration::from_secs(3), worker_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        router_task.abort();
    }

    #[tokio::test]
    #[serial]
    async fn service_worker_bootstraps_router_and_second_worker_joins_existing_router() {
        let port_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = port_listener.local_addr().unwrap().port();
        drop(port_listener);

        build_router_binary();
        build_math_worker_example();

        let _router_bin =
            set_env_var_str(ROUTER_BIN_ENV, &router_binary_path().display().to_string());

        let service_entrypoint = math_worker_entrypoint_path();
        let service_binary = math_worker_binary_path();

        let service_process = crate::process::spawn_with_options(
            service_entrypoint.to_str().unwrap(),
            service_binary.to_str().unwrap(),
            crate::SpawnOptions {
                caller_peer_id: Some("caller-a".into()),
                request_timeout: Some(Duration::from_secs(10)),
                router_port: Some(port),
            },
        )
        .await
        .unwrap();
        let service = Connection::connect_with_options(
            service_entrypoint.to_str().unwrap(),
            crate::ConnectOptions {
                caller_peer_id: Some("caller-a".into()),
                request_timeout: Some(Duration::from_secs(10)),
                router_port: Some(port),
            },
        )
        .await
        .unwrap()
        .with_service_process(service_process);
        let service_handle = service.handle();
        wait_for_peer_exists(&service_handle, service_entrypoint.to_str().unwrap()).await;
        let sum: i64 = service_handle
            .call("add", vec![json!(10), json!(20)])
            .await
            .unwrap();
        assert_eq!(sum, 30);

        let caller = Connection::connect_with_options(
            service_entrypoint.to_str().unwrap(),
            crate::ConnectOptions {
                caller_peer_id: Some("caller-b".into()),
                request_timeout: Some(Duration::from_secs(10)),
                router_port: Some(port),
            },
        )
        .await
        .unwrap();
        let caller_handle = caller.handle();
        assert!(caller_handle
            .query_peer_exists(service_entrypoint.to_str().unwrap())
            .await
            .unwrap());
        let product: i64 = caller_handle
            .call("multiply", vec![json!(6), json!(7)])
            .await
            .unwrap();
        assert_eq!(product, 42);

        service_handle.stop().await;
        caller_handle.stop().await;
    }
}
