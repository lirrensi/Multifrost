// FILE: golang/multifrost.go
// PURPOSE: Define the Go v5 package surface and point readers to the canonical router-based API.
// OWNS: Package documentation, Version constant.
// EXPORTS: Version.
// DOCS: docs/spec.md, docs/arch.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

// Package multifrost is the Go binding for the Multifrost v5 router-based IPC runtime.
//
// The Go surface is synchronous and context-driven:
//
//   - `Connect` returns caller configuration.
//   - `Connection.Handle` opens the live caller transport.
//   - `Handle.Call` performs blocking remote calls across the router.
//   - `Spawn` starts a service process.
//   - `RunService` registers a service peer and dispatches calls through `ServiceWorker.HandleCall`.
//
// Go does not expose async naming in its public API. Concurrency comes from
// goroutines around blocking calls, which keeps the surface idiomatic while the
// router handles presence and routing.
const Version = "5.0.0"
