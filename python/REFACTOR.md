# Refactored Comlink IPC v3 - Async-First Architecture

## What Changed?

The library has been completely refactored to eliminate sync/async code duplication while maintaining both APIs.

### Key Improvements

1. **Async-First Core**: All internal logic is now async
2. **Unified Call Path**: Single `_call_internal` method handles both sync and async calls
3. **Clean Separation**: Sync API is just a thin wrapper around async core
4. **No More Threading Hell**: Event loop managed cleanly in background thread for sync usage
5. **Better Structure**: Organized into logical modules

## New Architecture

```
comlink_ipc_v2/
├── core/
│   ├── message.py       # ComlinkMessage and MessageType
│   ├── async_worker.py  # ParentWorker (async-first)
│   ├── child.py         # ChildWorker
│   └── sync_wrapper.py  # SyncProxy, AsyncProxy, SyncWrapper
└── __init__.py          # Public API
```

## API Usage

### Async API (Recommended)

```python
import asyncio
from comlink_ipc_v2 import ParentWorker

async def main():
    # Create worker
    worker = ParentWorker('worker.py')

    # Start
    await worker.start()

    # Call methods
    result = await worker.acall.add(1, 2)
    print(result)  # 3

    # With options
    result = await worker.acall.with_options(timeout=5).slow_method()

    # Cleanup
    await worker.close()

# Or use context manager
async with ParentWorker('worker.py') as worker:
    await worker.start()
    result = await worker.acall.my_function()
```

### Sync API (Convenience Wrapper)

```python
from comlink_ipc_v2 import ParentWorker

# Create worker
worker = ParentWorker('worker.py')

# Start synchronously
worker.call.start()

# Call methods
result = worker.call.add(1, 2)
print(result)  # 3

# With options
result = worker.call.with_options(timeout=5).slow_method()

# Cleanup
worker.call.close()

# Or use context manager
with ParentWorker('worker.py').sync as worker:
    worker.start()
    result = worker.call.my_function()
```

## Worker Implementation (Unchanged)

```python
from comlink_ipc_v2 import ChildWorker

class MyWorker(ChildWorker):
    def add(self, a, b):
        return a + b

    async def slow_add(self, a, b):
        await asyncio.sleep(1)
        return a + b

if __name__ == '__main__':
    MyWorker().run()
```

## Migration Guide

### Old API (v2)

```python
# Sync
worker = ParentWorker('script.py')
worker.start()
result = worker.proxy.add(1, 2)  # OLD: .proxy
worker.close()

# Async
worker = ParentWorker('script.py')
await worker.astart()
result = await worker.aproxy.add(1, 2)  # OLD: .aproxy
await worker.aclose()
```

### New API (v3)

```python
# Sync
worker = ParentWorker('script.py')
worker.call.start()  # NEW: .call.start()
result = worker.call.add(1, 2)  # NEW: .call
worker.call.close()

# Async
worker = ParentWorker('script.py')
await worker.start()  # NEW: direct await
result = await worker.acall.add(1, 2)  # NEW: .acall
await worker.close()
```

## Key Differences

| Feature | v2 (Old) | v3 (New) |
|---------|----------|----------|
| Core architecture | Mixed sync/async | Async-first |
| Call methods | `_call_function` + `_acall_function` | Single `_call_internal` |
| Thread management | Multiple threads | Clean event loop thread |
| Proxy naming | `.proxy` / `.aproxy` | `.call` / `.acall` |
| Code duplication | High (separate paths) | None (unified) |
| Context managers | `__enter__` only | `__aenter__` + sync wrapper |

## Benefits

1. **Maintainability**: One code path to maintain instead of two
2. **Consistency**: Both APIs use the same underlying logic
3. **Performance**: No unnecessary thread pool overhead
4. **Clarity**: Clear separation between async core and sync wrapper
5. **Testability**: Easier to test with single code path

## Testing

Run the test suite:

```bash
python comlink_ipc_v2/test_refactored.py
```

This tests:
- Async API with various method types
- Sync API with various method types
- Context manager usage
- Options chaining
- Error handling

## Backward Compatibility

⚠️ **Breaking Changes**:

- `.proxy` → `.call` (sync)
- `.aproxy` → `.acall` (async)
- `.start()` / `.close()` remain same
- `.astart()` / `.aclose()` removed (use direct await)

The worker API (`ChildWorker`) remains **100% compatible** - no changes needed!

## Future Improvements

- [ ] Add streaming support
- [ ] Add request cancellation
- [ ] Add connection pooling
- [ ] Add metrics/monitoring
- [ ] Better error recovery
