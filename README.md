# Multifrost 🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈

> **"Bifrost but more powerful!"** - A rainbow bridge for your environments across spaces. Microservices made easy. Ditch making REST API connectors everywhere!

Multifrost is a high-performance inter-process communication (IPC) library that enables seamless communication between processes written in different languages. Think of it as a language-agnostic bridge that lets Python talk to Node.js, Go talk to Rust, and everything in between—without writing a single REST API or gRPC endpoint.

> Previously was **comlink_py** - you can find old version in __python/legacy/__

## ✨ Why Multifrost?

- **Zero-API, Zero-Latency**: Call remote functions like they're local. No HTTP overhead, no serialization complexity.
- **Cross-Language**: Python ↔ Node.js ↔ Go ↔ Rust ↔ Anything with a runtime
- **Two Modes**: Spawn (create workers) or Connect (service discovery)
- **Battle-Tested**: Built on ZeroMQ and msgpack for maximum performance
- **Async-Native**: Modern async/sync APIs in all languages

## 💡 Common Use Cases

### Bridge Two Python Environments
```python
# In your main app (parent)
from multifrost import ParentWorker

worker = ParentWorker.spawn("worker.py")
worker.sync.start()

# Call functions from the isolated environment
result = worker.sync.call.process_data(json_data)
```

```python
# In worker.py (child)
from multifrost import ChildWorker

class DataWorker(ChildWorker):
    def process_data(self, data: dict) -> dict:
        # Your processing logic here
        return {"result": "processed"}

DataWorker().run()
```

### Call ML Apps from Node.js
```typescript
import { ParentWorker } from 'multifrost';

const modelWorker = await ParentWorker.spawn('./model-service.ts', 'tsx');

// Call your PyTorch/TensorFlow model
const prediction = await modelWorker.call.predict([0.1, 0.2, 0.3]);
console.log(prediction);
```

### Connect Two Go Microservices
```go
package main

import (
    "context"
    "log"
    "github.com/multifrost/golang"
)

func main() {
    // Spawn a Go worker
    worker := multifrost.Spawn("examples/math_worker", "go", "run")
    if err := worker.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer worker.Close()

    // Call remote functions
    ctx := context.Background()

    result, err := worker.ACall.Call(ctx, "Add", 5, 3)
    if err != nil {
        log.Printf("Add failed: %v", err)
    } else {
        log.Printf("Add(5, 3) = %v\n", result)
    }

    result, err = worker.ACall.Call(ctx, "Multiply", 6, 7)
    if err != nil {
        log.Printf("Multiply failed: %v", err)
    } else {
        log.Printf("Multiply(6, 7) = %v\n", result)
    }
}
```

### Offload Heavy Compute to Rust
```rust
use multifrost::ParentWorker;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn Rust worker (compiled binary for maximum performance)
    let mut worker = ParentWorker::spawn("examples/math_worker.rs", None).await?;

    // Call factorial computation
    let result: u64 = worker.call("factorial", vec![
        json!(20),
    ]).await?;
    println!("Factorial of 20 = {}", result);

    worker.stop().await;
    Ok(())
}
```

```rust
//! math_worker.rs - Rust worker implementation
use multifrost::{ChildWorker, ChildWorkerContext, Result, run_worker};
use async_trait::async_trait;
use serde_json::Value;

struct MathWorker;

#[async_trait]
impl ChildWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "factorial" => {
                let n = args.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                let result: u64 = (1..=n).product();
                Ok(serde_json::json!(result))
            }
            "fibonacci" => {
                let n = args.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                let result = fibonacci(n);
                Ok(serde_json::json!(result))
            }
            _ => Err(multifrost::MultifrostError::FunctionNotFound(function.to_string()))
        }
    }
}

fn fibonacci(n: u64) -> u64 {
    if n == 0 { return 0; }
    if n == 1 { return 1; }

    let (mut a, mut b) = (0u64, 1u64);
    for _ in 2..=n {
        let temp = a.wrapping_add(b);
        a = b;
        b = temp;
    }
    b
}

#[tokio::main]
async fn main() {
    // Spawn mode (parent will provide port via env)
    let ctx = ChildWorkerContext::new();
    run_worker(MathWorker, ctx).await;
}
```

## 📚 Documentation Index

### Core Documentation
- **[Architecture Overview](docs/arch.md)** - Core design principles and protocol
- **[Protocol Specification](docs/protocol.md)** - Message format and wire protocol
- **[MsgPack Interop](docs/msgpack_interop.md)** - Cross-language data handling
- **[Get Started](examples/)** - Full examples for each language

### Language-Specific Documentation

#### Python
- **[Python Architecture](python/docs/arch.md)** - Async-first design, asyncio integration
- **[Python AGENTS Guide](python/AGENTS.md)** - Development guidelines and patterns
- **[Python Examples](python/examples/)** - Parent-child patterns, async workers

#### JavaScript / TypeScript
- **[JavaScript Architecture](javascript/docs/arch.md)** - Event-driven, proxy-based API
- **[JavaScript AGENTS Guide](javascript/AGENTS.md)** - Development guidelines
- **[JavaScript Examples](javascript/examples/)** - Node.js workers, TypeScript

#### Go
- **[Go Architecture](golang/docs/arch.md)** - Goroutines, reflection, type safety
- **[Go AGENTS Guide](golang/AGENTS.md)** - Development guidelines
- **[Go Examples](golang/examples/)** - Microservice patterns, spawn/connect

