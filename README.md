# Multifrost 🌈

> Cross-language IPC over ZeroMQ and MessagePack. Call worker methods across Python, JavaScript, Go, and Rust without building REST or gRPC glue.

Multifrost is a high-performance inter-process communication (IPC) library that enables seamless communication between processes written in different languages. Think of it as a language-agnostic bridge that lets Python talk to Node.js, Go talk to Rust, and everything in between—without writing a single REST API or gRPC endpoint.

> Previously was **comlink_py** - you can find old version in __python/legacy/__

## ✨ Why Multifrost?

- **Zero-API, Zero-Latency**: Call remote functions like they're local. No HTTP overhead, no serialization complexity.
- **Cross-Language**: Python ↔ Node.js ↔ Go ↔ Rust ↔ Anything with a runtime
- **Two Modes**: Spawn (create workers) or Connect (service discovery)
- **Battle-Tested**: Built on ZeroMQ and msgpack for maximum performance
- **Async-Native**: Modern async/sync APIs in all languages

## ✅ What Works Well Today

- **4 runtimes, 12 language pairs**: Python, JavaScript/TypeScript, Go, and Rust share the same wire protocol and are designed to interoperate with one another
- **One protocol contract**: DEALER/ROUTER over ZeroMQ, MessagePack payloads, and the same core message types across runtimes
- **Shared lifecycle model**: `.spawn()` / `.connect()` on the parent side and `.handle().start()` / `.handle().stop()` for runtime control
- **Broader interop coverage**: Recent e2e work focused on registry behavior, heartbeat consistency, restart handling, and numeric round-trips across languages

## ⚠️ Interop Rules You Should Know

- **Portable numbers**: Treat signed int64 integers and finite floats as the safe cross-language numeric subset
- **Precision-sensitive values**: Encode huge integers, decimals, money, tensors, and exact float-bit payloads explicitly at the application layer
- **String-keyed objects only**: Non-string map keys are not portable across all runtimes
- **Source of truth**: See `docs/msgpack_interop.md` for wire-value rules and `docs/support_matrix.md` for current parity notes

## 🚀 Release Highlights

- **More cross-language tests**: Added larger e2e coverage across the runtime matrix, including JavaScript numeric boundary checks against Python workers
- **Better registry and heartbeat alignment**: Runtime behavior now matches more closely across Python, JavaScript, Go, and Rust
- **Clearer docs for real-world interop**: The README, protocol docs, and MessagePack guide now call out the supported subset instead of implying every edge case is identical

For the running release log, see `CHANGELOG.md`.

## 💡 Common Use Cases

### Python: Spawn a Worker

```python
# parent.py - Spawn and call a Python worker
import asyncio
from multifrost import ParentWorker

async def main():
    # Spawn the child process (v4 API)
    worker = ParentWorker.spawn("worker.py")
    handle = worker.handle()
    await handle.start()

    # Call remote functions
    result = await handle.call.add(5, 3)
    print(f"5 + 3 = {result}")

    await handle.stop()

asyncio.run(main())
```

```python
# worker.py - Child worker implementation
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def add(self, a: int, b: int) -> int:
        return a + b

    def factorial(self, n: int) -> int:
        import math
        return math.factorial(n)

MathWorker().run()
```

### Node.js → Python: Call ML Models

```typescript
// Node.js parent calling a Python ML worker
import { ParentWorker } from 'multifrost';

async function main() {
    // Spawn a Python worker (e.g., PyTorch/TensorFlow model)
    const worker = ParentWorker.spawn('./ml_worker.py', 'python');
    const handle = worker.handle();
    await handle.start();

    // Call your ML model
    const prediction = await handle.call.predict([0.1, 0.2, 0.3]);
    console.log('Prediction:', prediction);

    await handle.stop();
}

main().catch(console.error);
```

```python
# ml_worker.py - Python ML worker
from multifrost import ChildWorker
import torch  # or tensorflow, sklearn, etc.

class MLWorker(ChildWorker):
    def predict(self, features: list) -> list:
        # Your ML inference here
        tensor = torch.tensor(features)
        result = self.model(tensor)
        return result.tolist()

MLWorker().run()
```

### Go: Spawn a Worker

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/multifrost/golang"
)

