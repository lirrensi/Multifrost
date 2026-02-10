# API Reference

## Python API

### ParentWorker

```python
class ParentWorker:
    def __init__(self, script_path: str, **kwargs)
    def start(self) -> None
    def close(self) -> None
    async def astart(self) -> None
    async def aclose(self) -> None
```

### ChildWorker

```python
class ChildWorker:
    def run(self) -> None
```

## JavaScript API

### ParentWorker

```typescript
class ParentWorker {
    constructor(scriptPath: string, options?: WorkerOptions)
    start(): Promise<void>
    close(): Promise<void>
}
```

### ChildWorker

```typescript
class ChildWorker {
    run(): void
}
```

## Cross-Language Communication

Multifrost uses ZeroMQ and msgpack for efficient, cross-language IPC. Both Python and JavaScript implementations can communicate with each other seamlessly.

## Message Format

Messages are serialized using msgpack format and transmitted over ZeroMQ sockets. This ensures:

- Binary efficiency
- Language-agnostic serialization
- Support for complex data types

## Error Handling

Errors are propagated across process boundaries with full stack traces when possible.
