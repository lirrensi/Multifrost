# MessagePack Interop Stability Guide

Quick reference for cross-language MessagePack serialization. Covers Python, JavaScript, Go, and Rust.

---

## Critical Configuration (Per Language)

| Language | Must Enable | Why |
|----------|-------------|-----|
| **Python** | `use_bin_type=True`, `raw=False` | Distinguishes bytes from strings; decodes strings as `str` not `bytes` |
| **JavaScript** | `useBigInt64: true` | Prevents truncation of integers >2^53 |
| **Go** | Struct tags (`msgpack:"field"`) | Field name matching; use `*T` for optional fields |
| **Rust** | `#[serde(default)]` | Handles missing fields from dynamic languages |

---

## Top 9 Gotchas

1. **String vs Binary** — Python `bytes` without `use_bin_type=True` serialize as strings → Go/Rust crash on UTF-8 decode
2. **Integer Overflow** — Python arbitrary precision (`2**100`) → crashes Go/Rust `int64`
3. **Non-String Map Keys** — `{1: "x"}` works in Python → breaks JS (auto-stringifies), confuses typed languages
4. **NaN/Infinity** — Inconsistent handling across languages; convert to `null`
5. **Optional Fields** — Go zero-values hide "missing" vs "default"; use pointers `*T`
6. **Unknown Fields** — Rust errors by default; add `#[serde(default)]` or `#[serde(deny_unknown_fields)]`
7. **Timestamps** — Use Ext type -1 (native), not ISO strings
8. **Size/Depth Limits** — Set `max_*_len` in Python; prevent DoS and stack overflow
9. **interface{} Boxing** — msgpack decodes arrays as `[]interface{}` and maps as `map[string]interface{}` in Go → requires unwrapping in typed method parameters

---

## Safe Type Subset

**Always Safe:**
- Strings (UTF-8)
- Integers in range `[-2^63, 2^63-1]`
- Floats (no NaN/Infinity)
- `bool`, `null`
- Arrays, objects with string keys only
- Binary data (with proper config)

**Avoid or Handle Carefully:**
- Non-string map keys
- Integers >2^63
- `NaN`, `Infinity`
- Circular references
- Python tuples as keys
- Rust enums (complex ADT format)
- Mixed-type arrays without schema

---

## Auto-Healing Strategy (Bridge/RPC)

| Issue | Action |
|-------|--------|
| NaN/Infinity | → `null` (silent) |
| Integer >2^63 | → clamp to int64 max |
| Non-string key | → stringify |
| Circular ref / depth >100 | → **ERROR** (hard limit) |
| Collection >100k items | → **ERROR** (DoS protection) |
| interface{} boxing (arrays/maps) | → unwrap underlying values automatically |

---

## Minimal Safe Config

### Python
```python
packed = msgpack.packb(data, use_bin_type=True)

unpacked = msgpack.unpackb(
    packed,
    raw=False,
    max_bin_len=10*1024*1024,
    max_str_len=10*1024*1024,
    max_array_len=100_000,
    max_map_len=100_000,
)
```

### JavaScript
```javascript
import { encode, decode } from '@msgpack/msgpack';

const encoded = encode(data, { useBigInt64: true });
const decoded = decode(encoded, { useBigInt64: true });
```

### Go
```go
type Config struct {
    Name string `msgpack:"name"`
    Age  *int   `msgpack:"age,omitempty"` // pointer for optional
}

var result Config
err := msgpack.Unmarshal(data, &result)
```

### Rust
```rust
#[derive(Serialize, Deserialize)]
struct Config {
    name: String,
    #[serde(default)]
    age: Option<i32>,
}

let decoded: Config = rmp_serde::from_slice(&data)?;
```

---

## Type Compatibility Matrix

| Problem | Python Fix | JavaScript Fix | Go Fix | Rust Fix |
|---------|-----------|----------------|---------|----------|
| Binary vs String | `use_bin_type=True` | Use `Uint8Array` | Use `[]byte` | Use `Vec<u8>` |
| Large Integers | Limit to ±2^63 | `useBigInt64: true` | Use `int64` | Use `i64` |
| Optional Fields | Don't send if `None` | `ignoreUndefined: true` | Use `*T` + `omitempty` | Use `Option<T>` |
| Map Keys | Use only strings | Objects = string keys only | Use `map[string]T` | Use `HashMap<String, T>` |
| Unknown Fields | Ignore them | Ignore them | Ignore by default | `#[serde(default)]` |
| NaN/Infinity | Avoid or use `null` | Convert to `null` | Handle as `null` | Handle as `None` |
| Timestamps | `datetime=True` | Manual encoding | Use `time.Time` | Use `chrono` crate |
| Array/Map Boxing | N/A (native) | N/A (native) | Use `unwrapInterface()` helper | Use `Option<T>` for nullable |
| Float for Integer | Avoid | Avoid | Automatic conversion | Avoid |