func main() {
    // Spawn a Go child worker
    worker := multifrost.Spawn("examples/math_worker", "go", "run")
    handle := worker.Handle()

    if err := handle.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer handle.Stop()

    ctx := context.Background()

    // Call remote functions
    result, err := handle.Call(ctx, "Add", 5, 3)
    if err != nil {
        log.Printf("Add failed: %v", err)
    } else {
        fmt.Printf("Add(5, 3) = %v\n", result)
    }
}
```

### Rust: Spawn a Compiled Worker

```rust
use multifrost::{ParentWorker, ParentWorkerBuilder, call};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Rust workers are compiled binaries, not .rs files
    let current_dir = env::current_dir()?;
    let worker_path = current_dir
        .join("target")
        .join("debug")
        .join("examples")
        .join("math_worker");

    let worker = ParentWorkerBuilder::spawn("", worker_path.to_str().unwrap())
        .build()
        .await?;

    let handle = worker.handle();
    handle.start().await?;

    // Use the call! macro for ergonomic calls
    let result: i64 = handle.call!(add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    handle.stop().await;
    Ok(())
}
```

---

## 🔗 Connect Mode (Service Discovery)

Connect to a long-running worker instead of spawning one. Multiple parents can share the same worker.

### Python: Connect to a Service

```python
# Terminal 1: Start the service
# python math_worker_service.py
```

```python
# math_worker_service.py - Long-running worker
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="math-service")  # Register with ID

    def add(self, a: int, b: int) -> int:
        return a + b

MathWorker().run()
```

```python
# Terminal 2: Connect and call
import asyncio
from multifrost import ParentWorker

async def main():
    worker = ParentWorker.connect("math-service", timeout=5.0)
    handle = worker.handle()
    await handle.start()

    result = await handle.call.add(5, 3)
    print(f"5 + 3 = {result}")

    await handle.stop()

asyncio.run(main())
```

### Node.js: Connect to a Service

```typescript
// Terminal 1: Start the service
// npx tsx math_worker_service.ts
```

```typescript
// math_worker_service.ts
import { ChildWorker } from './src/multifrost.js';

class MathWorker extends ChildWorker {
    constructor() {
        super("math-service");  // Register with ID
    }

    add(a: number, b: number): number {
        return a + b;
    }
}

new MathWorker().run();
```

```typescript
// Terminal 2: Connect and call
import { ParentWorker } from 'multifrost';

const worker = ParentWorker.connect("math-service", 5000);
const handle = worker.handle();
await handle.start();

const result = await handle.call.add(5, 3);
console.log(`5 + 3 = ${result}`);

await handle.stop();
```

### Rust: Connect to a Service

```rust
// Terminal 1: Start the service
// cargo run --example math_worker -- --service
```

```rust
// Terminal 2: Connect and call
use multifrost::{ParentWorker, call};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let worker = ParentWorker::connect("math-service", 5000).await?;
    let handle = worker.handle();
    handle.start().await?;

    let result: i64 = handle.call!(add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    handle.stop().await;
    Ok(())
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

4. **Protocol Version**: Uses `comlink_ipc_v4` app ID for version negotiation

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

Multifrost is a monorepo. Each runtime lives in its own subdirectory:

- `python/` - Python package
- `javascript/` - JavaScript / TypeScript package
- `golang/` - Go module
- `rust/` - Rust crate

That means installation depends on which runtime you want to use. If you only need one language, you work from that language's folder rather than treating the repo root as a single universal package.

### Python (`python/`)

Install the Python package directly from the Python subdirectory:

```bash
# From a local clone
pip install ./python

# Or from Git
pip install "git+https://github.com/lirrensi/Multifrost.git#subdirectory=python"
uv pip install "git+https://github.com/lirrensi/Multifrost.git#subdirectory=python"
```

### JavaScript / TypeScript (`javascript/`)

The JavaScript implementation has its own `package.json` inside `javascript/`.

If you are working from this repository:

```bash
cd javascript
npm install
```

If you want to use the checked-out package from another local project, point npm at that folder:

```bash
npm install ../Multifrost/javascript
```

Current package name: `multifrost`

### Go (`golang/`)

The Go implementation is a Go module rooted in `golang/` with module path `github.com/multifrost/golang`.

If you are using this repo checkout directly:

```bash
cd golang
go build ./...
go test ./...
```

In Go code, import the module path declared by `golang/go.mod`:

```go
import multifrost "github.com/multifrost/golang"
```

### Rust (`rust/`)

The Rust implementation is a crate rooted in `rust/`.

If you are working from this repo checkout:

```bash
cd rust
cargo build
cargo test
```

To use it from another local Rust project during development, add a path dependency:

```toml
[dependencies]
multifrost = { path = "../Multifrost/rust" }
```

### Why not one root install command?

Because this repository contains multiple language-specific packages, not one package manager format shared by every ecosystem. Python packaging, npm packaging, Go modules, and Rust crates each expect their own manifest in their own directory.

- Python resolves `python/pyproject.toml`
- npm resolves `javascript/package.json`
- Go resolves `golang/go.mod`
- Cargo resolves `rust/Cargo.toml`

So if you only want one implementation, you do not need the whole repo layout in your build tool - just the subdirectory for that language, or a local clone that points at it.

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
