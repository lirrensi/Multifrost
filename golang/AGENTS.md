# Multifrost Go v5 Guide

## Mental Model

Go is the synchronous, context-driven binding for Multifrost v5.

- `Connect` returns caller configuration.
- `Connection.Handle()` opens the live caller transport.
- `Handle.Call(ctx, function, args...)` is the main caller API.
- `Spawn` starts a service process.
- `RunService` connects a service peer and dispatches calls through `ServiceWorker.HandleCall`.

## Rules

- Use `context.Context` for cancellation and timeouts.
- Keep public API names aligned with the v5 surface.
- Do not reintroduce the retired v4 worker/proxy surface.
- Do not use reflection as the primary service dispatch path.
- Do not use async naming in Go. Goroutines are fine; async surface names are not.

## Service Dispatch

Service implementations should be tiny and explicit:

```go
type MathService struct{}

func (s *MathService) HandleCall(ctx context.Context, function string, args []any) (any, error) {
	switch function {
	case "add":
		return 30, nil
	default:
		return nil, fmt.Errorf("function not found: %s", function)
	}
}
```

## Process Notes

- `Spawn` only starts the child process.
- `ServiceProcess` owns process lifecycle, not network state.
- `RunService` owns registration and dispatch on the service side.

## Validation

From `golang/`, the usual checks are:

```bash
go fmt ./...
go test ./...
go vet ./...
```
