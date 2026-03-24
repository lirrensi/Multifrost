# Multifrost JavaScript

The JavaScript package is the Node v5 binding for Multifrost: a router-based RPC system where caller peers and service peers meet through the shared router.

## Install

```bash
cd javascript
npm install
```

## Quick Start

### Caller

```ts
import { connect } from "multifrost";

const connection = connect("math-service", {
  eagerTargetQuery: "exists",
});

const handle = connection.handle();
await handle.start();

const sum = await handle.call.add(2, 3);
console.log(sum);

await handle.stop();
```

### Service

```ts
import { runService, ServiceContext, ServiceWorker } from "multifrost";

class MathService extends ServiceWorker {
  async add(a: number, b: number): Promise<number> {
    return a + b;
  }
}

await runService(new MathService(), new ServiceContext({
  peerId: "math-service",
}));
```

### Launching a service process

```ts
import { spawn } from "multifrost";

const process = await spawn("./examples/math_worker_service.ts", "npx tsx ./examples/math_worker_service.ts");
console.log(process.id());
await process.stop();
```

## Model

- Callers connect to the router and use `handle.call.<method>(...)` to reach a service peer.
- Services register themselves with the router and dispatch public methods from a `ServiceWorker` subclass.
- `spawn(...)` is only a child-process launcher; it does not establish caller transport.
- `runService(...)` is the service entrypoint helper for long-running peers.

## References

- [Behavioral spec](../docs/spec.md)
- [Node architecture](./docs/arch.md)
