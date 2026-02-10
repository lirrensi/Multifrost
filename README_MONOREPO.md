# Multifrost

**Multifrost** is a lightweight, zero-boilerplate IPC (inter-process communication) library for Python and JavaScript/Node.js inspired by [comlink.js](https://github.com/GoogleChromeLabs/comlink).

It lets you **spawn and control worker processes** (even from different Python virtual environments or Node.js versions) through sync and async proxy objects—like calling regular functions!

## Features

- 🔗 **Cross-language IPC**: Python ↔ Node.js communication using ZeroMQ + msgpack
- 🧠 **Isolate dependencies**: Run each process in its own virtual environment or language runtime
- 💥 **No REST, sockets, or "multiprocessing" hackery** needed for simple synchronous and asynchronous calls
- 🦾 **Dead-simple call syntax**: Just import your worker as a regular object!
- 🔬 Great for experimenting with models/tools that require different CUDA/drivers/Python versions on the same system
- 💡 Useful for mixed environments (Windows, Linux) in modular/monolithic codebases
- 💡 See print()s directly from your parent script - useful for debug

## Quick Start

### Python

```bash
cd python
pip install -e .
```

```python
from multifrost import ParentWorker

# Create a worker
worker = ParentWorker("worker_script.py")
worker.start()

# Call functions as if they were local
result = worker.proxy.add(2, 3)

worker.close()
```

### JavaScript/Node.js

```bash
cd javascript
npm install
```

```javascript
import { ParentWorker } from 'multifrost';

const worker = new ParentWorker('./worker.ts');
await worker.start();

const result = await worker.proxy.add(2, 3);

await worker.close();
```

## Documentation

- [Python Documentation](python/README.md)
- [JavaScript Documentation](javascript/README.md)
- [API Reference](docs/api-reference.md)
- [Examples](examples/)

## Installation

### Using pip (Python)

```bash
pip install multifrost
```

### Using npm (JavaScript)

```bash
npm install multifrost
```

### Development

```bash
# Install both packages
make install

# Run tests
make test
```

## Project Structure

```
multifrost/
├── python/                 # Python implementation
│   ├── src/multifrost/    # Main package
│   ├── legacy/            # Legacy v1 implementation
│   ├── tests/
│   └── pyproject.toml
├── javascript/            # JavaScript/TypeScript implementation
│   ├── src/
│   ├── tests/
│   └── package.json
├── docs/                  # Shared documentation
├── examples/              # Usage examples
│   ├── python/
│   └── javascript/
└── Makefile              # Top-level commands
```

## License

MIT

## Credits

Inspired by comlink.js. Written by lirrensi. PRs and improvements welcome!
