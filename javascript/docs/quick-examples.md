# Multifrost JavaScript v5 Quick Examples

## Caller

```ts
import { connect } from "multifrost";

const connection = connect("math-service", {
  eagerTargetQuery: "exists",
});

const handle = connection.handle();
await handle.start();
console.log(await handle.call.multiply(6, 7));
await handle.stop();
```

## Service

```ts
import { runService, ServiceContext, ServiceWorker } from "multifrost";

class MathService extends ServiceWorker {
  async add(a: number, b: number): Promise<number> {
    return a + b;
  }
}

await runService(new MathService(), new ServiceContext({ peerId: "math-service" }));
```

## Spawn Then Connect

```ts
import { connect, spawn } from "multifrost";
import { fileURLToPath } from "url";

const serviceEntrypoint = fileURLToPath(new URL("../examples/math_worker_service.ts", import.meta.url));
const service = await spawn(serviceEntrypoint, `npx tsx ${serviceEntrypoint}`);

try {
  const connection = connect("math-service");
  const handle = connection.handle();
  await handle.start();
  console.log(await handle.call.add(2, 3));
  await handle.stop();
} finally {
  await service.stop();
}
```
