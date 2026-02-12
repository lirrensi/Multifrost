# Multifrost 🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈🏳️‍🌈

> **"Bifrost but more powerful!"** - A rainbow bridge for your environments across spaces. Microservices made easy. Ditch making REST API connectors everywhere!

Multifrost is a high-performance inter-process communication (IPC) library that enables seamless communication between processes written in different languages. Think of it as a language-agnostic bridge that lets Python talk to Node.js, Go talk to Rust, and everything in between—without writing a single REST API or gRPC endpoint.

> Previously was **comlink_py** - you can find old version in __python/legacy/__

## ✨ Why Multifrost?

- **Zero-API, Zero-Latency**: Call remote functions like they're local. No HTTP overhead, no serialization complexity.
- **Cross-Language**: Python ↔ Node.js ↔ Go ↔ Rust ↔ Anything with a runtime
- **Two Modes**: Spawn (create workers) or Connect (service discovery)
- **Battle-Tested**: Built on ZeroMQ and msgpack for maximum performance
- **Async-Native**: Modern async/await APIs in all languages

## 🚀 Quick Start

### Python
```bash
pip install multifrost
```

### JavaScript / TypeScript
```bash
npm install multifrost
```

### Go
```bash
go get github.com/multifrost/golang
```

### Rust
```bash
cargo install https://github.com/multifrost/rust
```

### uvx (Python)
```bash
uvx multifrost
```

## 💡 Common Use Cases

### Bridge Two Python Environments
```python
# In your main app (parent)
from multifrost import ParentWorker

worker = ParentWorker.spawn("worker.py")
worker.start()

# Call functions from the isolated environment
result = worker.call.process_data(json_data)
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
    "github.com/multifrost/golang"
)

func main() {
    worker, _ := golang.SpawnWorker("./service.go", nil)
    worker.Start()

    // Call the other service
    result, _ := worker.Call("process", []interface{}{data})
    fmt.Println(result)
}
```

### Offload Heavy Compute to Rust
```rust
// In your Python parent
worker = ParentWorker.spawn("./heavy_worker.rs", "cargo");
result = await worker.call.compute_heavy_matrix(matrix_data)
```

```rust
// heavy_worker.rs
use multifrost::ChildWorker;

pub struct HeavyWorker;

impl WorkerMethods for HeavyWorker {
    fn compute_heavy(&self, data: Vec<f64>) -> Vec<f64> {
        // CPU-intensive computation in Rust
        data.iter().map(|x| x * 2.0).collect()
    }
}

fn main() {
    HeavyWorker::run();
}
```

## 📚 Documentation

- **[Architecture](docs/arch.md)** - Core design principles and protocol
- **[Protocol](docs/protocol.md)** - Message format and wire protocol
- **[MsgPack Interop](docs/msgpack_interop.md)** - Cross-language data handling
- **[Get Started](examples/)** - Full examples for each language

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
pip install multifrost

# Using uv
uv pip install multifrost

# Using uvx (direct run)
uvx multifrost
```

### JavaScript / TypeScript
```bash
npm install multifrost

# Or use with tsx for TypeScript files
npx tsx worker.ts
```

### Go
```bash
go get github.com/multifrost/golang
```

### Rust
```bash
cargo install https://github.com/multifrost/rust
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

- **[Python Examples](python/examples/)** - Parent-child patterns, async workers
- **[JavaScript Examples](javascript/examples/)** - Node.js workers, TypeScript
- **[Go Examples](golang/examples/)** - Microservice patterns
- **[Rust Examples](rust/examples/)** - Heavy compute, async patterns

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
