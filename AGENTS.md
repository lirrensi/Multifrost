# Multifrost AGENTS Guide

## Build & Test Commands

### Root-level Commands
```bash
make install          # Install all language implementations
make test             # Run all tests
make clean            # Clean build artifacts
```

### Python
```bash
make install-python   # Install Python dependencies
make test-python      # Run Python tests
python -m pytest tests/ -v
```

### JavaScript
```bash
make install-javascript  # Install JavaScript dependencies
make test-javascript     # Run JavaScript tests
npm test
```

## Code Style Guidelines

### Import Organization
- **Standard library**: Import first
- **Third-party**: Import second
- **Local**: Import last
- Group imports alphabetically within each section

### Formatting
- **Python**: Use `black` formatting with `line-length = 88`
- **JavaScript**: Use `prettier` with default settings
- **Go**: Use `gofmt` (automatic, no config needed)
- **Rust**: Use `rustfmt` with default settings

### Type Safety
- **Python**: Prefer `typing` module for type hints, use `mypy` for static checking
- **JavaScript**: Use TypeScript for type safety, enable strict mode
- **Go**: Leverage interface types, avoid unnecessary pointers
- **Rust**: Use `rustc` type checking, prefer `Result<T, E>` over `Option<T>` for error handling

### Naming Conventions
- **Python**: `snake_case` for functions/variables, `PascalCase` for classes
- **JavaScript**: `camelCase` for functions/variables, `PascalCase` for classes (TypeScript)
- **Go**: `PascalCase` for exported names, `camelCase` for private names
- **Rust**: `snake_case` for variables/functions, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants

### Error Handling
- **Python**: Use exceptions, prefer `raise ValueError` for invalid inputs, `raise RuntimeError` for unexpected errors
- **JavaScript**: Use `throw new Error()` with descriptive messages
- **Go**: Use `errors.New()` or `fmt.Errorf()` with `%w` for wrapping, prefer `errors.Is()` for checking
- **Rust**: Use `Result<T, E>` and `unwrap()` cautiously; prefer `?` operator for propagating errors

### Documentation
- **Python**: Use docstrings with Google or NumPy style for public APIs
- **JavaScript**: Use JSDoc comments for public APIs
- **Go**: Use Go documentation comments with `//` prefix
- **Rust**: Use Rust documentation comments with `///` for items and `//!` for modules

## Architecture

Multifrost is a multilanguage IPC library enabling parent-child process communication using ZeroMQ. Each language implementation:

- Communicates over ZeroMQ DEALER (parent) / ROUTER (child) socket pairs
- Uses msgpack for message serialization
- Supports both spawn and connect modes for process management
- Provides language-specific async/sync APIs based on the language's paradigms

The core architecture is defined in `docs/arch.md`. Each language implementation has its own architecture documentation in `python/docs/arch.md`, `javascript/docs/arch.md`, `golang/docs/arch.md`, and `rust/docs/arch.md`.
