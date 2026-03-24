# Multifrost Python Agent Guide

## Build And Test

```bash
cd python
uv sync --all-groups
uv run ruff check src tests
uv run mypy src
uv run pytest tests -q
```

## Architecture Rules

- The public caller API is `connect`, `Connection`, `Handle`, and `HandleSync`.
- The public service API is `spawn`, `ServiceProcess`, `ServiceWorker`, `ServiceContext`, `run_service`, and `run_service_sync`.
- Python keeps one async core and one sync convenience layer.
- `HandleSync` must stay a thin wrapper over the async handle, not a separate transport.
- `ServiceWorker` is only a method host; it does not own transport or process lifecycle.
- Service methods may be synchronous or asynchronous public methods.
- The live transport is WebSocket only.
- The router is bootstrapped lazily and shared across callers and services.

## Coding Rules

- Keep imports ordered: standard library, third-party, local.
- Use `black`-compatible formatting with the repo line length.
- Type hint public APIs.
- Prefer explicit `raise` statements over `assert` for runtime validation.
- Keep new source files with a lightweight file header when they are materially edited.

## Example Patterns

```python
from multifrost import connect

connection = connect("math-service")
handle = connection.handle_sync()
handle.start()
try:
    print(handle.call.add(1, 2))
finally:
    handle.stop()
```

```python
from multifrost import ServiceContext, ServiceWorker, run_service


class MathService(ServiceWorker):
    async def add(self, a: int, b: int) -> int:
        return a + b
```
