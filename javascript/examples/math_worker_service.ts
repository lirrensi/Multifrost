/**
 * FILE: javascript/examples/math_worker_service.ts
 * PURPOSE: Show a minimal service peer built from ServiceWorker and runService.
 * OWNS: Service-side method dispatch example.
 * EXPORTS: None
 * DOCS: docs/spec.md, javascript/README.md
 */

import { runService, ServiceContext, ServiceWorker } from "../src/index.js";

class MathService extends ServiceWorker {
    async add(a: number, b: number): Promise<number> {
        return a + b;
    }

    async multiply(a: number, b: number): Promise<number> {
        return a * b;
    }

    async factorial(value: number): Promise<number> {
        let result = 1;
        for (let i = 2; i <= value; i += 1) {
            result *= i;
        }
        return result;
    }
}

void runService(
    new MathService(),
    new ServiceContext({
        peerId: "math-service",
    })
).catch(error => {
    console.error(error);
    process.exitCode = 1;
});