---

## Interface{} Boxing in Go (Critical for Cross-Language IPC)

When msgpack decodes data in Go, it uses `interface{}` (or `any`) as the default type for dynamic collections:

```go
// Python sends: [1, 2, 3]
// Go msgpack decodes to: []interface{} containing int64 values
// Each element is: interface{}(int64(1)), not int64(1) directly

// Python sends: {"a": 1, "b": 2}
// Go msgpack decodes to: map[string]interface{}{
//     "a": interface{}(int64(1)),
//     "b": interface{}(int64(2)),
// }
```

**This causes problems when Go methods expect typed collections:**

```go
// ❌ Won't work - Go panics when trying to set interface{} into int
func (w *Worker) Sum(arr []int) int {  // Expects []int
    sum := 0
    for _, v := range arr {  // v is interface{}, not int
        sum += v  // panic: interface {} is not int
    }
    return sum
}

// ✅ Works - uses interface{} for dynamic input
func (w *Worker) Sum(arr []interface{}) int {
    sum := 0
    for _, v := range arr {
        sum += int(v.(int64))  // Type assertion needed
    }
    return sum
}
```

### The Fix: Automatic Unwrapping

Go implementations should unwrap `interface{}` values before type conversion:

```go
// Helper to unwrap interface{} values from msgpack
func unwrapInterface(value reflect.Value) reflect.Value {
    if value.Kind() == reflect.Interface && value.Elem().IsValid() {
        return value.Elem()
    }
    return value
}

// Use in convertArg, convertSlice, and convertMap
```

This allows typed method parameters to work correctly:

```go
// Now works! The conversion helper unwraps interface{} → int64 → int
func (w *Worker) Sum(arr []int) int {
    sum := 0
    for _, v := range arr {
        sum += v  // Works: v is int
    }
    return sum
}
```

### Summary: What Works Across Languages

| Go Method Parameter | Python Sends | Works? | Notes |
|---------------------|--------------|--------|-------|
| `int` | `42` | ✅ | int64 → int conversion |
| `int64` | `42` | ✅ | Direct match |
| `float64` | `3.14` | ✅ | Direct match |
| `string` | `"hello"` | ✅ | Direct match |
| `bool` | `true` | ✅ | Direct match |
| `interface{}` | `42` | ✅ | Returns int64 |
| `[]int` | `[1, 2, 3]` | ✅ | Now works with unwrap |
| `[]string` | `["a", "b"]` | ✅ | Now works with unwrap |
| `map[string]int` | `{"a": 1}` | ✅ | Now works with unwrap |
| `map[string]any` | `{"a": 1}` | ✅ | Best for dynamic data |

---

## Testing Checklist

Create a torture test payload and verify round-trips across all language pairs:

```json
{
  "version": 1,
  "empty_string": "",
  "unicode": "Hello 世界",
  "binary": "<0x00, 0xFF>",
  "zero": 0,
  "negative": -42,
  "max_int64": 9223372036854775807,
  "min_int64": -9223372036854775808,
  "tiny_float": 0.0000001,
  "true": true,
  "false": false,
  "null": null,
  "empty_array": [],
  "empty_object": {},
  "nested": { "level1": { "level2": { "level3": "deep" } } },
  "mixed_array": [1, "two", 3.0, null, true]
}
```

**Test all 12 language pair combinations:**
- Python → Go → Python
- Python → JS → Python
- Python → Rust → Python
- Go → Rust → Go
- etc.

---

## Production Checklist

- [ ] Binary type distinction enabled (`use_bin_type=True` in Python)
- [ ] String decoding enabled (`raw=False` in Python)
- [ ] BigInt support enabled (JavaScript)
- [ ] Struct tags defined (Go)
- [ ] Optional fields use `Option`/pointers (Rust/Go)
- [ ] Size limits configured (prevent DoS)
- [ ] Timestamp handling standardized (use ext type -1)
- [ ] Schema version field included in all messages
- [ ] Round-trip tests pass for all language pairs
- [ ] Error handling implemented for all deserialize paths
