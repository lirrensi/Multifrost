# Multifrost Python

Python implementation of Multifrost IPC library.

## Installation

```bash
pip install multifrost
```

Or for development:

```bash
cd python
pip install -e .
```

## Quick Start

```python
from multifrost import ParentWorker

# Create a worker
worker = ParentWorker("worker_script.py")
worker.start()

# Call functions as if they were local
result = worker.proxy.add(2, 3)

worker.close()
```

## API

See [API Reference](../docs/api-reference.md) for detailed documentation.

## Examples

See [examples/python](../examples/python/) for more examples.

## Legacy Support

The legacy v1 implementation is available in the `legacy/` directory for backward compatibility.