#### Rust
- **[Rust Architecture](rust/docs/arch.md)** - Type-safe, async-native, Arc<RwLock<>>
- **[Rust AGENTS Guide](rust/AGENTS.md)** - Development guidelines
- **[Rust Examples](rust/examples/)** - Heavy compute, async patterns

### Cross-Language Reference
- **[Error Handling Patterns](docs/error_patterns.md)** - Consistent error handling across languages
- **[Type Mapping](docs/type_mapping.md)** - Data type compatibility between languages
- **[Performance Tips](docs/performance.md)** - Optimization strategies

## 🏗️ How It Works

```
┌─────────────────┐       ┌─────────────────┐
│  Parent Process │       │  Child Process  │
│   (Any Lang)    │       │   (Any Lang)    │
│                 │       │                 │
│  ┌───────────┐  │       │  ┌───────────┐  │
│  │ DEALER    │  │       │  │ ROUTER    │  │
│  │ Socket    │◄─┼───────┼──►│ Socket   │  │
│  └───────────┘  │       │  └───────────┘  │
│                 │       │                 │
│  • Spawns child │       │  • Exposes methods
│  • Manages life-│       │  • Handles calls
│    cycle        │       │  • Routes messages
└─────────────────┘       └─────────────────┘
          │                              │
          └──────── ZeroMQ over TCP ──────┘
               msgpack encoded
```

### Core Concepts

1. **Parent & Child Roles**: Parent initiates calls and manages lifecycle; Child exposes callable methods.

2. **Two Communication Modes**:
    - **Spawn**: Parent creates child process with `COMLINK_ZMQ_PORT` environment variable
    - **Connect**: Child registers in service registry (`~/.multifrost/services.json`)

3. **Message Flow**: All messages use ZeroMQ's multipart framing with msgpack encoding:
    - CALL → RESPONSE (success) or ERROR (failure)
    - Full bidirectional communication

4. **Protocol Version**: Uses `comlink_ipc_v3` app ID for version negotiation

## 🎯 Features

### Parent Worker
- ✅ Spawn mode (create new processes)
- ✅ Connect mode (service discovery)
- ✅ Circuit breaker pattern (auto-restart failed workers)
- ✅ Heartbeat monitoring (detect dead workers)
- ✅ Metrics collection (request/response times, error rates)
- ✅ Timeout support (configurable per call)
- ✅ Async-native API

### Child Worker
- ✅ Sync and async method handlers
- ✅ stdout/stderr forwarding to parent
- ✅ Graceful shutdown (SIGINT/SIGTERM handling)
- ✅ Function introspection (`listFunctions()`)
- ✅ Namespace support (multiple workers per app)

## 🔧 Installation

### Python
```bash
# Using pip
pip install git+https://github.com/lirrensi/Multifrost.git

# Using uv
uv pip install git+https://github.com/lirrensi/Multifrost.git

# Using pipx
pipx install git+https://github.com/lirrensi/Multifrost.git

# Using uvx (direct run)
uvx --from git+https://github.com/lirrensi/Multifrost.git
```

### JavaScript / TypeScript
```bash
npm install git+https://github.com/lirrensi/Multifrost.git

# Or use with tsx for TypeScript files
npx tsx worker.ts
```

### Go
```bash
go get https://github.com/lirrensi/Multifrost.git
```

### Rust
```bash
cargo install --git https://github.com/lirrensi/Multifrost.git
```

### Development
```bash
# Install all implementations
make install

# Run all tests
make test

# Run tests for specific language
make test-python
make test-javascript
```

## 📖 Examples

Check out the full examples in the repository:

### Python
- **[Python Examples](python/examples/)** - Parent-child patterns, async workers, concurrency

### JavaScript / TypeScript
- **[JavaScript Examples](javascript/examples/)** - Node.js workers, TypeScript, service discovery

### Go
- **[Go Examples](golang/examples/)** - Microservice patterns, spawn/connect modes
  - `math_worker/` - Simple math operations worker
  - `parent_spawn/` - Parent spawns child worker
  - `go_calls_python/` - Go calls Python worker

### Rust
- **[Rust Examples](rust/examples/)** - Heavy compute, async patterns
  - `math_worker.rs` - Math operations worker
  - `parent.rs` - Parent connects to service
  - `spawn.rs` - Parent spawns Rust worker
  - `connect.rs` - Parent connects to Rust service

## 🛡️ Security

- Protocol is **unauthenticated and unencrypted**
- Use on **localhost or trusted networks only**
- Validate all inputs in worker methods (args are untrusted)
- Never expose ports to untrusted clients

## 🌟 Contributing

Contributions are welcome! See each language's directory for implementation-specific guidelines:

- [Python AGENTS.md](python/AGENTS.md)
- [JavaScript AGENTS.md](javascript/AGENTS.md)
- [Go AGENTS.md](golang/AGENTS.md)
- [Rust AGENTS.md](rust/AGENTS.md)

## 📝 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

Built on top of **ZeroMQ** for socket communication and **msgpack** for efficient serialization. Inspired by the concept of Bifrost but designed for modern, cross-language microservice architectures.

---

**Ready to bridge your worlds?** Start with the [Get Started Guide](examples/) and build your first cross-language IPC system! 🚀
