# Multifrost JavaScript v5 FAQ

## Why is there no synchronous API?

Node is async-first here. The package keeps routing and filesystem work off the synchronous path so callers and services stay event-loop friendly.

## How do I connect to a service?

```ts
import { connect } from "multifrost";

const connection = connect("math-service");
const handle = connection.handle();
await handle.start();
const result = await handle.call.add(2, 3);
await handle.stop();
```

## How do I run a service peer?

```ts
import { runService, ServiceContext, ServiceWorker } from "multifrost";

class MathService extends ServiceWorker {
  async add(a: number, b: number): Promise<number> {
    return a + b;
  }
}

await runService(new MathService(), new ServiceContext({ peerId: "math-service" }));
```

## Does spawn connect the caller?

No. `spawn(...)` only launches the service process and sets the entrypoint/router environment. The service still performs its own router bootstrap and registration.

## How does peer identity work?

Service peers prefer an explicit `ServiceContext.peerId`. If that is missing, the runtime falls back to `MULTIFROST_ENTRYPOINT_PATH`, then to the canonicalized `process.argv[1]`.

## Where is the router started?

The caller and service helpers use the shared router bootstrap flow. If the router is already reachable, they reuse it; otherwise they take the coordinated startup lock and start it.

## What transport is used?

WebSocket only. Binary frames only. The frame layout is `[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]`.
