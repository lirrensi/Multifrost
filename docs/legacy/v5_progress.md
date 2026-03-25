# Multifrost v5 Progress Tracker

Status: Working tracker
Purpose: Keep the v5 rewrite state visible at a glance

## Where We Are

- `docs/product.md` has been rewritten as the v5 product canon
- `docs/spec.md` has been rewritten as the v5 behavioral canon
- the v5 router crate has been created in `router/`
- the router exists and already has initial test coverage, but coverage still needs hardening
- the Rust v5 library implementation exists and has end-to-end coverage against the router
- the Python package has been rewritten to the v5 surface, and its v5 test suite has been verified
- the JavaScript package has been rewritten to the v5 surface, and its v5 test suite has been verified
- the Go package has been rewritten to the v5 surface, and its v5 test suite has been verified

## Current Phase

We are in the first implementation phase after the canon rewrite:

1. lock the v5 headcanon
2. build the router
3. port one language end to end
4. use that result to guide the remaining language rewrites
5. harden the system with deeper tests

## Big Picture Checklist

### Canon

- [x] Rewrite `docs/product.md` for v5
- [x] Write `docs/spec.md` from scratch for v5
- [x] Rewrite `docs/arch.md` for the real v5 implementation shape after the first language port is proven
- [x] Add `docs/arch_router.md` for the concrete router architecture
- [ ] Mark `docs/protocol.md` as legacy v4 reference or retire it from the canon path

### Router

- [x] Create the Rust router in `router/`
- [ ] Expand router test coverage to cover the full v5 contract
- [x] Confirm router behavior against registration, query, routing, disconnect, duplicate registration, and body preservation
- [x] Add stronger end-to-end verification against at least one real language client

### Rust v5 Port

- [x] Replace the Rust v4 transport/runtime with the v5 router model
- [x] Convert Rust caller flow to caller-peer behavior over WebSocket
- [x] Convert Rust service flow to service-peer behavior over WebSocket
- [x] Add Rust-side router bootstrap behavior
- [x] Rewrite Rust examples and tests for v5
- [x] Confirm one full Rust caller -> router -> Rust service round-trip

### Other Language Ports

- [x] Rewrite Python to v5
- [x] Add Python v5 tests
- [x] Rewrite JavaScript to v5
- [x] Add JavaScript v5 tests
- [x] Rewrite Go to v5
- [x] Add Go v5 tests
- [ ] Decide Rust library v5 scope after the first successful port stabilizes

### End-to-End Coverage

- [x] Add router <-> Rust end-to-end tests
- [x] Add cross-language end-to-end tests for Rust <-> Python
- [x] Add cross-language end-to-end tests for Rust <-> JavaScript
- [x] Add cross-language end-to-end tests for Rust <-> Go
- [x] Add service presence and query tests across languages
- [ ] Add disconnect and re-register tests across languages
- [ ] Add malformed frame and router error tests across languages
- [ ] Add body preservation tests across languages

## Immediate Priorities

1. harden router tests further
2. harden Rust tests further
3. pick the next language port
4. port the next language to the same v5 model
5. expand cross-language end-to-end coverage

## Known Gaps Right Now

- router exists, but test coverage still needs to be broadened for the full v5 contract
- Rust v5 exists, but its coverage can still be hardened further
- the JavaScript v5 surface is rewritten and its Step 9 test pass has now been proven out
- the other languages are still on the old model
- cross-language end-to-end coverage does not exist yet

## Definition Of "V5 Is Real"

We should treat v5 as truly established only when all of the following are true:

- router behavior is covered by focused tests
- one language port works end to end against the router
- the remaining languages are ported to the same spec
- cross-language end-to-end tests pass
- architecture docs describe the actual v5 implementation rather than the retired v4 shape

## Related Files

- `docs/product.md`
- `docs/spec.md`
- `docs/arch.md`
- `docs/arch_router.md`
- `docs/v5_rewrite_plan.md`
- `agent_chat/plan_router-v5_2026-03-23.md`
- `agent_chat/plan_rust-v5-port_2026-03-23.md`
